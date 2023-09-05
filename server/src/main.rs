#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use axum::Router;
use base64::Engine;
#[cfg(target_os = "windows")]
use base64::{
    alphabet::STANDARD,
    engine::{GeneralPurpose, GeneralPurposeConfig},
};
use color_eyre::{eyre::eyre, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    time::sleep,
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use tower_http::services::ServeDir;
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties,
    },
    Storage::Streams::DataReader,
};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let config: Config = toml::from_str(&std::fs::read_to_string("./assets/config.toml")?)?;

    let config = Arc::new(config);

    let state = Arc::new(AppState {
        base64_engine: GeneralPurpose::new(&STANDARD, GeneralPurposeConfig::default()),
        gsmt_manager: GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.await?,
    });

    let site = tokio::spawn(serve_site(config.clone()));
    let socket = tokio::spawn(run_socket(config, state));

    tokio::select! {
        err = site => {
            err.unwrap().unwrap();
        },
        err = socket => {
            err.unwrap().unwrap();
        },
    }

    Ok(())
}

async fn run_socket(config: Arc<Config>, state: Arc<AppState>) -> Result<()> {
    let socket_addr = format!("{}:{}", config.ip, config.port + 1).parse::<SocketAddr>()?;
    let socket = TcpListener::bind(socket_addr).await?;

    while let Ok((stream, _addr)) = socket.accept().await {
        let socket = tokio_tungstenite::accept_async(stream).await?;
        tokio::spawn(socket_moment(state.clone(), socket));
    }

    Ok(())
}

async fn serve_site(config: Arc<Config>) -> Result<()> {
    let router: Router = Router::new().nest_service("/", ServeDir::new("./assets/site/"));
    let site_addr = format!("{}:{}", config.ip, config.port).parse::<SocketAddr>()?;
    axum::Server::try_bind(&site_addr)?
        .serve(router.into_make_service())
        .await?;
    Ok(())
}
async fn socket_moment(app_state: Arc<AppState>, stream: WebSocketStream<TcpStream>) {
    let (mut sink, mut stream) = stream.split();

    let send_app_state = app_state.clone();
    let send = tokio::spawn(async move {
        loop {
            let session = get_session(send_app_state.clone()).await?;
            let music = if let Some(session) = session {
                get_session_details(send_app_state.clone(), &session).await?
            } else {
                MusicInfo::none()
            };

            sink.send(Message::Text(serde_json::to_string(&music)?))
                .await?;

            sleep(Duration::from_millis(200)).await;
        }

        #[allow(unreachable_code)] //Specifies eyre error so I can smash a ? on the end
        Err::<(), color_eyre::eyre::Error>(eyre!("Send loop ended, exiting"))
    });

    let recieve_app_state = app_state.clone();
    let recieve = tokio::spawn(async move {
        loop {
            let message = stream
                .next()
                .await
                .ok_or_else(|| eyre!("No next message"))??;

            match message {
                Message::Text(command) => {
                    let session = get_session(recieve_app_state.clone()).await?;

                    if let Some(session) = session {
                        if command == "toggle" {
                            session.TryTogglePlayPauseAsync()?;
                        } else if command == "skip" {
                            session.TrySkipNextAsync()?;
                        } else if command == "back" {
                            session.TrySkipPreviousAsync()?;
                        }
                    }
                }
                _ => {}
            }
        }

        #[allow(unreachable_code)] //Specifies eyre error so I can smash a ? on the end
        Err::<(), color_eyre::eyre::Error>(eyre!("Recieve loop ended, exiting"))
    });

    tokio::select! {
        send_result = send => {
            if let Err(e) = send_result {
                println!("{}", e);
            }
        },
        recieve_result = recieve => {
            if let Err(e) = recieve_result {
                println!("{}", e);
            }
        }
    }
    println!("Socket closed");
}

async fn get_session(
    app_state: Arc<AppState>,
) -> Result<Option<GlobalSystemMediaTransportControlsSession>> {
    let sessions: Vec<GlobalSystemMediaTransportControlsSession> =
        app_state.gsmt_manager.GetSessions()?.into_iter().collect();

    if sessions.is_empty() {
        return Ok(None);
    }

    for session in sessions.clone() {
        if session
            .SourceAppUserModelId()?
            .to_string()
            .to_lowercase()
            .contains("spotify.exe")
        //I like spotify prioritisation
        {
            return Ok(Some(session));
        }
    }

    Ok(Some(sessions[0].to_owned()))
}

async fn get_session_details(
    app_state: Arc<AppState>,
    session: &GlobalSystemMediaTransportControlsSession,
) -> Result<MusicInfo> {
    let session_info = session.TryGetMediaPropertiesAsync()?.await?;
    let session_timeline = session.GetTimelineProperties()?;

    let start_time = Duration::from(session_timeline.StartTime()?).as_millis();
    let end_time = Duration::from(session_timeline.EndTime()?).as_millis();
    let position = Duration::from(session_timeline.Position()?).as_millis();

    let music_info = MusicInfo {
        song_name: session_info.Title()?.to_string(),
        song_subtitle: session_info.Subtitle()?.to_string(),
        artist: session_info.Artist()?.to_string(),
        album: session_info.AlbumTitle()?.to_string(),
        start_time,
        end_time,
        position,
        playing: match session.GetPlaybackInfo()?.PlaybackStatus()? {
            windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => true,
            _ => false,
        },
        album_artwork: app_state.base64_engine.encode(get_thumbnail(&session_info)?),
    };

    Ok(music_info)
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

struct AppState {
    base64_engine: GeneralPurpose,
    gsmt_manager: GlobalSystemMediaTransportControlsSessionManager,
}

#[derive(Serialize, Debug)]
struct MusicInfo {
    song_name: String,
    song_subtitle: String,
    artist: String,
    album: String,
    start_time: u128,
    end_time: u128,
    position: u128,
    playing: bool,
    album_artwork: String,
}

impl MusicInfo {
    fn none() -> Self {
        Self {
            song_name: format!("No media currently!"),
            song_subtitle: String::new(),
            artist: String::new(),
            album: String::new(),
            start_time: 0,
            end_time: 0,
            position: 0,
            playing: false,
            album_artwork: String::new(),
        }
    }
}
