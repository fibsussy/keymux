use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, channel, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::event_processor;
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
    /// Configuration
    config: Config,
    /// Active event processor threads - maps KeyboardId to shutdown channel Sender
    active_processors: HashMap<KeyboardId, Sender<()>>,
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

        // Load config
        let config_path = Config::default_path()?;
        let config = Config::load(&config_path)?;

        info!("Loaded config with {} enabled keyboard(s)",
              config.enabled_keyboards.as_ref().map(|k| k.len()).unwrap_or(0));

        Ok(Self {
            all_keyboards: HashMap::new(),
            hotplug_rx,
            config,
            active_processors: HashMap::new(),
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

        // Get enabled keyboard IDs from config
        let enabled_ids: HashSet<String> = self.config
            .enabled_keyboards
            .as_ref()
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default();

        // Track which keyboards are currently connected
        let mut connected_ids = HashSet::new();

        for (id, (device, name)) in keyboards {
            let device_path = device.physical_path().map(std::string::ToString::to_string);
            let id_str = id.to_string();
            connected_ids.insert(id.clone());

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

            // Start event processor if enabled and not already processing
            if enabled_ids.contains(&id_str) && !self.active_processors.contains_key(&id) {
                info!("Starting event processor for enabled keyboard: {} ({})", name, id);

                // Create shutdown channel
                let (shutdown_tx, shutdown_rx) = channel();

                if let Err(e) = event_processor::start_event_processor(
                    id.clone(),
                    device,
                    name,
                    shutdown_rx,
                ) {
                    error!("Failed to start event processor for {}: {}", id, e);
                } else {
                    self.active_processors.insert(id.clone(), shutdown_tx);
                }
            }
        }

        // Kill threads for keyboards that are no longer connected
        let disconnected_ids: Vec<KeyboardId> = self.active_processors
            .keys()
            .filter(|id| !connected_ids.contains(id))
            .cloned()
            .collect();

        for id in disconnected_ids {
            info!("Keyboard {} disconnected, stopping event processor", id);
            if let Some(shutdown_tx) = self.active_processors.remove(&id) {
                // Send shutdown signal (ignore if thread already died)
                let _ = shutdown_tx.send(());
            }
        }
    }
}
