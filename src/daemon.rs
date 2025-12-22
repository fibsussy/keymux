use anyhow::Result;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, channel, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::event_processor;
use crate::keyboard_id::{find_all_keyboards, KeyboardId};
use crate::ipc::{get_socket_path, IpcRequest, IpcResponse};
use crate::niri::{self, NiriEvent};

/// Hotplug event from udev monitor
#[derive(Debug)]
enum HotplugEvent {
    Added,
    Removed,
}

/// IPC command from client
#[derive(Debug)]
enum IpcCommand {
    Reload,
}

/// Main daemon orchestrator
pub struct Daemon {
    /// All detected keyboards (connected + disconnected)
    all_keyboards: HashMap<KeyboardId, KeyboardMeta>,
    /// Hotplug event receiver
    hotplug_rx: Receiver<HotplugEvent>,
    /// IPC command receiver
    ipc_rx: Receiver<IpcCommand>,
    /// Niri event receiver
    niri_rx: Receiver<NiriEvent>,
    /// File watcher event receiver
    file_watcher_rx: Receiver<Event>,
    /// Configuration
    config: Config,
    /// Active event processor threads - maps KeyboardId to shutdown channel Sender
    active_processors: HashMap<KeyboardId, Sender<()>>,
    /// Game mode senders - maps KeyboardId to game mode toggle channel Sender
    game_mode_senders: HashMap<KeyboardId, Sender<bool>>,
    /// Current game mode state
    game_mode_active: bool,
}

#[derive(Debug, Clone)]
struct KeyboardMeta {
    name: String,
    device_path: Option<String>,
    connected: bool,
}

/// Start file watcher for config file
fn start_config_watcher(config_path: &Path, tx: Sender<Event>) -> notify::Result<()> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            // Only care about Modify events
            if matches!(event.kind, EventKind::Modify(_)) {
                let _ = tx.send(event);
            }
        }
    })?;

    // Watch config directory (some editors use atomic rename)
    let config_dir = config_path
        .parent()
        .ok_or_else(|| notify::Error::generic("Invalid config path"))?;

    watcher.watch(config_dir, RecursiveMode::NonRecursive)?;

    info!("Watching config file for changes: {:?}", config_path);

    // Keep watcher alive in a thread
    thread::spawn(move || {
        let _watcher = watcher; // Move watcher into thread to keep it alive
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    });

    Ok(())
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

/// Start IPC listener for client commands
fn start_ipc_listener(tx: Sender<IpcCommand>) {
    thread::spawn(move || {
        let socket_path = get_socket_path();

        // Remove old socket if it exists
        let _ = std::fs::remove_file(&socket_path);

        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => {
                info!("IPC socket listening at {:?}", socket_path);
                l
            }
            Err(e) => {
                error!("Failed to bind IPC socket: {}", e);
                return;
            }
        };

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    // Read request
                    let mut len_buf = [0u8; 4];
                    if stream.read_exact(&mut len_buf).is_err() {
                        continue;
                    }
                    let len = u32::from_le_bytes(len_buf) as usize;

                    let mut buf = vec![0u8; len];
                    if stream.read_exact(&mut buf).is_err() {
                        continue;
                    }

                    let request: IpcRequest = match bincode::deserialize(&buf) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    debug!("IPC request: {:?}", request);

                    // Handle request
                    let response = match request {
                        IpcRequest::ToggleKeyboards => {
                            // Send reload command to daemon
                            let _ = tx.send(IpcCommand::Reload);
                            IpcResponse::Ok
                        }
                        _ => IpcResponse::Error("Not implemented".to_string()),
                    };

                    // Send response
                    if let Ok(encoded) = bincode::serialize(&response) {
                        let len = (encoded.len() as u32).to_le_bytes();
                        use std::io::Write;
                        let _ = stream.write_all(&len);
                        let _ = stream.write_all(&encoded);
                    }
                }
                Err(e) => {
                    error!("IPC connection error: {}", e);
                }
            }
        }
    });
}

