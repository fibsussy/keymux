use evdev::Device;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Hardware-based keyboard identifier that persists across reboots
/// Format: vendor:product:version:bustype (e.g., "2e3c:c365:0110:0003")
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyboardId(String);

/// Represents a logical keyboard with potentially multiple event devices
pub struct LogicalKeyboard {
    /// The base hardware ID
    pub id: KeyboardId,
    /// Human-readable name
    pub name: String,
    /// All event devices for this keyboard (sorted by input number)
    pub devices: Vec<(PathBuf, Device)>,
}

impl std::fmt::Debug for LogicalKeyboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogicalKeyboard")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("device_count", &self.devices.len())
            .finish()
    }
}

impl KeyboardId {
    /// Create a new KeyboardId from a string
    pub const fn new(id: String) -> Self {
        Self(id)
    }

    /// Create a simplified hardware ID from an evdev device
    /// Format: vendor:product:version:bustype
    pub fn from_device(device: &Device) -> Self {
        let id_vendor = device.input_id().vendor();
        let id_product = device.input_id().product();
        let id_version = device.input_id().version();
        let id_bustype = device.input_id().bus_type();

        // Construct simple hardware ID: vendor:product:version:bustype
        let hardware_id = format!(
            "{:04x}:{:04x}:{:04x}:{:04x}",
            id_vendor, id_product, id_version, id_bustype.0
        );

        Self(hardware_id)
    }
}

impl std::fmt::Display for KeyboardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Extract input number from device path (e.g., /dev/input/event12 for input12)
fn get_input_number(path: &Path) -> Option<u32> {
    let event_name = path.file_name()?.to_str()?;
    if !event_name.starts_with("event") {
        return None;
    }

    // Read symlink from /sys/class/input/eventX to find inputY
    let sysfs_path = format!("/sys/class/input/{}", event_name);
    if let Ok(target) = fs::read_link(&sysfs_path) {
        // Target looks like: ../../devices/.../inputX/eventY
        let target_str = target.to_string_lossy();
        for component in target_str.split('/') {
            if let Some(stripped) = component.strip_prefix("input") {
                if let Ok(num) = stripped.parse::<u32>() {
                    return Some(num);
                }
            }
        }
    }

    None
}

/// Find all keyboard devices and return them grouped by hardware ID
/// Each logical keyboard may have multiple event devices (input0, input1, etc.)
pub fn find_all_keyboards() -> HashMap<KeyboardId, LogicalKeyboard> {
    let mut device_groups: HashMap<KeyboardId, Vec<(PathBuf, Device, String, u32)>> =
        HashMap::new();

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

                // Skip mice - check for mouse buttons
                let has_mouse_buttons = keys.contains(evdev::Key::BTN_TOOL_MOUSE)
                    || keys.contains(evdev::Key::BTN_TOOL_FINGER)
                    || keys.contains(evdev::Key::BTN_TOOL_PEN);

                if has_mouse_buttons {
                    tracing::debug!("Skipping mouse device (has mouse buttons): {}", name);
                    continue;
                }

                // Skip mice - check for relative axes (mouse movement)
                if let Some(rel_axes) = device.supported_relative_axes() {
                    let has_mouse_axes = rel_axes.contains(evdev::RelativeAxisType::REL_X)
                        || rel_axes.contains(evdev::RelativeAxisType::REL_Y);

                    if has_mouse_axes {
                        tracing::debug!("Skipping mouse device (has relative axes): {}", name);
                        continue;
                    }
                }

                // Get base hardware ID (without input number)
                let id = KeyboardId::from_device(&device);

                // Get input number for sorting
                let input_num = get_input_number(&path).unwrap_or(999);

                tracing::debug!(
                    "Found keyboard device: '{}' at {} (ID: {}, input: {})",
                    name,
                    path.display(),
                    id,
                    input_num
                );

                device_groups
                    .entry(id)
                    .or_default()
                    .push((path, device, name, input_num));
            }
        }
    }

    // Convert grouped devices into LogicalKeyboards
    let mut keyboards = HashMap::new();
    for (id, mut devices) in device_groups {
        // Sort by input number (lowest first)
        devices.sort_by_key(|(_, _, _, input_num)| *input_num);

        // Use name from first device (lowest input number)
        let name = devices[0].2.clone();
        let lowest_input = devices[0].3;

        tracing::info!(
            "Logical keyboard '{}' (ID: {}) has {} device(s), lowest input: input{}",
            name,
            id,
            devices.len(),
            lowest_input
        );

        let logical_kb = LogicalKeyboard {
            id: id.clone(),
            name,
            devices: devices
                .into_iter()
                .map(|(path, dev, _, _)| (path, dev))
                .collect(),
        };

        keyboards.insert(id, logical_kb);
    }

    keyboards
}
