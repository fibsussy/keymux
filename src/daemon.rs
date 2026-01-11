/// Async Daemon - Main orchestrator with async management layer
///
/// Provides async event handling for hotplug, IPC, config changes, and session management
/// while maintaining synchronous event processors for zero-latency key processing.
use crate::config_manager::ConfigManager;
use crate::event_processor;
use crate::ipc::{get_root_socket_path, IpcRequest, IpcResponse};
use crate::keyboard_id::{find_all_keyboards, KeyboardId};
use crate::session_manager::SessionManager;
use anyhow::{Context, Result};

use evdev::Device;
use std::collections::{HashMap, HashSet};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Metadata about a keyboard
#[derive(Debug, Clone)]
struct KeyboardMeta {
    name: String,
    /// ALL event file paths for this logical keyboard
    paths: Vec<PathBuf>,
    connected: bool,
}

/// Active event processor thread handle
struct ProcessorHandle {
    shutdown_tx: crossbeam_channel::Sender<()>,
    game_mode_tx: mpsc::Sender<bool>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

/// Async daemon orchestrator
pub struct AsyncDaemon {
    /// Per-user configuration managers (uid -> ConfigManager)
    user_configs: HashMap<u32, ConfigManager>,
    /// Session manager for multi-user support
    session_manager: SessionManager,
    /// All detected keyboards
    all_keyboards: HashMap<KeyboardId, KeyboardMeta>,
    /// Active event processors - ONE THREAD PER EVENT FILE (event_path -> (keyboard_id, uid, handle))
    active_processors: HashMap<PathBuf, (KeyboardId, u32, ProcessorHandle)>,
    /// Keyboard ownership (keyboard_id -> uid)
    keyboard_owners: HashMap<KeyboardId, u32>,
    /// Current game mode state (preserved across thread restarts)
    game_mode_active: bool,
    /// Last config reload time for debouncing
    last_config_reload: Option<std::time::Instant>,
}

impl AsyncDaemon {
    /// Create a new async daemon
    pub fn new(_config_path: Option<PathBuf>, _user: Option<String>) -> Result<Self> {
        info!("Initializing async keyboard middleware daemon");

        // Check if running as root
        let is_root = unsafe { libc::getuid() } == 0;
        if !is_root {
            return Err(anyhow::anyhow!(
                "Daemon must run as root for device access. Use 'sudo systemctl start keyboard-middleware'"
            ));
        }

        let session_manager = SessionManager::new();

        Ok(Self {
            user_configs: HashMap::new(),
            session_manager,
            all_keyboards: HashMap::new(),
            active_processors: HashMap::new(),
            keyboard_owners: HashMap::new(),
            game_mode_active: false,
            last_config_reload: None,
        })
    }

    /// Run the async daemon event loop
    #[allow(clippy::future_not_send)]
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting async keyboard middleware daemon (multi-user mode)");

        // Start background services
        let hotplug_rx = self.start_hotplug_monitor();
        let ipc_rx = self.start_ipc_server()?;
        let niri_rx = self.start_niri_monitor();
        let config_watch_rx = self.start_config_watcher();

        // Initial session and keyboard discovery
        info!("Refreshing user sessions...");
        self.refresh_sessions().await;

        info!("Discovering keyboards...");
        self.discover_keyboards().await?;

        // Load user configs and sync keyboards
        info!("Loading user configs...");
        self.load_user_configs().await;

        info!("Syncing keyboards to users...");
        self.sync_keyboards_to_users().await;

