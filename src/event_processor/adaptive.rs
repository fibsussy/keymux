use crate::event_processor::actions::mt::RollingStats;
use crate::keycode::KeyCode;
use std::collections::HashMap;

pub struct AdaptiveProcessor {
    all_key_stats: HashMap<KeyCode, RollingStats>,
    key_press_times: HashMap<KeyCode, std::time::Instant>,
}

impl AdaptiveProcessor {
    pub fn new() -> Self {
        Self {
            all_key_stats: HashMap::new(),
            key_press_times: HashMap::new(),
        }
    }

    pub fn record_key_press(&mut self, keycode: KeyCode) {
        self.key_press_times
            .insert(keycode, std::time::Instant::now());
    }

    pub fn record_key_release(&mut self, keycode: KeyCode, is_game_mode: bool) -> Option<f32> {
        if let Some(press_time) = self.key_press_times.remove(&keycode) {
            let duration_ms = press_time.elapsed().as_millis() as f32;
            let threshold_ms = 130.0;
            if duration_ms < threshold_ms && !is_game_mode {
                let stats = self
                    .all_key_stats
                    .entry(keycode)
                    .or_insert_with(|| RollingStats::new(threshold_ms));
                stats.update_tap(duration_ms, 30.0);
                return Some(duration_ms);
            }
        }
        None
    }

    pub fn save_adaptive_stats(&self, user_id: u32) -> Result<(), std::io::Error> {
        let home = Self::get_user_home(user_id);
        let all_path =
            std::path::PathBuf::from(format!("{}/.config/keymux/all_key_stats.json", home));
        self.save_all_key_stats(&all_path)?;
        Ok(())
    }

    pub fn load_adaptive_stats(&mut self, user_id: u32) -> Result<(), std::io::Error> {
        let home = Self::get_user_home(user_id);
        let all_path =
            std::path::PathBuf::from(format!("{}/.config/keymux/all_key_stats.json", home));
        self.load_all_key_stats(&all_path)?;
        Ok(())
    }

    fn save_all_key_stats(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if self.all_key_stats.is_empty() {
            return Ok(());
        }
        let mut stats_map: std::collections::HashMap<String, RollingStats> =
            std::collections::HashMap::new();
        for (keycode, stats) in &self.all_key_stats {
            let key_str = format!("{:?}", keycode).replace("KC_", "");
            stats_map.insert(key_str, stats.clone());
        }
        let json = serde_json::to_string_pretty(&stats_map)?;
        std::fs::write(path, json)?;
        tracing::info!("ALL KEYS: Saved {} key stats", self.all_key_stats.len());
        Ok(())
    }

    fn load_all_key_stats(&mut self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if !path.exists() {
            return Ok(());
        }
        let json = std::fs::read_to_string(path)?;
        let stats_map: std::collections::HashMap<String, RollingStats> =
            serde_json::from_str(&json)?;
        self.all_key_stats.clear();
        for (key_str, stats) in stats_map {
            let key_json = format!("\"KC_{}\"", key_str);
            if let Ok(keycode) = serde_json::from_str::<KeyCode>(&key_json) {
                self.all_key_stats.insert(keycode, stats);
            }
        }
        tracing::info!("ALL KEYS: Loaded {} key stats", self.all_key_stats.len());
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_all_key_stats(&self) -> HashMap<KeyCode, RollingStats> {
        self.all_key_stats.clone()
    }

    fn get_user_home(user_id: u32) -> String {
        use std::process::Command;
        let output = Command::new("getent")
            .args(["passwd", &user_id.to_string()])
            .output();
        if let Ok(output) = output {
            if let Ok(line) = String::from_utf8(output.stdout) {
                if let Some(home) = line.split(':').nth(5) {
                    return home.trim().to_string();
                }
            }
        }
        "/root".to_string()
    }
}

impl Default for AdaptiveProcessor {
    fn default() -> Self {
        Self::new()
    }
}
