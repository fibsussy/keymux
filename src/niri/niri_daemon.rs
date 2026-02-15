use crate::config::GameMode;
use crate::ipc::{send_request, IpcRequest, IpcResponse};
use crate::niri;
use crate::ui::window::{get_all_windows, GameModeState};
use anyhow::Result;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};

pub fn run_niri_daemon() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("Starting keymux-niri watcher");

    if !GameMode::auto_detect_enabled() {
        error!("Automatic game mode detection is disabled in config");
        error!("Set game_mode.detection_method = \"Auto\" to enable");
        return Ok(());
    }

    if !niri::is_niri_available() {
        error!("Niri socket not found - is Niri running?");
        error!("This daemon requires Niri window manager");
        return Ok(());
    }

    info!("Niri detected, starting window focus monitor");

    let (niri_tx, niri_rx) = mpsc::channel();
    niri::start_niri_monitor_sync(niri_tx);

    let mut current_game_mode = false;

    loop {
        match niri_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(niri::NiriEvent::WindowFocusChanged(_window_info)) => {
                let should_enable = get_all_windows().is_ok_and(|windows| {
                    windows
                        .iter()
                        .find(|w| w.is_focused)
                        .is_some_and(|w| matches!(w.game_mode_state(), GameModeState::GameMode(_)))
                });

                if should_enable != current_game_mode {
                    current_game_mode = should_enable;
                    info!(
                        "Game mode state changed: {}",
                        if should_enable { "ENABLED" } else { "DISABLED" }
                    );

                    match send_request(&IpcRequest::SetGameMode(should_enable)) {
                        Ok(IpcResponse::Ok) => {
                            info!("Successfully sent game mode update to daemon");
                        }
                        Ok(other) => {
                            warn!("Unexpected response from daemon: {:?}", other);
                        }
                        Err(e) => {
                            error!("Failed to send game mode update to daemon: {}", e);
                            error!("Is keymux daemon running?");
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                error!("Niri monitor died, exiting");
                break;
            }
        }

        thread::sleep(Duration::from_millis(50));
    }

    info!("Niri watcher stopped");
    Ok(())
}