        // Main event loop
        let mut hotplug_check = tokio::time::interval(Duration::from_millis(100));
        let mut ipc_check = tokio::time::interval(Duration::from_millis(10));
        let mut session_check = tokio::time::interval(Duration::from_secs(5));
        let mut niri_check = tokio::time::interval(Duration::from_millis(50));
        let mut config_check = tokio::time::interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                _ = hotplug_check.tick() => {
                    self.handle_hotplug_events(&hotplug_rx).await;
                }
                _ = ipc_check.tick() => {
                    self.handle_ipc_commands(&ipc_rx).await;
                }
                _ = session_check.tick() => {
                    self.refresh_sessions().await;
                    self.sync_keyboards_to_users().await;
                }
                _ = niri_check.tick() => {
                    self.handle_niri_events(&niri_rx).await;
                }
                _ = config_check.tick() => {
                    self.handle_config_changes(&config_watch_rx).await;
                }
            }
        }
    }

    /// Discover all keyboards (updates metadata only, doesn't start processors)
    async fn discover_keyboards(&mut self) -> Result<()> {
        info!("Discovering keyboards...");

        let keyboards = find_all_keyboards();
        info!("Found {} logical keyboard(s)", keyboards.len());

        // Mark all existing keyboards as disconnected first
        for meta in self.all_keyboards.values_mut() {
            meta.connected = false;
        }

        // Update keyboard metadata for connected keyboards
        for (kbd_id, logical_kbd) in keyboards {
            let kbd_name = logical_kbd.name.clone();
            let paths: Vec<PathBuf> = logical_kbd
                .devices
                .iter()
                .map(|(path, _)| path.clone())
                .collect();

            let was_known = self.all_keyboards.contains_key(&kbd_id);
            info!(
                "{} keyboard: {} ({}) with {} event file(s)",
                if was_known {
                    "Found existing"
                } else {
                    "Detected new"
                },
                kbd_name,
                kbd_id,
                paths.len()
            );

            self.all_keyboards.insert(
                kbd_id.clone(),
                KeyboardMeta {
                    name: kbd_name,
                    paths,
                    connected: true,
                },
            );
        }

        // Log disconnected keyboards
        for (kbd_id, meta) in &self.all_keyboards {
            if !meta.connected {
                info!("Keyboard disconnected: {} ({})", meta.name, kbd_id);
            }
        }

        Ok(())
    }

    /// Synchronize keyboards to active users based on their configs
    async fn sync_keyboards_to_users(&mut self) {
        // Get all active user sessions
        if let Err(e) = self.session_manager.refresh_sessions().await {
            error!("Failed to refresh sessions: {}", e);
            return;
        }

        // Load configs for active users (if not already loaded)
        self.load_user_configs().await;

        // First, stop processors for disconnected keyboards
        let disconnected_keyboards: Vec<_> = self
            .all_keyboards
            .iter()
            .filter(|(_, meta)| !meta.connected)
            .map(|(id, _)| id.clone())
            .collect();

        for kbd_id in disconnected_keyboards {
            info!("Stopping processors for disconnected keyboard: {}", kbd_id);
            let _ = self.stop_processors_for_keyboard(&kbd_id).await;
            self.keyboard_owners.remove(&kbd_id);
        }

        // Collect keyboard data first to avoid borrow checker issues
        let keyboards: Vec<_> = self
            .all_keyboards
            .iter()
            .filter(|(_, meta)| meta.connected) // Only process connected keyboards
            .map(|(id, meta)| (id.clone(), meta.clone()))
            .collect();

        // For each keyboard, check if any active user wants it
        for (kbd_id, meta) in keyboards {
            let mut assigned_uid = None;

            // Check existing ownership first
            if let Some(&owner_uid) = self.keyboard_owners.get(&kbd_id) {
                // Verify owner session is still active
                if self.session_manager.is_user_active(owner_uid).await {
                    // Check if keyboard is still enabled in their config
                    if let Some(config_mgr) = self.user_configs.get(&owner_uid) {
                        let config = config_mgr.get_config().await;
                        let kbd_id_str = kbd_id.to_string();
                        let enabled = config
                            .enabled_keyboards
                            .as_ref()
                            .map(|list| list.contains(&kbd_id_str))
                            .unwrap_or(false);

                        if enabled {
                            assigned_uid = Some(owner_uid);
                        } else {
                            info!("User {} disabled keyboard {}", owner_uid, meta.name);
                        }
                    }
                } else {
                    info!(
                        "User {} session no longer active, releasing keyboard {}",
                        owner_uid, kbd_id
                    );
                }
            }

            // If not assigned, check other active users (first-come-first-serve)
            if assigned_uid.is_none() {
                let user_configs: Vec<_> = self
                    .user_configs
                    .iter()
                    .map(|(uid, cfg)| (*uid, cfg.clone()))
                    .collect();
                for (uid, config_mgr) in user_configs {
                    if !self.session_manager.is_user_active(uid).await {
                        continue;
                    }

                    let config = config_mgr.get_config().await;
                    let kbd_id_str = kbd_id.to_string();
                    let wants_keyboard = config
                        .enabled_keyboards
                        .as_ref()
                        .map(|list| list.contains(&kbd_id_str))
                        .unwrap_or(false);

                    if wants_keyboard {
                        info!("Assigning keyboard {} to user {}", meta.name, uid);
                        assigned_uid = Some(uid);
                        break;
                    }
                }
            }

            // Start or stop processor based on assignment
            match assigned_uid {
                Some(uid) => {
                    // Check if already running for this user by checking if ANY event path for this keyboard is active
                    let has_active_processors = meta.paths.iter().any(|path| {
                        self.active_processors
                            .get(path)
                            .map(|(_, owner_uid, _)| *owner_uid == uid)
                            .unwrap_or(false)
                    });

                    if !has_active_processors {
                        // Stop any existing processors for this keyboard (might be owned by different user)
                        let _ = self.stop_processors_for_keyboard(&kbd_id).await;

                        // Start ONE THREAD PER EVENT FILE
                        if let Err(e) = self
                            .start_processors_for_keyboard(&kbd_id, &meta.name, &meta.paths, uid)
                            .await
                        {
                            error!("Failed to start processors for user {}: {}", uid, e);
                        } else {
                            self.keyboard_owners.insert(kbd_id.clone(), uid);
                        }
                    }
                }
                None => {
                    // No user wants this keyboard, stop if running
                    let has_processors = meta
                        .paths
                        .iter()
                        .any(|path| self.active_processors.contains_key(path));

                    if has_processors {
                        info!(
                            "No active user wants keyboard {}, stopping processors",
                            meta.name
                        );
                        let _ = self.stop_processors_for_keyboard(&kbd_id).await;
                        self.keyboard_owners.remove(&kbd_id);
                    }
                }
            }
        }
    }

    /// Load configs for all active users
    async fn load_user_configs(&mut self) {
        if let Err(e) = self.session_manager.refresh_sessions().await {
            error!("Failed to refresh sessions: {}", e);
            return;
        }

        // Get active user UIDs
        let active_uids = self.get_active_user_uids().await;
        info!("Active user UIDs: {:?}", active_uids);

        for &uid in &active_uids {
            // Skip if already loaded
            if self.user_configs.contains_key(&uid) {
                continue;
            }

            // Get user's home directory
            let home_dir = match self.get_user_home_dir(uid) {
                Ok(dir) => dir,
                Err(e) => {
                    warn!("Failed to get home directory for user {}: {}", uid, e);
                    continue;
                }
            };

            let config_path = home_dir.join(".config/keyboard-middleware/config.ron");

            // Load user's config
            match ConfigManager::new(config_path.clone()) {
                Ok(config_mgr) => {
                    info!("Loaded config for user {} from {:?}", uid, config_path);
                    self.user_configs.insert(uid, config_mgr);
                }
                Err(e) => {
                    debug!("No config for user {} at {:?}: {}", uid, config_path, e);
                }
            }
        }

        // Remove configs for inactive users
        self.user_configs.retain(|uid, _| active_uids.contains(uid));
    }

    /// Get list of active user UIDs
    async fn get_active_user_uids(&self) -> Vec<u32> {
        self.session_manager.get_active_uids().await
    }

    /// Get user's home directory
    fn get_user_home_dir(&self, uid: u32) -> Result<PathBuf> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("getent passwd {} | cut -d: -f6", uid))
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to get home directory for UID {}",
                uid
            ));
        }

        let home = String::from_utf8(output.stdout)?.trim().to_string();

        if home.is_empty() {
            return Err(anyhow::anyhow!("Empty home directory for UID {}", uid));
        }

        Ok(PathBuf::from(home))
    }

    /// Start event processors for ALL event files of a keyboard - ONE THREAD PER EVENT FILE!
    async fn start_processors_for_keyboard(
        &mut self,
        kbd_id: &KeyboardId,
        kbd_name: &str,
        event_paths: &[PathBuf],
        uid: u32,
    ) -> Result<()> {
        // Get user's config
        let config = self
            .user_configs
            .get(&uid)
            .context("User config not loaded")?
            .get_config()
            .await;

        info!(
            "Starting {} event processor thread(s) for: {} (user: {})",
            event_paths.len(),
            kbd_name,
            uid
        );

        // Spawn ONE THREAD PER EVENT FILE
        for (idx, event_path) in event_paths.iter().enumerate() {
            // Check if already running
            if self.active_processors.contains_key(event_path) {
                warn!("Processor already running for: {}", event_path.display());
                continue;
            }

            // Open device
            let device = Device::open(event_path)
                .with_context(|| format!("Failed to open device: {}", event_path.display()))?;

            // Create channels
            let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded(1);
            let (game_mode_tx, game_mode_rx) = mpsc::channel();

            // Start event processor thread
            let kbd_id_clone = kbd_id.clone();
            let kbd_name_clone = kbd_name.to_string();
            let event_path_display = event_path.display().to_string();
            let config_clone = config.clone();

            let handle = thread::spawn(move || {
                info!(
                    "Event processor thread started for {} (event file: {})",
                    kbd_name_clone, event_path_display
                );
                if let Err(e) = event_processor::start_event_processor(
                    kbd_id_clone,
                    device,
                    kbd_name_clone.clone(),
                    config_clone,
                    shutdown_rx,
                    game_mode_rx,
                ) {
                    error!("Event processor failed for {}: {}", kbd_name_clone, e);
                }
            });

            // Store processor handle indexed by EVENT PATH
            self.active_processors.insert(
                event_path.clone(),
                (
                    kbd_id.clone(),
                    uid,
                    ProcessorHandle {
                        shutdown_tx,
                        game_mode_tx: game_mode_tx.clone(),
                        thread_handle: Some(handle),
                    },
                ),
            );

            // Send current game mode state to the new thread to preserve state across restarts
            let _ = game_mode_tx.send(self.game_mode_active);

            info!(
                "Started thread {}/{} for {} at {} (game_mode: {})",
                idx + 1,
                event_paths.len(),
                kbd_name,
                event_path.display(),
                self.game_mode_active
            );
        }

        Ok(())
    }

    /// Stop ALL event processors for a keyboard
    async fn stop_processors_for_keyboard(&mut self, kbd_id: &KeyboardId) -> Result<()> {
        // Find all event paths for this keyboard
        let paths_to_stop: Vec<PathBuf> = self
            .active_processors
            .iter()
            .filter(|(_, (k_id, _, _))| k_id == kbd_id)
            .map(|(path, _)| path.clone())
            .collect();

        if paths_to_stop.is_empty() {
            return Ok(());
        }

        info!(
            "Stopping {} processor thread(s) for: {}",
            paths_to_stop.len(),
            kbd_id
        );

        for path in paths_to_stop {
            if let Some((_, _, mut handle)) = self.active_processors.remove(&path) {
                // Send shutdown signal
                let _ = handle.shutdown_tx.send(());

                // Wait for thread to finish (with timeout)
                if let Some(thread_handle) = handle.thread_handle.take() {
                    // Wait in background to avoid blocking
                    tokio::task::spawn_blocking(move || {
                        let _ = thread_handle.join();
                    });
                }

                info!("Stopped processor for: {}", path.display());
            }
        }

        Ok(())
    }

    /// Start hotplug monitor (udev)
    fn start_hotplug_monitor(&self) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            loop {
                // Use udevadm to monitor for input device changes
                let mut child = Command::new("udevadm")
                    .arg("monitor")
                    .arg("--subsystem-match=input")
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("Failed to start udevadm monitor");

                if let Some(stdout) = child.stdout.take() {
                    use std::io::BufRead;
                    let reader = std::io::BufReader::new(stdout);

                    for line in reader.lines().map_while(Result::ok) {
                        if line.contains("event") {
                            let _ = tx.send(line);
                        }
                    }
                }

                // Wait for child process to exit to avoid zombies
                let _ = child.wait();

                // If udevadm exits, restart it
                warn!("udevadm monitor died, restarting...");
                thread::sleep(Duration::from_secs(1));
            }
        });

        rx
    }

    /// Start IPC server
    fn start_ipc_server(&self) -> Result<mpsc::Receiver<(IpcRequest, mpsc::Sender<IpcResponse>)>> {
        let (tx, rx) = mpsc::channel();
        let socket_path = get_root_socket_path();

        // Remove old socket if exists
        let _ = std::fs::remove_file(&socket_path);

        // Create socket directory
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path).context("Failed to bind IPC socket")?;

        // Set socket permissions to allow user access (mode 0666)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o666);
            if let Err(e) = std::fs::set_permissions(&socket_path, permissions) {
                warn!("Failed to set socket permissions: {}", e);
            } else {
                info!("Socket permissions set to 0666 (world-readable/writable)");
            }
        }

        info!("IPC server listening on: {:?}", socket_path);

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        use std::io::{Read, Write};

                        // Read length prefix (4 bytes)
                        let mut len_buf = [0u8; 4];
                        if let Err(e) = stream.read_exact(&mut len_buf) {
                            error!("Failed to read IPC length: {}", e);
                            continue;
                        }
                        let len = u32::from_le_bytes(len_buf) as usize;

                        // Read request data
                        let mut buffer = vec![0u8; len];
                        match stream.read_exact(&mut buffer) {
                            Ok(()) => {
                                if let Ok(request) = bincode::deserialize::<IpcRequest>(&buffer) {
                                    // Create response channel
                                    let (resp_tx, resp_rx) = mpsc::channel();

                                    // Send to main loop
                                    if tx.send((request, resp_tx)).is_ok() {
                                        // Wait for response
                                        if let Ok(response) =
                                            resp_rx.recv_timeout(Duration::from_secs(5))
                                        {
                                            if let Ok(resp_bytes) = bincode::serialize(&response) {
                                                // Send length prefix
                                                let resp_len =
                                                    (resp_bytes.len() as u32).to_le_bytes();
                                                let _ = stream.write_all(&resp_len);
                                                // Send response data
                                                let _ = stream.write_all(&resp_bytes);
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => error!("Failed to read IPC request: {}", e),
                        }
                    }
                    Err(e) => error!("Failed to accept IPC connection: {}", e),
                }
            }
        });

        Ok(rx)
    }

    /// Start niri window monitor
    fn start_niri_monitor(&self) -> mpsc::Receiver<crate::niri::NiriEvent> {
        let (tx, rx) = mpsc::channel();

        if crate::niri::is_niri_available() {
            crate::niri::start_niri_monitor(tx);
            info!("Started niri window monitor");
        } else {
            debug!("Niri not available, skipping window monitor");
        }

        rx
    }

    /// Start config file watcher for automatic reload
    /// Returns: Receiver<()> that signals when any config changed
    fn start_config_watcher(&self) -> mpsc::Receiver<()> {
        use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};

        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let (watch_tx, watch_rx) = std::sync::mpsc::channel();

            let mut watcher = match recommended_watcher(watch_tx) {
                Ok(w) => w,
                Err(e) => {
                    error!("Failed to create config file watcher: {}", e);
                    return;
                }
            };

            // Build set of config paths by scanning /home
            let mut watched_paths: HashSet<PathBuf> = HashSet::new();

            // Scan for users with keyboard-middleware configs
            if let Ok(entries) = std::fs::read_dir("/home") {
                for entry in entries.flatten() {
                    let home_dir = entry.path();
                    let config_dir = home_dir.join(".config/keyboard-middleware");
                    let config_path = config_dir.join("config.ron");

                    if config_path.exists() {
                        // Watch the config directory (not recursively)
                        if let Err(e) = watcher.watch(&config_dir, RecursiveMode::NonRecursive) {
                            warn!("Failed to watch {:?}: {}", config_dir, e);
                        } else {
                            info!("Watching config at {:?}", config_path);
                            watched_paths.insert(config_path);
                        }
                    }
                }
            }

            info!(
                "Config file watcher started for {} config(s)",
                watched_paths.len()
            );

            loop {
                match watch_rx.recv() {
                    Ok(Ok(Event {
                        kind: EventKind::Modify(_) | EventKind::Create(_),
                        paths,
                        ..
                    })) => {
                        // Check if any modified file is a config.ron we're watching
                        let mut detected_change = false;
                        for path in paths {
                            if watched_paths.contains(&path) {
                                info!("Config file changed: {:?}", path);
                                detected_change = true;
                                break;
                            }
                        }

                        if detected_change {
                            // Debounce: drain all events for the next 300ms
                            let debounce_start = std::time::Instant::now();
                            while debounce_start.elapsed() < Duration::from_millis(300) {
                                if watch_rx.recv_timeout(Duration::from_millis(50)).is_err() {
                                    break;
                                }
                                // Drain event, continue debouncing
                            }

                            // Send single reload signal after debounce
                            info!("Config changes settled, triggering reload");
                            let _ = tx.send(());
                        }
                    }
                    Ok(Ok(_)) => {} // Ignore other event types
                    Ok(Err(e)) => error!("Config watch error: {}", e),
                    Err(e) => {
                        error!("Config watch channel error: {}", e);
                        break;
                    }
                }
            }
        });

        rx
    }

    /// Handle hotplug events
    #[allow(clippy::future_not_send)]
    async fn handle_hotplug_events(&mut self, rx: &mpsc::Receiver<String>) {
        while let Ok(event) = rx.try_recv() {
            debug!("Hotplug event: {}", event);

            // Reload config before handling the hotplug event
            info!("Reloading configs before processing hotplug event...");
            self.load_user_configs().await;

            // Rediscover keyboards (updates all_keyboards metadata)
            if let Err(e) = self.discover_keyboards().await {
                error!("Failed to rediscover keyboards: {}", e);
                continue;
            }

            // Sync keyboards to users (start/stop threads based on what changed)
            self.sync_keyboards_to_users().await;
        }
    }

    /// Handle config file changes - triggers full reload (same as IPC)
    #[allow(clippy::future_not_send)]
    async fn handle_config_changes(&mut self, rx: &mpsc::Receiver<()>) {
        while rx.try_recv().is_ok() {
            // Additional debouncing: ignore if we reloaded very recently
            if let Some(last_reload) = self.last_config_reload {
                if last_reload.elapsed() < Duration::from_millis(500) {
                    debug!("Ignoring config change (too soon after last reload)");
                    continue;
                }
            }

            info!("Config file changed, triggering full reload...");
            self.last_config_reload = Some(std::time::Instant::now());
            if let Err(e) = self.reload_all_configs().await {
                error!("Auto-reload failed: {}", e);
            }
        }
    }

    /// Reload all user configs and restart processors
    async fn reload_all_configs(&mut self) -> Result<()> {
        info!("Reloading all user configs...");

        // Send notification to user
        let _ = std::process::Command::new("runuser")
            .args([
                "-u",
                "fib",
                "--",
                "/usr/bin/notify-send",
                "reloading middleware",
            ])
            .spawn();

        // Step 1: Validate all configs before stopping anything
        info!("Validating configs...");
        let active_uids = self.get_active_user_uids().await;
        for &uid in &active_uids {
            let home_dir = match self.get_user_home_dir(uid) {
                Ok(dir) => dir,
                Err(_) => continue,
            };

            let config_path = home_dir.join(".config/keyboard-middleware/config.ron");
            if config_path.exists() {
                // Try to load and validate
                let new_config = match crate::config::Config::load(&config_path) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        error!("Config load failed for user {}: {}", uid, e);
                        return Err(anyhow::anyhow!("Invalid config for user {}: {}", uid, e));
                    }
                };
                if let Err(e) = new_config.validate_silent() {
                    error!("Config validation failed for user {}: {}", uid, e);
                    return Err(anyhow::anyhow!("Invalid config for user {}: {}", uid, e));
                }
            }
        }

        // Step 2: Stop all processors
        info!("Stopping all processors...");
        let all_kbd_ids: Vec<_> = self.keyboard_owners.keys().cloned().collect();
        for kbd_id in all_kbd_ids {
            let _ = self.stop_processors_for_keyboard(&kbd_id).await;
        }

        // Step 3: Clear and reload configs
        info!("Reloading configs from disk...");
        self.user_configs.clear();
        self.load_user_configs().await;

        // Step 4: Restart all processors with new configs
        info!("Restarting processors with new configs...");
        self.sync_keyboards_to_users().await;

        info!("Config reload complete!");
        Ok(())
    }

    /// Handle IPC commands
    #[allow(clippy::future_not_send)]
    async fn handle_ipc_commands(
        &mut self,
        rx: &mpsc::Receiver<(IpcRequest, mpsc::Sender<IpcResponse>)>,
    ) {
        while let Ok((request, resp_tx)) = rx.try_recv() {
            debug!("IPC request: {:?}", request);

            let response = match request {
                IpcRequest::Ping => IpcResponse::Pong,
                IpcRequest::SetGameMode(enabled) => {
                    self.set_game_mode_all(enabled).await;
                    IpcResponse::Ok
                }
                IpcRequest::ListKeyboards => {
                    let keyboards = self
                        .all_keyboards
                        .iter()
                        .map(|(id, meta)| {
                            // Use first path as representative (for display)
                            let device_path = meta
                                .paths
                                .first()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();

                            // Keyboard is enabled if ANY of its event paths have active processors
                            let enabled = meta
                                .paths
                                .iter()
                                .any(|path| self.active_processors.contains_key(path));

                            crate::ipc::KeyboardInfo {
                                hardware_id: id.to_string(),
                                name: meta.name.clone(),
                                device_path,
                                enabled,
                                connected: meta.connected,
                            }
                        })
                        .collect();
                    IpcResponse::KeyboardList(keyboards)
                }
                IpcRequest::ToggleKeyboards => {
                    info!("Toggle keyboards requested via IPC");
                    match self.reload_all_configs().await {
                        Ok(()) => IpcResponse::Ok,
                        Err(e) => {
                            error!("Toggle reload failed: {}", e);
                            IpcResponse::Error(format!("Toggle failed: {}", e))
                        }
                    }
                }
                IpcRequest::EnableKeyboard(hardware_id) => {
                    info!("Enable keyboard requested via IPC: {}", hardware_id);
                    // Create KeyboardId from string
                    let kbd_id = crate::keyboard_id::KeyboardId::new(hardware_id.clone());
                    // Check if keyboard exists
                    if self.all_keyboards.contains_key(&kbd_id) {
                        // Find the first active user to assign to
                        // In multi-user mode, we need to know which user's config to update
                        // For now, we'll trigger a resync which will check all user configs
                        info!("Keyboard {} found, triggering resync", hardware_id);
                        self.sync_keyboards_to_users().await;
                        IpcResponse::Ok
                    } else {
                        IpcResponse::Error(format!("Keyboard not found: {}", hardware_id))
                    }
                }
                IpcRequest::DisableKeyboard(hardware_id) => {
                    info!("Disable keyboard requested via IPC: {}", hardware_id);
                    // Create KeyboardId from string
                    let kbd_id = crate::keyboard_id::KeyboardId::new(hardware_id.clone());
                    // Stop all processors for this keyboard
                    if let Err(e) = self.stop_processors_for_keyboard(&kbd_id).await {
                        error!("Failed to stop processors: {}", e);
                        IpcResponse::Error(format!("Failed to stop processors: {}", e))
                    } else {
                        self.keyboard_owners.remove(&kbd_id);
                        IpcResponse::Ok
                    }
                }
                IpcRequest::Reload => {
                    info!("Config reload requested via IPC");
                    match self.reload_all_configs().await {
                        Ok(()) => IpcResponse::Ok,
                        Err(e) => {
                            error!("Config reload failed: {}", e);
                            IpcResponse::Error(format!("Reload failed: {}", e))
                        }
                    }
                }
                IpcRequest::Shutdown => {
                    info!("Shutdown requested via IPC");
                    // TODO: Implement graceful shutdown
                    IpcResponse::Ok
                }
            };

            let _ = resp_tx.send(response);
        }
    }

    /// Handle niri window focus events
    #[allow(clippy::future_not_send)]
    async fn handle_niri_events(&mut self, rx: &mpsc::Receiver<crate::niri::NiriEvent>) {
        while let Ok(event) = rx.try_recv() {
            match event {
                crate::niri::NiriEvent::WindowFocusChanged(window_info) => {
                    let should_enable = crate::niri::should_enable_gamemode(&window_info);

                    debug!("Niri window focus changed, game mode: {}", should_enable);
                    self.set_game_mode_all(should_enable).await;
                }
            }
        }
    }

    /// Set game mode for all active processors
    async fn set_game_mode_all(&mut self, enabled: bool) {
        // Only update if the state actually changed
        if self.game_mode_active == enabled {
            return;
        }

        info!(
            "Setting game mode to: {} ({} active threads)",
            enabled,
            self.active_processors.len()
        );

        // Store the new state so new threads will get it
        self.game_mode_active = enabled;

        // Send to all active threads
        for (_, _, handle) in self.active_processors.values() {
            let _ = handle.game_mode_tx.send(enabled);
        }
    }

    /// Refresh user sessions
    async fn refresh_sessions(&self) {
        if let Err(e) = self.session_manager.refresh_sessions().await {
            error!("Failed to refresh sessions: {}", e);
        }
    }
}
