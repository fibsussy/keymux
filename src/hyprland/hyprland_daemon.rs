use crate::config::GameMode;
use crate::hyprland;
use crate::ipc::{send_request, IpcRequest, IpcResponse};
use crate::window_manager::WindowManagerEvent::WindowFocusChanged;
use anyhow::Result;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};

pub fn run_hyprland_daemon() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("Starting keymux-hyprland watcher");

    if !GameMode::auto_detect_enabled() {
        error!("Automatic game mode detection is disabled in config");
        error!("Set game_mode.detection_method = \"Auto\" to enable");
        return Ok(());
    }

    if !hyprland::is_hyprland_available() {
        error!("Hyprland socket not found - is Hyprland running?");
        error!("This daemon requires Hyprland window manager");
        return Ok(());
    }

    info!("Hyprland detected, starting window focus monitor");

    let (hyprland_tx, hyprland_rx) = mpsc::channel();
    hyprland::start_hyprland_monitor_sync(hyprland_tx);

    let mut current_game_mode = false;

    loop {
        match hyprland_rx.recv_timeout(Duration::from_millis(100)) {
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
                error!("Hyprland monitor died, exiting");
                break;
            }
        }

        thread::sleep(Duration::from_millis(50));
    }

    info!("Hyprland watcher stopped");
    Ok(())
}
