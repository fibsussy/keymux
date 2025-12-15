use anyhow::Result;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::ipc::{IpcRequest, IpcResponse, IpcServer, KeyboardInfo};
use crate::keyboard_id::{find_all_keyboards, KeyboardId};
use crate::keyboard_thread::KeyboardThread;
use crate::niri::{self, NiriEvent};

/// Main daemon orchestrator
pub struct Daemon {
    config: Config,
    config_path: std::path::PathBuf,
    /// Currently running keyboard threads
    threads: HashMap<KeyboardId, KeyboardThread>,
    /// All detected keyboards (connected + disconnected)
    all_keyboards: HashMap<KeyboardId, KeyboardMeta>,
    /// IPC server
    ipc_server: IpcServer,
    /// Niri event broadcaster
    niri_tx: Sender<NiriEvent>,
    niri_rx: Receiver<NiriEvent>,
}

#[derive(Debug, Clone)]
struct KeyboardMeta {
    name: String,
    device_path: Option<String>,
    connected: bool,
}

impl Daemon {
    pub fn new(config: Config, config_path: std::path::PathBuf) -> Result<Self> {
        let ipc_server = IpcServer::new()?;
        let (niri_tx, niri_rx) = mpsc::channel();

        Ok(Self {
            config,
            config_path,
            threads: HashMap::new(),
            all_keyboards: HashMap::new(),
            ipc_server,
            niri_tx,
            niri_rx,
        })
    }

    /// Start the daemon
    pub fn run(&mut self) -> Result<()> {
        info!("Starting keyboard middleware daemon");

        // Start niri monitor
        let niri_tx = self.niri_tx.clone();
        niri::start_niri_monitor(niri_tx);
        info!("Niri monitor started");

        // Discover and start keyboard threads
        self.discover_keyboards()?;

        // Initialize config if needed and save it
        self.init_enabled_set();
        if let Err(e) = self.config.save(&self.config_path) {
            warn!("Failed to save initial config: {}", e);
        } else {
            info!("Config initialized and saved to {:?}", self.config_path);
        }

        self.start_enabled_keyboards()?;

        info!("Daemon ready, {} keyboard(s) active out of {} discovered",
              self.threads.len(),
              self.all_keyboards.len());

        let mut last_hotplug_check = std::time::Instant::now();
        let hotplug_interval = Duration::from_secs(2);

        // Main daemon loop
        loop {
            // Handle IPC requests
            if let Some((request, stream)) = self.ipc_server.try_accept()? {
                let is_shutdown = matches!(request, IpcRequest::Shutdown);
                let response = self.handle_ipc_request(request);
                if let Err(e) = IpcServer::send_response(stream, &response) {
                    error!("Failed to send IPC response: {}", e);
                }

                // Check if we got shutdown request
                if is_shutdown && matches!(response, IpcResponse::Ok) {
                    info!("Received shutdown request, stopping daemon");
                    break;
                }
            }

            // Periodic hotplug detection (check every 2 seconds)
            if last_hotplug_check.elapsed() >= hotplug_interval {
                if let Err(e) = self.handle_hotplug() {
                    error!("Hotplug check failed: {}", e);
                }
                last_hotplug_check = std::time::Instant::now();
            }

            // Clean up dead threads
            self.cleanup_dead_threads();

            // Sleep briefly to avoid busy-waiting
            thread::sleep(Duration::from_millis(10));
        }

        // Shutdown all threads
        self.shutdown_all_threads();

        info!("Daemon stopped");
        Ok(())
    }

