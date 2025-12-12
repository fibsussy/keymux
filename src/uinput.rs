use anyhow::Result;
use evdev::{uinput::VirtualDeviceBuilder, AttributeSet, EventType, InputEvent, Key};
use std::collections::HashSet;
use std::thread;
use std::time::Duration;

pub struct VirtualKeyboard {
    device: evdev::uinput::VirtualDevice,
    active_socd_keys: HashSet<Key>,
}

impl VirtualKeyboard {
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<Key>::new();

        // Register all keyboard keys
        for i in 0..256 {
            let key = Key::new(i);
            keys.insert(key);
        }

        let device = VirtualDeviceBuilder::new()?
            .name("Keyboard Middleware Virtual Keyboard")
            .with_keys(&keys)?
            .build()?;

        // Give udev/systemd time to recognize the device
        thread::sleep(Duration::from_millis(200));

        Ok(Self {
            device,
            active_socd_keys: HashSet::new(),
        })
    }

    pub fn press_key(&mut self, key: Key) -> Result<()> {
        let events = [
            InputEvent::new(EventType::KEY, key.code(), 1),
            InputEvent::new(EventType::SYNCHRONIZATION, 0, 0), // SYN_REPORT
        ];
        self.device.emit(&events)?;
        Ok(())
    }

    pub fn release_key(&mut self, key: Key) -> Result<()> {
        let events = [
            InputEvent::new(EventType::KEY, key.code(), 0),
            InputEvent::new(EventType::SYNCHRONIZATION, 0, 0), // SYN_REPORT
        ];
        self.device.emit(&events)?;
        Ok(())
    }

    pub fn tap_key(&mut self, key: Key) -> Result<()> {
        // Emit press + release as a single batch with SYN
        let events = [
            InputEvent::new(EventType::KEY, key.code(), 1),
            InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
            InputEvent::new(EventType::KEY, key.code(), 0),
            InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ];
        self.device.emit(&events)?;
        Ok(())
    }

    pub fn update_socd_keys(&mut self, new_keys: HashSet<Key>) -> Result<()> {
        // Collect keys to release and press first
        let keys_to_release: Vec<Key> = self
            .active_socd_keys
            .difference(&new_keys)
            .copied()
            .collect();
        let keys_to_press: Vec<Key> = new_keys
            .difference(&self.active_socd_keys)
            .copied()
            .collect();

        // Release keys that are no longer active
        for key in keys_to_release {
            self.release_key(key)?;
        }

        // Press keys that are newly active
        for key in keys_to_press {
            self.press_key(key)?;
        }

        self.active_socd_keys = new_keys;
        Ok(())
    }
}
