use anyhow::Result;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::keyboard_id::{find_all_keyboards, KeyboardId};

/// Hotplug event from udev monitor
#[derive(Debug)]
enum HotplugEvent {
    Added,
    Removed,
}

/// Main daemon orchestrator
pub struct Daemon {
    /// All detected keyboards (connected + disconnected)
    all_keyboards: HashMap<KeyboardId, KeyboardMeta>,
    /// Hotplug event receiver
    hotplug_rx: Receiver<HotplugEvent>,
}

#[derive(Debug, Clone)]
struct KeyboardMeta {
    name: String,
    device_path: Option<String>,
    connected: bool,
}

/// Start udev monitor for keyboard hotplug events
fn start_udev_monitor(tx: Sender<HotplugEvent>) {
    thread::spawn(move || {
        loop {
            info!("Starting udevadm monitor for input device hotplug");

            let mut child = match Command::new("udevadm")
                .args(["monitor", "--subsystem-match=input"])
                .stdout(Stdio::piped())
                .spawn() {
                Ok(child) => child,
                Err(e) => {
                    error!("Failed to spawn udevadm: {}", e);
                    error!("Retrying in 5 seconds...");
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };

            let Some(stdout) = child.stdout.take() else {
                error!("Failed to capture udevadm stdout");
                thread::sleep(Duration::from_secs(5));
                continue;
            };

            let reader = BufReader::new(stdout);

            for line in reader.lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(e) => {
                        error!("Error reading udevadm output: {}", e);
                        break;
                    }
                };

                // Parse lines like: "KERNEL[12345.678] add      /devices/.../input/input123/event4 (input)"
                // or: "KERNEL[12345.678] remove   /devices/.../input/input123 (input)"

                if !line.starts_with("KERNEL[") {
                    continue;
                }

                let is_add = line.contains("] add ");
                let is_remove = line.contains("] remove ");

                if !is_add && !is_remove {
                    continue;
                }

                // Only care about events with "event" in the path (actual input devices)
                if !line.contains("/event") {
                    continue;
                }

                if is_add {
                    debug!("[udev] Input device added");
                    let _ = tx.send(HotplugEvent::Added);
                } else if is_remove {
                    debug!("[udev] Input device removed");
                    let _ = tx.send(HotplugEvent::Removed);
                }
            }

            warn!("udevadm monitor process ended, restarting in 5 seconds...");
            thread::sleep(Duration::from_secs(5));
        }
    });
}

impl Daemon {
    pub fn new() -> Result<Self> {
        let (hotplug_tx, hotplug_rx) = mpsc::channel();

        // Start udev monitor in background
        start_udev_monitor(hotplug_tx);

        Ok(Self {
            all_keyboards: HashMap::new(),
            hotplug_rx,
        })
    }

    /// Start the daemon
    pub fn run(&mut self) -> Result<()> {
        info!("Starting keyboard middleware daemon");

        // Discover keyboards
        self.discover_keyboards();

        info!("Daemon ready, {} keyboard(s) discovered", self.all_keyboards.len());

        // Main daemon loop - event-driven, no polling!
        loop {
            // Check for hotplug events from udev (non-blocking)
            match self.hotplug_rx.try_recv() {
                Ok(HotplugEvent::Added | HotplugEvent::Removed) => {
                    // Keyboard added or removed - rediscover
                    debug!("Hotplug event detected, rediscovering keyboards");
                    self.discover_keyboards();
                    info!("Hotplug: Updated keyboard list, {} total", self.all_keyboards.len());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No hotplug event, continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    warn!("Hotplug monitor died, exiting");
                    break;
                }
            }

            // Sleep briefly to avoid busy-waiting
            thread::sleep(Duration::from_millis(100));
        }

        info!("Daemon stopped");
        Ok(())
    }

    /// Discover all connected keyboards
    fn discover_keyboards(&mut self) {
        // First, mark all existing keyboards as disconnected
        for meta in self.all_keyboards.values_mut() {
            meta.connected = false;
        }

        // Then discover currently connected keyboards
        let keyboards = find_all_keyboards();

        for (id, (device, name)) in keyboards {
            let device_path = device.physical_path().map(std::string::ToString::to_string);

            // Update or insert keyboard
            if let Some(meta) = self.all_keyboards.get_mut(&id) {
                // Keyboard was known before, update it
                meta.connected = true;
                meta.device_path = device_path;
                info!("Re-discovered keyboard: {} ({})", name, id);
            } else {
                // New keyboard
                self.all_keyboards.insert(
                    id.clone(),
                    KeyboardMeta {
                        name: name.clone(),
                        device_path,
                        connected: true,
                    },
                );
                info!("Discovered keyboard: {} ({})", name, id);
            }
        }
    }
}
