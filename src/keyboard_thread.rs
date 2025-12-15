use anyhow::{Context, Result};
use evdev::Device;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{debug, error, info};

use crate::keyboard_id::KeyboardId;
use crate::niri::NiriEvent;
use crate::process_event_new::process_event;
use crate::{KeyboardState, VirtualKeyboard};

/// Command sent to a keyboard thread
pub enum ThreadCommand {
    Shutdown,
}

/// Handle to a running keyboard thread
pub struct KeyboardThread {
    pub keyboard_id: KeyboardId,
    pub name: String,
    handle: Option<JoinHandle<()>>,
    command_tx: mpsc::Sender<ThreadCommand>,
    running: Arc<AtomicBool>,
}

impl KeyboardThread {
    /// Spawn a new keyboard processing thread
    pub fn spawn(
        keyboard_id: KeyboardId,
        mut device: Device,
        name: String,
        niri_rx: Receiver<NiriEvent>,
        password: Option<String>,
    ) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let keyboard_id_clone = keyboard_id.clone();
        let name_clone = name.clone();

        let handle = thread::spawn(move || {
            if let Err(e) = Self::run_keyboard_loop(
                keyboard_id_clone,
                &name_clone,
                &mut device,
                command_rx,
                niri_rx,
                running_clone,
                password,
            ) {
                error!("Keyboard thread {} error: {}", name_clone, e);
            }
        });

        info!("Started keyboard thread: {} ({})", name, keyboard_id);

        Ok(Self {
            keyboard_id,
            name,
            handle: Some(handle),
            command_tx,
            running,
        })
    }

    /// Main keyboard processing loop
    fn run_keyboard_loop(
        keyboard_id: KeyboardId,
        name: &str,
        device: &mut Device,
        command_rx: Receiver<ThreadCommand>,
        niri_rx: Receiver<NiriEvent>,
        running: Arc<AtomicBool>,
        password: Option<String>,
    ) -> Result<()> {
        // Grab the device
        device
            .grab()
            .context("Failed to grab keyboard device")?;

        info!("Grabbed keyboard: {} ({})", name, keyboard_id);

        // Create virtual keyboard (one per physical keyboard thread)
        let mut vkbd = VirtualKeyboard::new()
            .context("Failed to create virtual keyboard")?;

        // Initialize state
        let mut state = KeyboardState::new(password);

        // Main event loop
        loop {
            // Check for shutdown command
            if let Ok(ThreadCommand::Shutdown) = command_rx.try_recv() {
                info!("Keyboard thread {} received shutdown command", name);
                break;
            }

            if !running.load(Ordering::Relaxed) {
                info!("Keyboard thread {} shutting down", name);
                break;
            }

            // Check for niri events
            match niri_rx.try_recv() {
                Ok(NiriEvent::WindowFocusChanged(app_id)) => {
                    let should_enable = crate::niri::should_enable_gamemode(app_id.as_deref());
                    if should_enable && !state.game_mode {
                        info!("🎮 [{}] Entering game mode (gamescope detected)", name);
                        state.game_mode = true;
                        state.layers = vec![crate::Layer::Base, crate::Layer::Game];
                    } else if !should_enable && state.game_mode {
                        info!("💻 [{}] Exiting game mode (left gamescope)", name);
                        state.game_mode = false;
                        state.layers = vec![crate::Layer::Base, crate::Layer::HomeRowMod];
                        state.socd_cleaner.reset();
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No niri events, continue
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    debug!("Niri monitor disconnected in thread {}", name);
                }
            }

            // Process keyboard events
            match device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        // Use blocking runtime for async process_event
                        let runtime = tokio::runtime::Runtime::new().unwrap();
                        if let Err(e) = runtime.block_on(process_event(event, &mut state, &mut vkbd)) {
                            error!("[{}] Error processing event: {}", name, e);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No events available, sleep briefly
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => {
                    error!("[{}] Error fetching events: {}", name, e);
                    break;
                }
            }
        }

        running.store(false, Ordering::Relaxed);
        info!("Keyboard thread {} stopped", name);
        Ok(())
    }

    /// Request thread to shutdown
    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.command_tx.send(ThreadCommand::Shutdown);
    }

    /// Wait for thread to finish
    pub fn join(mut self) -> Result<()> {
        self.shutdown();
        if let Some(handle) = self.handle.take() {
            handle.join().map_err(|_| anyhow::anyhow!("Thread panicked"))?;
        }
        Ok(())
    }

    /// Check if thread is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for KeyboardThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}
