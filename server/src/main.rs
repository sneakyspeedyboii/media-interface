#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(target_os = "windows")]
use anyhow::{Error, Result};
use axum::routing::Router as AxumRouter;
use base64::{
    alphabet::STANDARD,
    engine::{GeneralPurpose, GeneralPurposeConfig},
    Engine,
};
use futures::{SinkExt, StreamExt};

use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch, watch::Receiver},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;
use tower_http::services::ServeDir;
use tracing::{event, Level};
use tray_icon::{menu::MenuEvent, ClickEvent, TrayEvent, TrayIconBuilder};
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties,
    },
    Storage::Streams::DataReader,
};
use winit::event_loop::{ControlFlow, EventLoopBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    let log_writer = tracing_appender::rolling::never("./assets", "log.txt");
    let tracing = tracing_subscriber::fmt()
        .with_line_number(true)
        .with_file(true)
        .with_ansi(false)
        .with_max_level(Level::DEBUG)
        .with_writer(log_writer)
        .finish();

    tracing::subscriber::set_global_default(tracing)?;
    event!(Level::INFO, "Starting");

    let config: Config = toml::from_str(&std::fs::read_to_string("./assets/config.toml")?)?;

    let website = format!("{}:{}", config.ip, config.port).parse::<SocketAddr>()?;
    let socket = format!("{}:{}", config.ip, config.port + 1).parse::<SocketAddr>()?;

    let cancel = CancellationToken::new();
    tokio::spawn(start_server(cancel.clone(), website, socket));

    let mut state = Some(cancel);

    let event_loop = EventLoopBuilder::new().build();

    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open("./assets/icon.png")?.into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    let icon = tray_icon::icon::Icon::from_rgba(icon_rgba, icon_width, icon_height)?;

    let _tray_icon = TrayIconBuilder::new()
        .with_tooltip("Media Interface")
        .with_icon(icon)
        .build()?;

    let _menu_channel = MenuEvent::receiver();
    let tray_channel = TrayEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        if let Ok(event) = tray_channel.try_recv() {
            if event.event == ClickEvent::Left {
                if let Some(cancel_token) = state.take() {
                    cancel_token.cancel();
                } else {
                    let cancel = CancellationToken::new();
                    match read_config() {
                        Ok((website, socket)) => {
                            tokio::spawn(start_server(cancel.clone(), website, socket));
                            state = Some(cancel)
                        }
                        Err(err) => event!(Level::ERROR, "Error reading config: {}", err),
                    }
                }
            }
        }
    });
}

fn read_config() -> Result<(SocketAddr, SocketAddr)> {
    let config: Config = toml::from_str(&std::fs::read_to_string("./assets/config.toml")?)?;

    let website = format!("{}:{}", config.ip, config.port).parse::<SocketAddr>()?;
    let socket = format!("{}:{}", config.ip, config.port + 1).parse::<SocketAddr>()?;

    Ok((website, socket))
}

async fn start_server(
    parent_cancel_token: CancellationToken,
    site: SocketAddr,
    socket: SocketAddr,
) -> Result<()> {
    let listener = TcpListener::bind(socket).await?;

    //Socket
    let child_cancel_token = parent_cancel_token.clone();
    let socket_server = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            event!(
                Level::INFO,
                "New socket connection: {}",
                stream.peer_addr()?
            );
            tokio::spawn(handle_connection(stream, child_cancel_token.clone()));
        }
        Ok::<(), Error>(())
    });
    let socket_abort = socket_server.abort_handle();
    event!(Level::INFO, "Socket server started");

    //Website
    let website_cancel_token = parent_cancel_token.clone();
    let website = tokio::spawn(async move {
        let router: AxumRouter =
            AxumRouter::new().nest_service("/", ServeDir::new("./assets/site"));
        axum::Server::bind(&site)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async {
                website_cancel_token.cancelled().await;
            })
            .await?;
        Ok::<(), Error>(())
    });
    event!(Level::INFO, "Web Server started");

    tokio::select! {
        _ = website => {
            parent_cancel_token.cancel();
        },
        _ = socket_server => {
            parent_cancel_token.cancel();
        },
        _ = parent_cancel_token.cancelled() => {
            socket_abort.abort();
        },
    }

    event!(Level::INFO, "Shutting down");
    Ok(())
}