    /// Discover all connected keyboards
    fn discover_keyboards(&mut self) -> Result<()> {
        // First, mark all existing keyboards as disconnected
        for meta in self.all_keyboards.values_mut() {
            meta.connected = false;
        }

        // Then discover currently connected keyboards
        let keyboards = find_all_keyboards()?;

        for (id, (device, name)) in keyboards {
            let device_path = device.physical_path().map(|s| s.to_string());

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

        Ok(())
    }

    /// Start threads for all enabled keyboards
    fn start_enabled_keyboards(&mut self) -> Result<()> {
        let keyboards = find_all_keyboards()?;

        info!("Starting enabled keyboards (found {} total)", keyboards.len());

        for (id, (device, name)) in keyboards {
            info!("Checking keyboard: {} ({}), enabled: {}", name, id, self.is_keyboard_enabled(&id));
            if self.is_keyboard_enabled(&id) {
                self.start_keyboard_thread(id, device, name)?;
            } else {
                info!("Skipping disabled keyboard: {} ({})", name, id);
            }
        }

        Ok(())
    }

    /// Check if a keyboard is enabled in config
    /// DEFAULT: All keyboards are DISABLED unless explicitly in the enabled set
    fn is_keyboard_enabled(&self, id: &KeyboardId) -> bool {
        let result = match &self.config.enabled_keyboards {
            None => {
                // No config = all disabled by default
                debug!("enabled_keyboards is None, {} is DISABLED (default)", id);
                false
            }
            Some(set) => {
                let contains = set.contains(id.as_str());
                debug!("Checking if {} is in enabled set: {} (set has {} items)", id, contains, set.len());
                contains
            }
        };
        result
    }

    /// Initialize enabled_keyboards set if needed (creates empty set by default)
    fn init_enabled_set(&mut self) {
        if self.config.enabled_keyboards.is_none() {
            // Create an EMPTY set - all keyboards disabled by default
            info!("Initializing enabled_keyboards with empty set (all disabled by default)");
            self.config.enabled_keyboards = Some(std::collections::HashSet::new());
        }
    }

    /// Start a keyboard thread
    fn start_keyboard_thread(
        &mut self,
        id: KeyboardId,
        device: evdev::Device,
        name: String,
    ) -> Result<()> {
        info!("Starting keyboard thread for: {} (ID: {})", name, id);

        // Each keyboard thread gets its own niri monitor
        // This is simpler than broadcasting and works well
        let (niri_tx, niri_rx) = mpsc::channel();
        niri::start_niri_monitor(niri_tx);

        let thread = KeyboardThread::spawn(
            id.clone(),
            device,
            name.clone(),
            niri_rx,
            self.config.password.clone(),
        )?;

        self.threads.insert(id.clone(), thread);
        info!("Thread started and inserted into HashMap for: {} (ID: {}), total threads: {}", name, id, self.threads.len());
        Ok(())
    }

    /// Handle hotplug events (keyboard connect/disconnect)
    fn handle_hotplug(&mut self) -> Result<()> {
        // Re-discover keyboards
        self.discover_keyboards()?;

        // Start threads for newly connected, enabled keyboards
        let keyboards = find_all_keyboards()?;
        for (id, (device, name)) in keyboards {
            if self.is_keyboard_enabled(&id) && !self.threads.contains_key(&id) {
                info!("Hotplug: Starting thread for newly connected keyboard: {} ({})", name, id);
                self.start_keyboard_thread(id, device, name)?;
            }
        }

        // Stop threads for disconnected keyboards
        let disconnected: Vec<KeyboardId> = self
            .all_keyboards
            .iter()
            .filter(|(id, meta)| !meta.connected && self.threads.contains_key(id))
            .map(|(id, _)| id.clone())
            .collect();

        for id in disconnected {
            if let Some(mut thread) = self.threads.remove(&id) {
                info!("Hotplug: Stopping thread for disconnected keyboard: {} ({})", thread.name, id);
                thread.shutdown();
            }
        }

        Ok(())
    }

    /// Cleanup threads that have stopped
    fn cleanup_dead_threads(&mut self) {
        let dead: Vec<KeyboardId> = self
            .threads
            .iter()
            .filter(|(_, thread)| !thread.is_running())
            .map(|(id, _)| id.clone())
            .collect();

        for id in dead {
            if let Some(thread) = self.threads.remove(&id) {
                warn!("Removing dead thread: {} ({})", thread.name, id);
            }
        }
    }

    /// Shutdown all running threads
    fn shutdown_all_threads(&mut self) {
        for (_, mut thread) in self.threads.drain() {
            info!("Shutting down thread: {} ({})", thread.name, thread.keyboard_id);
            thread.shutdown();
        }
    }

    /// Handle an IPC request
    fn handle_ipc_request(&mut self, request: IpcRequest) -> IpcResponse {
        debug!("Handling IPC request: {:?}", request);

        match request {
            IpcRequest::Ping => IpcResponse::Pong,

            IpcRequest::ListKeyboards => {
                let mut infos = Vec::new();

                for (id, meta) in &self.all_keyboards {
                    let enabled = self.is_keyboard_enabled(id);
                    infos.push(KeyboardInfo {
                        hardware_id: id.to_string(),
                        name: meta.name.clone(),
                        device_path: meta.device_path.clone().unwrap_or_else(|| "unknown".to_string()),
                        enabled,
                        connected: meta.connected,
                    });
                }

                IpcResponse::KeyboardList(infos)
            }

            IpcRequest::EnableKeyboard(hardware_id) => {
                self.enable_keyboard(&hardware_id)
            }

            IpcRequest::DisableKeyboard(hardware_id) => {
                self.disable_keyboard(&hardware_id)
            }

            IpcRequest::ToggleKeyboards => {
                // Return current list for client to handle interactively
                let mut infos = Vec::new();
                for (id, meta) in &self.all_keyboards {
                    let enabled = self.is_keyboard_enabled(id);
                    infos.push(KeyboardInfo {
                        hardware_id: id.to_string(),
                        name: meta.name.clone(),
                        device_path: meta.device_path.clone().unwrap_or_else(|| "unknown".to_string()),
                        enabled,
                        connected: meta.connected,
                    });
                }
                IpcResponse::KeyboardList(infos)
            }

            IpcRequest::Shutdown => {
                info!("Shutdown requested via IPC");
                IpcResponse::Ok
            }
        }
    }

    /// Enable a keyboard by hardware ID
    fn enable_keyboard(&mut self, hardware_id: &str) -> IpcResponse {
        let id = KeyboardId::from_string(hardware_id.to_string());
        info!("Enable keyboard requested: {}", hardware_id);

        // Ensure enabled set is initialized
        self.init_enabled_set();

        // Update config
        if let Some(enabled_set) = &mut self.config.enabled_keyboards {
            let was_inserted = enabled_set.insert(hardware_id.to_string());
            info!("Config updated, inserted: {}, set now has {} items", was_inserted, enabled_set.len());
        }

        // Save config
        if let Err(e) = self.config.save(&self.config_path) {
            error!("Failed to save config: {}", e);
            return IpcResponse::Error(format!("Failed to save config: {}", e));
        }
        info!("Config saved to {:?}", self.config_path);

        // Start thread if keyboard is connected and not already running
        if !self.threads.contains_key(&id) {
            if let Ok(mut keyboards) = find_all_keyboards() {
                if let Some((device, name)) = keyboards.remove(&id) {
                    match self.start_keyboard_thread(id.clone(), device, name) {
                        Ok(_) => info!("Enabled and started keyboard: {}", hardware_id),
                        Err(e) => {
                            error!("Failed to start keyboard: {}", e);
                            return IpcResponse::Error(format!("Failed to start keyboard: {}", e));
                        }
                    }
                } else {
                    info!("Enabled keyboard {} (not currently connected)", hardware_id);
                }
            }
        } else {
            info!("Keyboard {} already running", hardware_id);
        }

        IpcResponse::Ok
    }

    /// Disable a keyboard by hardware ID
    fn disable_keyboard(&mut self, hardware_id: &str) -> IpcResponse {
        let id = KeyboardId::from_string(hardware_id.to_string());
        info!("Disable keyboard requested: {}", hardware_id);

        // Ensure enabled set is initialized
        self.init_enabled_set();

        // Update config
        if let Some(enabled_set) = &mut self.config.enabled_keyboards {
            let was_removed = enabled_set.remove(hardware_id);
            info!("Config updated, removed: {}, set now has {} items", was_removed, enabled_set.len());
        }

        // Save config
        if let Err(e) = self.config.save(&self.config_path) {
            error!("Failed to save config: {}", e);
            return IpcResponse::Error(format!("Failed to save config: {}", e));
        }
        info!("Config saved to {:?}", self.config_path);

        // Stop thread if running
        if let Some(mut thread) = self.threads.remove(&id) {
            thread.shutdown();
            info!("Disabled and stopped keyboard: {}", hardware_id);
        } else {
            info!("Keyboard {} was not running", hardware_id);
        }

        IpcResponse::Ok
    }
}
