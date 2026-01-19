use anyhow::Result;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber;
use window::get_all_windows;

mod config;
mod ipc;
mod niri;
mod window;

/// Niri window watcher daemon that monitors window focus changes
/// and sends game mode updates to the root keyboard-middleware daemon via IPC
fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("Starting keyboard-middleware-niri watcher");

    // Check if automatic game mode detection is enabled
    if !config::GameMode::auto_detect_enabled() {
        error!("Automatic game mode detection is disabled in config");
        error!("Set game_mode.detection_method = \"Auto\" to enable");
        return Ok(());
    }

    // Check if niri is available
    if !niri::is_niri_available() {
        error!("Niri socket not found - is Niri running?");
        error!("This daemon requires Niri window manager");
        return Ok(());
    }

    info!("Niri detected, starting window focus monitor");

    // Create channel for niri events
    let (niri_tx, niri_rx) = mpsc::channel();

    // Start niri monitor
    niri::start_niri_monitor_sync(niri_tx);

    // Track current game mode state to avoid sending redundant IPC requests
    let mut current_game_mode = false;

    loop {
        match niri_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(niri::NiriEvent::WindowFocusChanged(_window_info)) => {
                // Get all windows and check if any focused window should enable game mode
                match get_all_windows() {
                    Ok(windows) => {
                        if let Some(focused_window) = windows.iter().find(|w| w.is_focused) {
                            let game_state = focused_window.game_mode_state();
                            let should_enable =
                                matches!(game_state, window::GameModeState::GameMode(_));
                            info!(
                                "Focused window: {} ({}), game state: {:?}, should_enable: {}",
                                focused_window.title,
                                focused_window.app_id,
                                game_state,
                                should_enable
                            );

                            // Only send IPC if state changed
                            if should_enable != current_game_mode {
                                current_game_mode = should_enable;
                                info!(
                                    "Game mode state changed: {}",
                                    if should_enable { "ENABLED" } else { "DISABLED" }
                                );

                                // Send IPC request to root daemon
                                match ipc::send_request(&ipc::IpcRequest::SetGameMode(
                                    should_enable,
                                )) {
                                    Ok(ipc::IpcResponse::Ok) => {
                                        info!("Successfully sent game mode update to daemon");
                                    }
                                    Ok(other) => {
                                        warn!("Unexpected response from daemon: {:?}", other);
                                    }
                                    Err(e) => {
                                        error!("Failed to send game mode update to daemon: {}", e);
                                        error!("Is keyboard-middleware daemon running?");
                                    }
                                }
                            }
                        } else {
                            warn!("No focused window found");
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get windows: {}", e);
                    }
                }
            }
                }

                // Only send IPC if state changed
                if should_enable != current_game_mode {
                    current_game_mode = should_enable;
                    info!(
                        "Game mode state changed: {}",
                        if should_enable { "ENABLED" } else { "DISABLED" }
                    );

                    // Send IPC request to root daemon
                    match ipc::send_request(&ipc::IpcRequest::SetGameMode(should_enable)) {
                        Ok(ipc::IpcResponse::Ok) => {
                            info!("Successfully sent game mode update to daemon");
                        }
                        Ok(other) => {
                            warn!("Unexpected response from daemon: {:?}", other);
                        }
                        Err(e) => {
                            error!("Failed to send game mode update to daemon: {}", e);
                            error!("Is keyboard-middleware daemon running?");
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No event, continue
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                error!("Niri monitor died, exiting");
                break;
            }
        }

        // Brief sleep to avoid busy-waiting
        thread::sleep(Duration::from_millis(50));
    }

    info!("Niri watcher stopped");
    Ok(())
}
