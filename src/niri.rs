use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use tracing::{error, info};

#[derive(Debug)]
pub enum NiriEvent {
    WindowFocusChanged(Option<String>),
}

/// Get the currently focused window's app ID
fn get_focused_window_app_id() -> Option<String> {
    let output = Command::new("niri")
        .args(["msg", "focused-window"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;

    // Parse the output looking for: App ID: "something"
    for line in text.lines() {
        if let Some(app_id_part) = line.trim().strip_prefix("App ID:") {
            // Extract string between quotes
            let app_id = app_id_part.trim().trim_matches('"');
            return Some(app_id.to_string());
        }
    }

    None
}

/// Start monitoring niri window focus events
/// Returns immediately after spawning the monitor thread
pub fn start_niri_monitor(tx: Sender<NiriEvent>) {
    thread::spawn(move || {
        loop {
            info!("Starting niri event stream monitor...");
            info!("Watching for gamescope windows...");

            let mut child = match Command::new("niri")
                .args(["msg", "event-stream"])
                .stdout(Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    error!("Failed to spawn niri: {}", e);
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };

            let Some(stdout) = child.stdout.take() else {
                error!("Failed to capture niri stdout");
                thread::sleep(Duration::from_secs(5));
                continue;
            };

            let reader = BufReader::new(stdout);

            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if line.starts_with("Window focus changed:") {
                            let app_id = get_focused_window_app_id();
                            if let Some(ref app) = app_id {
                                info!("Focus changed → app_id: {}", app);
                            }
                            if tx.send(NiriEvent::WindowFocusChanged(app_id)).is_err() {
                                error!("Niri monitor: channel closed, exiting");
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error reading niri event: {}", e);
                        break;
                    }
                }
            }

            error!("Niri event stream ended, restarting in 5 seconds...");
            thread::sleep(Duration::from_secs(5));
        }
    });
}

/// Handle niri window change and return whether game mode should be active
pub fn should_enable_gamemode(app_id: Option<&str>) -> bool {
    app_id == Some("gamescope")
}