impl Daemon {
    pub fn new() -> Result<Self> {
        let (hotplug_tx, hotplug_rx) = mpsc::channel();
        let (ipc_tx, ipc_rx) = mpsc::channel();
        let (niri_tx, niri_rx) = mpsc::channel();
        let (file_watcher_tx, file_watcher_rx) = mpsc::channel();

        // Start udev monitor in background
        start_udev_monitor(hotplug_tx);

        // Start IPC listener
        start_ipc_listener(ipc_tx);

        // Load config
        let config_path = Config::default_path()?;
        let config = Config::load(&config_path)?;

        // Start file watcher
        if let Err(e) = start_config_watcher(&config_path, file_watcher_tx) {
            error!("Failed to start config file watcher: {}", e);
            warn!("Hot-reload will not work automatically");
        }

        // Start niri monitor if auto_detect is enabled
        if crate::config::GameMode::auto_detect_enabled() {
            if niri::is_niri_available() {
                info!("Starting niri window monitor for automatic game mode detection");
                niri::start_niri_monitor(niri_tx);
            } else {
                warn!("Automatic game mode detection enabled but Niri socket not found");
                warn!("Game mode detection will be unavailable");
            }
        } else {
            info!("Automatic game mode detection is disabled");
        }

        info!("Loaded config with {} enabled keyboard(s)",
              config.enabled_keyboards.as_ref().map(|k| k.len()).unwrap_or(0));

        Ok(Self {
            all_keyboards: HashMap::new(),
            hotplug_rx,
            ipc_rx,
            niri_rx,
            file_watcher_rx,
            config,
            active_processors: HashMap::new(),
            game_mode_senders: HashMap::new(),
            game_mode_active: false,
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

            // Check for IPC commands (non-blocking)
            match self.ipc_rx.try_recv() {
                Ok(IpcCommand::Reload) => {
                    info!("IPC: Reload requested");
                    self.perform_hot_reload();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No IPC command
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    warn!("IPC listener died");
                }
            }

            // Check for niri window events (non-blocking)
            // Check for config file changes (non-blocking)
            match self.file_watcher_rx.try_recv() {
                Ok(_event) => {
                    info!("Config file changed, triggering hot-reload...");
                    // Add debouncing to avoid multiple rapid reloads
                    thread::sleep(Duration::from_millis(100));

                    // Drain any additional events
                    while self.file_watcher_rx.try_recv().is_ok() {}

                    // Perform reload
                    self.perform_hot_reload();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No file changes
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    warn!("File watcher died");
                }
            }

            match self.niri_rx.try_recv() {
                Ok(NiriEvent::WindowFocusChanged(window_info)) => {
                    // Determine if game mode should be active
                    let should_enable = niri::should_enable_gamemode(&window_info);

                    // Only update and broadcast if state changed
                    if should_enable != self.game_mode_active {
                        self.game_mode_active = should_enable;
                        info!("Game mode {}", if should_enable { "ENABLED" } else { "DISABLED" });
                        self.broadcast_game_mode(should_enable);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No niri event
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    warn!("Niri monitor died");
                }
            }

            // Sleep briefly to avoid busy-waiting
            thread::sleep(Duration::from_millis(100));
        }

        info!("Daemon stopped");
        Ok(())
    }

    /// Broadcast game mode state to all active processors
    fn broadcast_game_mode(&self, active: bool) {
        for (id, tx) in &self.game_mode_senders {
            if let Err(e) = tx.send(active) {
                debug!("Failed to send game mode state to {}: {}", id, e);
            }
        }
    }

    /// Perform hot-reload: kill threads, reload config, respawn threads
    fn perform_hot_reload(&mut self) {
        info!("Starting hot-reload...");

        // 1. Stop all active processors to release devices
        info!("Stopping {} active processors for hot-reload...", self.active_processors.len());

        // Send shutdown signal to all processors and wait for them to exit
        for (id, shutdown_tx) in self.active_processors.drain() {
            let _ = shutdown_tx.send(());
            debug!("Sent shutdown signal to {}", id);
        }
        self.game_mode_senders.clear();

        // Give threads time to actually exit and release device file descriptors
        // This is critical - the old thread must close the device before we can reopen it
        thread::sleep(Duration::from_millis(500));

        // 2. Reload config from disk
        let config_path = match Config::default_path() {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to get config path: {}", e);
                return;
            }
        };

        match Config::load(&config_path) {
            Ok(new_config) => {
                self.config = new_config;
                info!("Config reloaded successfully");

                // Notify user of successful reload
                let _ = std::process::Command::new("notify-send")
                    .arg("-a")
                    .arg("Keyboard Middleware")
                    .arg("Config Reloaded")
                    .arg("Configuration reloaded successfully")
                    .spawn();
            }
            Err(e) => {
                error!("Failed to reload config: {}", e);
                error!("Keeping previous config");

                // Notify user of config error
                let error_msg = format!("Invalid config: {}", e);
                let _ = std::process::Command::new("notify-send")
                    .arg("-u")
                    .arg("critical")
                    .arg("-a")
                    .arg("Keyboard Middleware")
                    .arg("Config Error")
                    .arg(&error_msg)
                    .spawn();

                return;
            }
        }

        // 3. Run discover_keyboards which will restart enabled keyboards
        // Since we removed all processors above, discover_keyboards will see
        // all keyboards as needing to be started
        self.discover_keyboards();

        info!("Hot-reload complete! {} processors active", self.active_processors.len());
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
                // Create game mode channel
                let (game_mode_tx, game_mode_rx) = channel();

                // Get keyboard-specific config
                let keyboard_config = self.config.for_keyboard(&id_str);

                if let Err(e) = event_processor::start_event_processor(
                    id.clone(),
                    device,
                    name,
                    keyboard_config,
                    shutdown_rx,
                    game_mode_rx,
                ) {
                    error!("Failed to start event processor for {}: {}", id, e);
                } else {
                    self.active_processors.insert(id.clone(), shutdown_tx);
                    self.game_mode_senders.insert(id.clone(), game_mode_tx);

                    // Send current game mode state to newly started processor
                    if self.game_mode_active {
                        if let Some(tx) = self.game_mode_senders.get(&id) {
                            let _ = tx.send(true);
                        }
                    }
                }
            }
        }

        // Kill threads for keyboards that are either:
        // 1. No longer connected (unplugged)
        // 2. Still connected but disabled in config
        let should_stop: Vec<KeyboardId> = self.active_processors
            .keys()
            .filter(|id| {
                let id_str = id.to_string();
                // Stop if disconnected OR not in enabled list
                !connected_ids.contains(id) || !enabled_ids.contains(&id_str)
            })
            .cloned()
            .collect();

        for id in should_stop {
            if !connected_ids.contains(&id) {
                info!("Keyboard {} disconnected, stopping event processor", id);
            } else {
                info!("Keyboard {} disabled, stopping event processor", id);
            }

            if let Some(shutdown_tx) = self.active_processors.remove(&id) {
                // Send shutdown signal (ignore if thread already died)
                let _ = shutdown_tx.send(());
            }
            // Remove game mode sender as well
            self.game_mode_senders.remove(&id);
        }
    }
}
