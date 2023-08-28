#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(target_os = "windows")]
use base64::{
    alphabet::STANDARD,
    engine::{GeneralPurpose, GeneralPurposeConfig},
    Engine,
};
use color_eyre::{eyre::eyre, Result};
use futures::{SinkExt, StreamExt, stream::FusedStream};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    time::sleep,
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
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

    let state = Arc::new(AppState {
        base64_engine: GeneralPurpose::new(&STANDARD, GeneralPurposeConfig::default()),
    });

    let socket_addr = format!("{}:{}", config.ip, config.port).parse::<SocketAddr>()?;
    let socket = TcpListener::bind(socket_addr).await?;

    while let Ok((stream, _addr)) = socket.accept().await {
        let socket = tokio_tungstenite::accept_async(stream).await?;
        tokio::spawn(socket_moment(state.clone(), socket));
    }

    Ok(())
}

async fn socket_moment(app_state: Arc<AppState>, stream: WebSocketStream<TcpStream>) {
    
    let (mut sink, mut stream) = stream.split();

    let send = tokio::spawn(async move {
        loop {
            println!("send");
            let session = get_session().await?;
            let music = get_session_details(app_state.clone(), &session).await?;

            sink.send(Message::Text(serde_json::to_string(&music)?))
                .await?;

            sleep(Duration::from_millis(200)).await;
        }

        #[allow(unreachable_code)] //Specifies eyre error so I can smash a ? on the end
        Err::<(), color_eyre::eyre::Error>(eyre!("Send loop ended, exiting"))
    });

    let recieve = tokio::spawn(async move {
        loop {
            println!("recieve");
            let message = stream.next().await.ok_or_else(|| eyre!("No next message"))??;
            
            match message {
                Message::Text(command) => {
                    let session = get_session().await?;

                    if command == "toggle" {
                        session.TryTogglePlayPauseAsync()?;
                    } else if command == "skip" {
                        session.TrySkipNextAsync()?;
                    } else if command == "back" {
                        session.TrySkipPreviousAsync()?;
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
                eprintln!("{}", e);
            }
        },
        recieve_result = recieve => {
            if let Err(e) = recieve_result {
                eprintln!("{}", e);
            }
        }
    }
    println!("exited")
}

async fn get_session() -> Result<GlobalSystemMediaTransportControlsSession> {
    match GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.await {
        Ok(gsmt_session_manager) => {
            let sessions: Vec<GlobalSystemMediaTransportControlsSession> =
                gsmt_session_manager.GetSessions()?.into_iter().collect();

            if sessions.is_empty() {
                return Err(eyre!("No sessions found"));
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
        Err(_) => Err(eyre!(
            "Could not get session manager (Caused by all sessions closing? not sure tbh)"
        )),
    }
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
        // album_artwork: app_state.base64_engine.encode(get_thumbnail(&session_info)?),
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
    // album_artwork: String,
}
