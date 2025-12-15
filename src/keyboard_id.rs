use evdev::Device;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Hardware-based keyboard identifier that persists across reboots
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyboardId(String);

/// Try to read the unique identifier (serial number) from sysfs
fn read_device_serial(device_path: &Path) -> Option<String> {
    // Extract event number from /dev/input/eventX
    let event_name = device_path.file_name()?.to_str()?;
    if !event_name.starts_with("event") {
        return None;
    }

    // Try to read serial from sysfs
    // /sys/class/input/eventX/device/id/serial or ../uniq
    let sysfs_base = format!("/sys/class/input/{event_name}/device");

    // Try various locations for unique ID
    let possible_paths = vec![
        format!("{}/uniq", sysfs_base),
        format!("{}/id/serial", sysfs_base),
        format!("{}/../../serial", sysfs_base), // USB device serial
    ];

    for path in possible_paths {
        if let Ok(serial) = fs::read_to_string(&path) {
            let serial = serial.trim();
            if !serial.is_empty() && serial != "0" {
                tracing::debug!("Found serial number at {}: {}", path, serial);
                return Some(serial.to_string());
            }
        }
    }

    None
}

impl KeyboardId {
    /// Create a keyboard ID from an evdev device with device path
    pub fn from_device_with_path(device: &Device, device_path: &Path) -> Self {
        // Try to read unique identifier (serial number) from sysfs
        if let Some(serial) = read_device_serial(device_path) {
            tracing::info!("Using serial number for keyboard: {}", serial);
            return Self(format!("serial:{serial}"));
        }

        // Fall back to hardware properties if no serial available
        let id_product = device.input_id().product();
        let id_vendor = device.input_id().vendor();
        let id_version = device.input_id().version();
        let id_bustype = device.input_id().bus_type();

        // Get physical path as last resort
        let phys = device.physical_path().unwrap_or("unknown");

        // Construct hardware ID: vendor:product:version:bustype:phys
        // BusType doesn't implement LowerHex, so we cast it to u16
        let hardware_id = format!(
            "{:04x}:{:04x}:{:04x}:{:04x}:{}",
            id_vendor, id_product, id_version, id_bustype.0, phys
        );

        tracing::info!("No serial available, using hardware properties: {}", hardware_id);
        Self(hardware_id)
    }

    /// Create a keyboard ID from an evdev device (without path - uses fallback method)
    pub fn from_device(device: &Device) -> Self {
        // Fall back to hardware properties
        let id_product = device.input_id().product();
        let id_vendor = device.input_id().vendor();
        let id_version = device.input_id().version();
        let id_bustype = device.input_id().bus_type();
        let phys = device.physical_path().unwrap_or("unknown");

        let hardware_id = format!(
            "{:04x}:{:04x}:{:04x}:{:04x}:{}",
            id_vendor, id_product, id_version, id_bustype.0, phys
        );

        Self(hardware_id)
    }

    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Create from string
    pub const fn from_string(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for KeyboardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Find all keyboard devices and return them with their IDs
pub fn find_all_keyboards() -> HashMap<KeyboardId, (Device, String)> {
    let mut keyboards = HashMap::new();

    for (path, device) in evdev::enumerate() {
        // Check if it has keyboard keys
        if let Some(keys) = device.supported_keys() {
            let has_letter_keys = keys.contains(evdev::Key::KEY_A)
                && keys.contains(evdev::Key::KEY_Z)
                && keys.contains(evdev::Key::KEY_SPACE);

            if has_letter_keys {
                let name = device.name().unwrap_or("unknown").to_string();

                // Skip virtual keyboards created by this daemon
                if name.contains("Keyboard Middleware Virtual Keyboard") {
                    tracing::debug!("Skipping virtual keyboard: {}", name);
                    continue;
                }

                let id = KeyboardId::from_device_with_path(&device, &path);

                // Log detailed device info
                tracing::info!(
                    "Found keyboard: '{}' at {} (ID: {}, phys: {:?})",
                    name,
                    path.display(),
                    id,
                    device.physical_path()
                );

                keyboards.insert(id, (device, name));
            }
        }
    }

    keyboards
}

/// Find a specific keyboard by its hardware ID
pub fn find_keyboard_by_id(target_id: &KeyboardId) -> Option<(Device, String)> {
    let mut keyboards = find_all_keyboards();
    keyboards.remove(target_id)
}
