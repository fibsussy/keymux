use crate::config::GameMode;
use crate::hyprland;
use crate::ipc::{send_request, IpcRequest, IpcResponse};
use crate::window_manager::WindowManagerEvent::WindowFocusChanged;
use anyhow::Result;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};

pub fn run_sway_daemon() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("Starting keymux-sway watcher");

    if !GameMode::auto_detect_enabled() {
        error!("Automatic game mode detection is disabled in config");
        error!("Set game_mode.detection_method = \"Auto\" to enable");
        return Ok(());
    }

    if !hyprland::is_sway_available() {
        error!("Sway socket not found - is Sway running?");
        error!("This daemon requires Sway window manager");
        return Ok(());
    }

    info!("Sway detected, starting window focus monitor");

    let (sway_tx, sway_rx) = mpsc::channel();
    hyprland::start_sway_monitor_sync(sway_tx);

    let mut current_game_mode = false;

    loop {
        match sway_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(WindowFocusChanged(_window_info)) => {
                let should_enable = hyprland::should_enable_gamemode(&_window_info);

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
                error!("Sway monitor died, exiting");
                break;
            }
        }

        thread::sleep(Duration::from_millis(50));
    }

    info!("Sway watcher stopped");
    Ok(())
}
