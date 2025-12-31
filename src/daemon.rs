use anyhow::Result;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, channel, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::event_processor;
use crate::keyboard_id::{find_all_keyboards, KeyboardId};
use crate::ipc::{get_root_socket_path, IpcRequest, IpcResponse};

/// Hotplug event from udev monitor
#[derive(Debug)]
enum HotplugEvent {
    Added,
    Removed,
}

/// IPC command from client
#[derive(Debug)]
enum IpcCommand {
    /// Reload configuration for a specific user
    Reload { username: String },
    /// Set game mode for a specific user
    SetGameMode { username: String, enabled: bool },
}

/// Get username from Unix socket peer credentials using SO_PEERCRED
fn get_peer_username(stream: &UnixStream) -> Result<String> {
    use std::os::unix::io::AsRawFd;
    use std::mem;

    #[repr(C)]
    struct UCred {
        pid: libc::pid_t,
        uid: libc::uid_t,
        gid: libc::gid_t,
    }

    let fd = stream.as_raw_fd();
    let mut cred: UCred = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<UCred>() as libc::socklen_t;

    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut UCred as *mut libc::c_void,
            &mut len,
        )
    };

    if ret != 0 {
        return Err(anyhow::anyhow!("Failed to get peer credentials"));
    }

    let uid = cred.uid;

    // Convert uid to username
    let output = Command::new("id")
        .arg("-nu")
        .arg(uid.to_string())
        .output()?;

    if output.status.success() {
        let username = String::from_utf8(output.stdout)?
            .trim()
            .to_string();
        Ok(username)
    } else {
        Err(anyhow::anyhow!("Failed to get username for uid {}", uid))
    }
}

/// Main daemon orchestrator
pub struct Daemon {
    /// All detected keyboards (connected + disconnected)
    all_keyboards: HashMap<KeyboardId, KeyboardMeta>,
    /// Hotplug event receiver
    hotplug_rx: Receiver<HotplugEvent>,
    /// IPC command receiver
    ipc_rx: Receiver<IpcCommand>,
    /// File watcher event receiver
    file_watcher_rx: Receiver<Event>,
    /// Per-user configurations (username -> config)
    user_configs: HashMap<String, Config>,
    /// Keyboard ownership (keyboard_id -> username)
    keyboard_owners: HashMap<KeyboardId, String>,
    /// Active event processor threads - maps `KeyboardId` to shutdown channel Sender
    active_processors: HashMap<KeyboardId, Sender<()>>,
    /// Game mode senders - maps `KeyboardId` to game mode toggle channel Sender
    game_mode_senders: HashMap<KeyboardId, Sender<bool>>,
    /// Per-user game mode state (username -> game_mode_active)
    user_game_modes: HashMap<String, bool>,
}

#[derive(Debug, Clone)]
struct KeyboardMeta {
    #[allow(dead_code)]
    name: String,
    /// All event device paths for this logical keyboard
    device_paths: Vec<String>,
    connected: bool,
    /// Number of active event processors for this keyboard
    active_device_count: usize,
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
        // Root daemon always uses /run/keyboard-middleware.sock
        let socket_path = get_root_socket_path();

