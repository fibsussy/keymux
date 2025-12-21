use anyhow::{Context, Result};
use evdev::{Device, InputEvent, Key, AttributeSet};
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use std::sync::mpsc::Receiver;
use std::thread;
use tracing::{error, info, warn};

use crate::keyboard_id::KeyboardId;

/// Process events from a physical keyboard and output to virtual device
/// Returns immediately after spawning thread
/// shutdown_rx: Receiver to signal thread shutdown
pub fn start_event_processor(
    keyboard_id: KeyboardId,
    mut device: Device,
    keyboard_name: String,
    shutdown_rx: Receiver<()>,
) -> Result<()> {
    thread::spawn(move || {
        if let Err(e) = run_event_processor(&keyboard_id, &mut device, &keyboard_name, shutdown_rx) {
            error!("Event processor for {} failed: {}", keyboard_id, e);
        }
        info!("Event processor thread exiting for: {}", keyboard_id);
    });

    Ok(())
}

fn run_event_processor(
    keyboard_id: &KeyboardId,
    device: &mut Device,
    keyboard_name: &str,
    shutdown_rx: Receiver<()>,
) -> Result<()> {
    info!("Starting event processor for: {} ({})", keyboard_name, keyboard_id);

    // Grab the device for exclusive access
    device.grab().context("Failed to grab device")?;
    info!("Grabbed device: {}", keyboard_name);

    // Create virtual uinput device
    let mut virtual_device = create_virtual_device(device, keyboard_name)?;
    info!("Created virtual device for: {}", keyboard_name);

    // Event processing loop
    loop {
        // Check for shutdown signal (non-blocking)
        match shutdown_rx.try_recv() {
            Ok(()) => {
                warn!("Shutdown signal received for: {}", keyboard_name);
                // Ungrab device before exiting
                let _ = device.ungrab();
                return Ok(());
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                warn!("Shutdown channel disconnected for: {}", keyboard_name);
                let _ = device.ungrab();
                return Ok(());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No shutdown signal, continue
            }
        }

        // Read events from physical keyboard
        for ev in device.fetch_events()? {
            let mut output_event = ev;

            // Remap keys: CAPS -> ESC
            if let evdev::EventType::KEY = ev.event_type() {
                if ev.code() == Key::KEY_CAPSLOCK.code() {
                    // Remap CAPS to ESC
                    output_event = InputEvent::new_now(
                        ev.event_type(),
                        Key::KEY_ESC.code(),
                        ev.value(),
                    );
                }
            }

            // Write to virtual device
            virtual_device.emit(&[output_event])?;
        }
    }
}

/// Create a virtual uinput device that mimics the physical keyboard
fn create_virtual_device(
    physical_device: &Device,
    keyboard_name: &str,
) -> Result<VirtualDevice> {
    let mut keys = AttributeSet::<Key>::new();

    // Copy all supported keys from physical device
    if let Some(physical_keys) = physical_device.supported_keys() {
        for key in physical_keys.iter() {
            keys.insert(key);
        }
    }

    // Build virtual device
    let virtual_device = VirtualDeviceBuilder::new()?
        .name(&format!("Keyboard Middleware Virtual Keyboard ({})", keyboard_name))
        .with_keys(&keys)?
        .build()?;

    Ok(virtual_device)
}