async fn handle_connection(stream: TcpStream, cancel_token: CancellationToken) -> Result<()> {
    let ws = accept_async(stream).await?;
    event!(Level::INFO, "Accepted socket connection");

    let (mut ws_sender, mut ws_reciever) = ws.split();

    /*
        Cannot send the sink/stream across threads, does not implement send, copy/clone.
        Tokio's message handlers are used to interact with the websocket across threads

        MPSC for sending messages to the sink
        Watch for receiving messages from the stream

        Both spawned threads don't need to be awaited, if they finish the threads using them will panic, and cause the websocket to close, in theory.

        Oh and abort handles seems to have a different effect to just aborting the join handle, Hence all the abort handles.
        Oh yeah and this also might leak memory, but I am also too lazy to debug and fix that.
    */

    let (tx, mut rx) = mpsc::channel::<String>(10);
    let mpsc_send_handler = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            ws_sender.send(Message::Text(msg)).await?;
        }
        Ok::<(), Error>(())
    });
    event!(Level::DEBUG, "MPSC sender started");

    let (wtx, wrx) = watch::channel::<String>("init".to_string());

    let watch_receive_handler = tokio::spawn(async move {
        while let Some(Ok(message)) = ws_reciever.next().await {
            let message = message.to_text()?.to_string();
            wtx.send(message)?;
        }

        Ok::<(), Error>(())
    });
    event!(Level::DEBUG, "Watch receiver started");

    let recieve_info = tokio::spawn(recieve_info(wrx.clone()));
    event!(Level::DEBUG, "Socket receive handler started");
    let send_info = tokio::spawn(send_info(tx));
    event!(Level::DEBUG, "Socket send handler started");

    let mpsc_abort = mpsc_send_handler.abort_handle();
    let watch_abort = watch_receive_handler.abort_handle();
    let recieve_abort = recieve_info.abort_handle();
    let send_abort = send_info.abort_handle();

    let kill = || {
        recieve_abort.abort();
        send_abort.abort();
        watch_abort.abort();
        mpsc_abort.abort();
        event!(Level::DEBUG, "Aborted all handlers");
    };

    tokio::select! {
        _ = mpsc_send_handler => kill(),
        _ = watch_receive_handler => kill(),
        _ = recieve_info => kill(),
        _ = send_info => kill(),
        _ = cancel_token.cancelled() => kill()
    }

    event!(Level::INFO, "Socket connection closed");

    Ok(())
}

async fn send_info(tx: mpsc::Sender<String>) -> Result<()> {
    loop {
        let session = get_session().await?;
        let music = get_session_details(&session).await?;

        tx.send(serde_json::to_string(&music)?).await?;

        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

async fn recieve_info(mut reciever: Receiver<String>) -> Result<()> {
    while reciever.changed().await.is_ok() {
        let message = reciever.borrow_and_update().to_string();

        let session = get_session().await?;

        if message == "toggle" {
            session.TryTogglePlayPauseAsync()?;
        } else if message == "skip" {
            session.TrySkipNextAsync()?;
        } else if message == "back" {
            session.TrySkipPreviousAsync()?;
        }
    }

    Err(anyhow::anyhow!("Receiver closed"))
}

async fn get_session() -> Result<GlobalSystemMediaTransportControlsSession> {
    match GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.await {
        Ok(gsmt_session_manager) => {
            let sessions: Vec<GlobalSystemMediaTransportControlsSession> =
                gsmt_session_manager.GetSessions()?.into_iter().collect();

            if sessions.is_empty() {
                return Err(anyhow::anyhow!("No sessions found"));
            }

            for session in sessions.clone() {
                if session
                    .SourceAppUserModelId()?
                    .to_string()
                    .to_lowercase()
                    .contains("spotify.exe")
                //I like spotify prioritisation
                {
                    return Ok(session);
                }
            }

            Ok(sessions[0].to_owned())
        }
        Err(_) => Err(anyhow::anyhow!(
            "Could not get session manager (Caused by all sessions closing? not sure tbh)"
        )),
    }
}

async fn get_session_details(
    session: &GlobalSystemMediaTransportControlsSession,
) -> Result<MusicInfo> {
    let session_info = session.TryGetMediaPropertiesAsync()?.await?;
    let session_timeline = session.GetTimelineProperties()?;

    let engine = GeneralPurpose::new(&STANDARD, GeneralPurposeConfig::default());

    let start_time = Duration::from(session_timeline.StartTime()?).as_millis();
    let end_time = Duration::from(session_timeline.EndTime()?).as_millis();
    let position = Duration::from(session_timeline.Position()?).as_millis();

    let thumbnail = get_thumbnail(&session_info)?;
    Ok(
    MusicInfo {
        song_name: session_info.Title()?.to_string(),
        song_subtitle: session_info.Subtitle()?.to_string(),
        artist: session_info.Artist()?.to_string(),
        album: session_info.AlbumTitle()?.to_string(),
        album_artwork: engine.encode(thumbnail),
        start_time,
        end_time,
        position,
        playing: match session.GetPlaybackInfo()?.PlaybackStatus()? {
            windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => true,
            _ => false,
        }})
}

fn get_thumbnail(
    session_info: &GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Result<Vec<u8>> {
    let thumb = session_info.Thumbnail()?.OpenReadAsync()?.get()?;

    let stream_len = thumb.Size()? as usize;
    let mut data = vec![0u8; stream_len];
    let reader = DataReader::CreateDataReader(&thumb)?;
    reader.LoadAsync(stream_len as u32)?.get()?;

    reader.ReadBytes(&mut data)?;
    reader.Close()?;
    thumb.Close()?;

    Ok(data)
}

#[derive(Deserialize)]
struct Config {
    ip: String,
    port: u16,
}

#[derive(Serialize)]
struct MusicInfo {
    song_name: String,
    song_subtitle: String,
    artist: String,
    album: String,
    album_artwork: String,
    start_time: u128,
    end_time: u128,
    position: u128,
    playing: bool,
}