        // Remove old socket if it exists
        let _ = std::fs::remove_file(&socket_path);

        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => {
                info!("IPC socket listening at {:?}", socket_path);

                // Set socket permissions so all users can connect
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666)) {
                    error!("Failed to set socket permissions: {}", e);
                }

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
                    // Extract username from socket credentials
                    let username = match get_peer_username(&stream) {
                        Ok(u) => u,
                        Err(e) => {
                            error!("Failed to get peer username: {}", e);
                            continue;
                        }
                    };

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

                    debug!("IPC request from user '{}': {:?}", username, request);

                    // Handle request
                    let response = match request {
                        IpcRequest::ToggleKeyboards => {
                            // Send reload command to daemon for this user
                            let _ = tx.send(IpcCommand::Reload { username: username.clone() });
                            IpcResponse::Ok
                        }
                        IpcRequest::SetGameMode(enabled) => {
                            // Send game mode command for this user
                            let _ = tx.send(IpcCommand::SetGameMode { username: username.clone(), enabled });
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
    pub fn new(_config_path_opt: Option<std::path::PathBuf>, _user_opt: Option<String>) -> Result<Self> {
        let (hotplug_tx, hotplug_rx) = mpsc::channel();
        let (ipc_tx, ipc_rx) = mpsc::channel();
        let (file_watcher_tx, file_watcher_rx) = mpsc::channel();

        // Start udev monitor in background
        start_udev_monitor(hotplug_tx);

        // Start IPC listener
        start_ipc_listener(ipc_tx);

        // Note: Configs are now loaded on-demand per user when they make IPC requests
        info!("Daemon starting in multi-user mode - configs loaded per user on demand");

        // File watcher is disabled in multi-user mode for now
        // TODO: Watch all user config directories
        let _ = file_watcher_tx; // Suppress unused warning

        // Niri monitoring has been moved to keyboard-middleware-niri user service
        // That service will send SetGameMode IPC commands to this daemon
        info!("Niri window monitoring should be handled by keyboard-middleware-niri user service");

        Ok(Self {
            all_keyboards: HashMap::new(),
            hotplug_rx,
            ipc_rx,
            file_watcher_rx,
            user_configs: HashMap::new(),
            keyboard_owners: HashMap::new(),
            active_processors: HashMap::new(),
            game_mode_senders: HashMap::new(),
            user_game_modes: HashMap::new(),
        })
    }

    /// Load or reload a user's config
    fn load_user_config(&mut self, username: &str) -> Result<()> {
        let user_home = std::path::PathBuf::from(format!("/home/{}", username));
        let user_config_path = user_home.join(".config/keyboard-middleware/config.ron");

        info!("Loading config for user '{}' from: {}", username, user_config_path.display());

        let config = Config::load(&user_config_path)
            .map_err(|e| anyhow::anyhow!("Failed to load config for user '{}' from '{}': {}", username, user_config_path.display(), e))?;

        info!("Loaded config for user '{}' with {} enabled keyboard(s)",
              username,
              config.enabled_keyboards.as_ref().map_or(0, std::vec::Vec::len));

        self.user_configs.insert(username.to_string(), config);
        Ok(())
    }

    /// Get a user's config, loading it if not already loaded
    fn get_user_config(&mut self, username: &str) -> Result<&Config> {
        if !self.user_configs.contains_key(username) {
            self.load_user_config(username)?;
        }
        Ok(self.user_configs.get(username).unwrap())
    }

    /// Auto-load configs for users with existing config files
    fn auto_load_user_configs(&mut self) {
        // Scan /home for user directories
        let home_entries = match std::fs::read_dir("/home") {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to scan /home for user configs: {}", e);
                return;
            }
        };

        for entry in home_entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(username) = entry.file_name().to_str() {
                        // Check if this user has a config file
                        let config_path = entry.path().join(".config/keyboard-middleware/config.ron");
                        if config_path.exists() {
                            info!("Found config for user '{}', auto-loading keyboards...", username);
                            if let Err(e) = self.discover_keyboards_for_user(username) {
                                warn!("Failed to auto-load keyboards for user '{}': {}", username, e);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Start the daemon
    pub fn run(&mut self) -> Result<()> {
        info!("Starting keyboard middleware daemon");

        // Discover keyboards
        self.discover_keyboards();

        info!("Daemon ready, {} keyboard(s) discovered", self.all_keyboards.len());

        // Auto-load configs for users with existing config files
        self.auto_load_user_configs();

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
                Ok(IpcCommand::Reload { username }) => {
                    info!("IPC: Reload requested for user '{}'", username);
                    if let Err(e) = self.perform_hot_reload_for_user(&username) {
                        error!("Failed to reload config for user '{}': {}", username, e);
                    }
                }
                Ok(IpcCommand::SetGameMode { username, enabled }) => {
                    info!("IPC: Set game mode {} for user '{}'", if enabled { "ON" } else { "OFF" }, username);
                    self.set_game_mode_for_user(&username, enabled);
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
                    // File watcher is disabled in multi-user mode
                    // Users trigger reloads explicitly via IPC
                    debug!("Config file change detected but file watcher is disabled in multi-user mode");
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No file changes
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // File watcher not active in multi-user mode
                }
            }


            // Sleep briefly to avoid busy-waiting
            thread::sleep(Duration::from_millis(100));
        }

        info!("Daemon stopped");
        Ok(())
    }

    /// Perform hot-reload for a specific user
    fn perform_hot_reload_for_user(&mut self, username: &str) -> Result<()> {
        info!("Starting hot-reload for user '{}'...", username);

        // 1. Reload config for this user
        self.load_user_config(username)?;

        // 2. Stop processors owned by this user
        let keyboards_to_stop: Vec<KeyboardId> = self.keyboard_owners
            .iter()
            .filter(|(_, owner)| *owner == username)
            .map(|(id, _)| id.clone())
            .collect();

        info!("Stopping {} processors owned by '{}'", keyboards_to_stop.len(), username);
        for id in &keyboards_to_stop {
            if let Some(shutdown_tx) = self.active_processors.remove(id) {
                let _ = shutdown_tx.send(());
                debug!("Sent shutdown signal to {} (owner: {})", id, username);
            }
            self.game_mode_senders.remove(id);
            self.keyboard_owners.remove(id);
        }

        // Give threads time to exit
        thread::sleep(Duration::from_millis(500));

        // 3. Rediscover keyboards and restart this user's enabled ones
        self.discover_keyboards_for_user(username)?;

        info!("Hot-reload complete for user '{}'! {} total processors active", username, self.active_processors.len());

        // Notify user of successful reload
        let _ = std::process::Command::new("sudo")
            .arg("-u")
            .arg(username)
            .arg("notify-send")
            .arg("-a")
            .arg("Keyboard Middleware")
            .arg("Config Reloaded")
            .arg("Configuration reloaded successfully")
            .spawn();

        Ok(())
    }

    /// Set game mode state for a specific user
    fn set_game_mode_for_user(&mut self, username: &str, enabled: bool) {
        // Update per-user game mode state
        self.user_game_modes.insert(username.to_string(), enabled);

        // Broadcast to all keyboards owned by this user
        let keyboards_to_update: Vec<KeyboardId> = self.keyboard_owners
            .iter()
            .filter(|(_, owner)| *owner == username)
            .map(|(id, _)| id.clone())
            .collect();

        info!("Broadcasting game mode {} to {} keyboards owned by '{}'",
              if enabled { "ON" } else { "OFF" },
              keyboards_to_update.len(),
              username);

        for id in keyboards_to_update {
            if let Some(sender) = self.game_mode_senders.get(&id) {
                let _ = sender.send(enabled);
            }
        }
    }

    /// Discover keyboards and start processors for a specific user's enabled keyboards
    fn discover_keyboards_for_user(&mut self, username: &str) -> Result<()> {
        let keyboards = find_all_keyboards();

        // Get this user's enabled keyboard IDs and keyboard configs
        // We need to collect these first to avoid borrow conflicts
        let (enabled_ids, keyboard_configs): (HashSet<String>, HashMap<String, Config>) = {
            let user_config = self.get_user_config(username)?;
            let ids: HashSet<String> = user_config
                .enabled_keyboards
                .as_ref()
                .map(|ids| ids.iter().cloned().collect())
                .unwrap_or_default();

            // Pre-fetch all keyboard configs for enabled keyboards
            let configs: HashMap<String, Config> = ids
                .iter()
                .map(|id| (id.clone(), user_config.for_keyboard(id)))
                .collect();

            (ids, configs)
        };

        let user_game_mode = self.user_game_modes.get(username).copied().unwrap_or(false);

        for (id, logical_kb) in keyboards {
            let id_str = id.to_string();

            // Only process keyboards enabled by this user
            if !enabled_ids.contains(&id_str) {
                continue;
            }

            // Check if this keyboard is already owned by another user
            if let Some(existing_owner) = self.keyboard_owners.get(&id) {
                if existing_owner != username {
                    warn!("Keyboard {} is already owned by user '{}', skipping for '{}'",
                          id, existing_owner, username);
                    continue;
                }
            }

            // Check if processor is already running
            if self.active_processors.contains_key(&id) {
                debug!("Processor already running for {}, skipping", id);
                continue;
            }

            // Start event processor for this keyboard
            if let Some((_, first_device)) = logical_kb.devices.into_iter().next() {
                info!("Starting event processor for user '{}': {} ({})", username, logical_kb.name, id);

                // Create channels
                let (shutdown_tx, shutdown_rx) = channel();
                let (game_mode_tx, game_mode_rx) = channel();

                // Get keyboard-specific config for this user (should always exist since we pre-fetched)
                let keyboard_config = keyboard_configs.get(&id_str).cloned()
                    .expect("Keyboard config should have been pre-fetched");

                if let Err(e) = event_processor::start_event_processor(
                    id.clone(),
                    first_device,
                    logical_kb.name.clone(),
                    keyboard_config,
                    shutdown_rx,
                    game_mode_rx,
                ) {
                    error!("Failed to start event processor for {}: {}", id, e);
                } else {
                    // Track ownership and processors
                    self.active_processors.insert(id.clone(), shutdown_tx);
                    self.game_mode_senders.insert(id.clone(), game_mode_tx.clone());
                    self.keyboard_owners.insert(id.clone(), username.to_string());

                    // Send initial game mode state for this user
                    let _ = game_mode_tx.send(user_game_mode);
                }
            }
        }

        Ok(())
    }

    /// Discover all connected keyboards (metadata only, no processor starting)
    fn discover_keyboards(&mut self) {
        // First, mark all existing keyboards as disconnected
        for meta in self.all_keyboards.values_mut() {
            meta.connected = false;
        }

        // Then discover currently connected keyboards (now returns LogicalKeyboard)
        let keyboards = find_all_keyboards();

        // Track which keyboards are currently connected
        let mut connected_ids = HashSet::new();

        for (id, logical_kb) in keyboards {
            connected_ids.insert(id.clone());

            // Collect all device paths
            let device_paths: Vec<String> = logical_kb.devices.iter()
                .filter_map(|(_path, dev)| dev.physical_path().map(|p| p.to_string()))
                .collect();

            // Update or insert keyboard
            if let Some(meta) = self.all_keyboards.get_mut(&id) {
                // Keyboard was known before, update it
                meta.connected = true;
                meta.device_paths = device_paths;
                debug!("Re-discovered keyboard: {} ({}) with {} device(s)",
                      logical_kb.name, id, logical_kb.devices.len());
            } else {
                // New keyboard
                self.all_keyboards.insert(
                    id.clone(),
                    KeyboardMeta {
                        name: logical_kb.name.clone(),
                        device_paths,
                        connected: true,
                        active_device_count: 0,
                    },
                );
                info!("Discovered new keyboard: {} ({}) with {} device(s)",
                      logical_kb.name, id, logical_kb.devices.len());
            }
        }

        // Stop processors for disconnected keyboards
        let should_stop: Vec<KeyboardId> = self.active_processors
            .keys()
            .filter(|id| !connected_ids.contains(id))
            .cloned()
            .collect();

        for id in should_stop {
            let owner = self.keyboard_owners.get(&id).map(|s| s.as_str()).unwrap_or("unknown");
            info!("Keyboard {} (owned by '{}') disconnected, stopping event processor", id, owner);

            if let Some(shutdown_tx) = self.active_processors.remove(&id) {
                // Send shutdown signal (ignore if thread already died)
                let _ = shutdown_tx.send(());
            }
            // Clean up state
            self.game_mode_senders.remove(&id);
            self.keyboard_owners.remove(&id);
        }
    }
}
