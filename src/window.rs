use std::fs;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum GameModeState {
    Normal,
    GameMode(String), // Contains reason
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: u32,
    pub title: String,
    pub app_id: String,
    pub pid: u32,
    pub is_floating: bool,
    pub is_focused: bool,
}

impl Window {
    /// Check if this window should be in game mode
    pub fn game_mode_state(&self) -> GameModeState {
        // Check app ID first (fastest check)
        if self.app_id == "gamescope" {
            return GameModeState::GameMode("gamescope window".to_string());
        }

        // Check for Steam games (app ID format: steam_app_<appid>)
        if self.app_id.starts_with("steam_app_") {
            return GameModeState::GameMode("Steam game".to_string());
        }

        // Check for specific gaming applications and platforms
        let app_id = &self.app_id;

        // Wine games
        if app_id.contains("wine") || app_id.contains(".exe") {
            return GameModeState::GameMode("Wine game".to_string());
        }

        // Roblox
        if app_id == "com.roblox.RobloxPlayer" || app_id.contains("roblox") {
            return GameModeState::GameMode("Roblox".to_string());
        }

        // Epic Games Launcher
        if app_id.contains("epicgames") || app_id.contains("epic") {
            return GameModeState::GameMode("Epic Games".to_string());
        }

        // Lutris games
        if app_id.contains("lutris") {
            return GameModeState::GameMode("Lutris game".to_string());
        }

        // Heroic Games Launcher
        if app_id.contains("heroic") {
            return GameModeState::GameMode("Heroic Games".to_string());
        }

        // Sober (virtualization)
        if app_id == "org.vinegarhq.Sober" {
            return GameModeState::GameMode("Sober virtualization".to_string());
        }

        // Proton games
        if app_id.contains("proton") {
            return GameModeState::GameMode("Proton game".to_string());
        }

        // Flatpak games
        if app_id.contains("com") && app_id.contains(".") {
            return GameModeState::GameMode("Flatpak application".to_string());
        }

        // Check for .NET applications (like Terraria, Stardew Valley, etc.)
        if self.app_id == "dotnet" {
            // Detect specific .NET games by window title
            let title_lower = self.title.to_lowercase();
            if title_lower.contains("terraria") {
                return GameModeState::GameMode("Terraria".to_string());
            } else if title_lower.contains("stardew") || title_lower.contains("stardew valley") {
                return GameModeState::GameMode("Stardew Valley".to_string());
            } else if title_lower.contains("minecraft") {
                return GameModeState::GameMode("Minecraft".to_string());
            } else if title_lower.contains("hollow knight") {
                return GameModeState::GameMode("Hollow Knight".to_string());
            } else if title_lower.contains("celeste") {
                return GameModeState::GameMode("Celeste".to_string());
            } else if title_lower.contains("cuphead") {
                return GameModeState::GameMode("Cuphead".to_string());
            } else if title_lower.contains("ori") {
                return GameModeState::GameMode("Ori series".to_string());
            } else if title_lower.contains("dead cells") {
                return GameModeState::GameMode("Dead Cells".to_string());
            } else if title_lower.contains("hades") {
                return GameModeState::GameMode("Hades".to_string());
            } else if title_lower.contains("slay the spire") {
                return GameModeState::GameMode("Slay the Spire".to_string());
            }

            // Generic .NET game detection if title doesn't match known games
            return GameModeState::GameMode(".NET game".to_string());
        }

        // Check environment variables for IS_GAME=1
        if check_is_game_env(self.pid) {
            return GameModeState::GameMode("IS_GAME=1 environment".to_string());
        }

        // Check process tree for gamescope/gamemode with enhanced detection
        let (has_gamescope, has_gamemode) = check_process_tree(self.pid);
        if has_gamescope || has_gamemode {
            // Try to be more specific about what we found
            let cmdline = get_process_cmdline(self.pid);
            let cmdline_lower = cmdline.to_lowercase();

            let reason = if has_gamescope && has_gamemode {
                "gamescope + gamemode"
            } else if has_gamescope {
                if cmdline_lower.contains("steam") || cmdline_lower.contains("steamapps") {
                    "Steam + gamescope"
                } else if cmdline_lower.contains("lutris") {
                    "Lutris + gamescope"
                } else if cmdline_lower.contains("heroic") {
                    "Heroic + gamescope"
                } else {
                    "gamescope wrapper"
                }
            } else if has_gamemode {
                if cmdline_lower.contains("steam") {
                    "Steam + gamemode"
                } else if cmdline_lower.contains("lutris") {
                    "Lutris + gamemode"
                } else if cmdline_lower.contains("heroic") {
                    "Heroic + gamemode"
                } else {
                    "gamemode"
                }
            } else {
                "game launcher"
            };
            return GameModeState::GameMode(reason.to_string());
        }

        GameModeState::Normal
    }
}

/// Get all windows from niri
pub fn get_all_windows() -> Result<Vec<Window>, String> {
    let output = Command::new("niri")
        .args(["msg", "windows"])
        .output()
        .map_err(|e| format!("Failed to run niri: {}", e))?;

    if !output.status.success() {
        return Err("niri command failed".to_string());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_niri_windows(&text)
}

/// Parse niri windows output
fn parse_niri_windows(text: &str) -> Result<Vec<Window>, String> {
    let mut windows = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Look for window header: "Window ID <id>: (focused)"
        if let Some(window_start) = line.strip_prefix("Window ID ") {
            let parts: Vec<&str> = window_start.split(':').collect();
            if parts.len() >= 1 {
                let id_part = parts[0].trim();
                if let Ok(id) = id_part.parse::<u32>() {
                    let is_focused = line.contains("(focused)");

                    // Parse window properties
                    let mut window = Window {
                        id,
                        title: String::new(),
                        app_id: String::new(),
                        pid: 0,
                        is_floating: false,
                        is_focused,
                    };

                    // Look ahead for properties
                    let mut j = i + 1;
                    while j < lines.len() && lines[j].starts_with("  ") {
                        let prop_line = lines[j].trim();

                        if let Some(title) = prop_line.strip_prefix("Title: ") {
                            window.title = title.trim_matches('"').to_string();
                        } else if let Some(app_id) = prop_line.strip_prefix("App ID: ") {
                            window.app_id = app_id.trim_matches('"').to_string();
                        } else if let Some(pid_str) = prop_line.strip_prefix("PID: ") {
                            window.pid = pid_str.parse().unwrap_or(0);
                        } else if prop_line == "Is floating: yes" {
                            window.is_floating = true;
                        }

                        j += 1;
                    }

                    windows.push(window);
                    i = j - 1; // Adjust for outer loop increment
                }
            }
        }

        i += 1;
    }

    Ok(windows)
}

/// Get process command line for more specific detection
fn get_process_cmdline(pid: u32) -> String {
    let cmdline_path = format!("/proc/{pid}/cmdline");
    if let Ok(contents) = fs::read(&cmdline_path) {
        String::from_utf8_lossy(&contents).replace('\0', " ")
    } else {
        String::new()
    }
}

/// Check if a process has `IS_GAME=1` in its environment
fn check_is_game_env(pid: u32) -> bool {
    let env_path = format!("/proc/{pid}/environ");
    if let Ok(contents) = fs::read(&env_path) {
        // Environment variables are null-separated
        let env_str = String::from_utf8_lossy(&contents);
        for var in env_str.split('\0') {
            if var == "IS_GAME=1" {
                return true;
            }
        }
    }
    false
}

/// Check if a process is running through gamescope, gamemode, or custom-gamescope
/// by examining its command line and parent process chain
fn check_process_tree(process_id: u32) -> (bool, bool) {
    let mut has_gamescope = false;
    let mut has_gamemode = false;
    let mut current_pid = process_id;

    // Walk up the process tree (max 10 levels to avoid infinite loops)
    for _ in 0..10 {
        // Check the command line
        let cmdline_path = format!("/proc/{current_pid}/cmdline");
        if let Ok(contents) = fs::read(&cmdline_path) {
            let cmdline = String::from_utf8_lossy(&contents);
            let cmd_lower = cmdline.to_lowercase();

            // Check for gamescope or custom-gamescope wrapper
            if cmd_lower.contains("gamescope") || cmd_lower.contains("custom-gamescope") {
                has_gamescope = true;
            }
            if cmd_lower.contains("gamemode") || cmd_lower.contains("gamemoded") {
                has_gamemode = true;
            }
        }

        // Get parent PID
        let stat_path = format!("/proc/{current_pid}/stat");
        let parent_pid = fs::read_to_string(&stat_path).ok().and_then(|stat| {
            // stat format: pid (comm) state ppid ...
            // Find the last ')' to handle process names with spaces/parens
            let parts: Vec<&str> = stat.rsplitn(2, ')').collect();
            if parts.len() == 2 {
                parts[0].split_whitespace().nth(1)?.parse::<u32>().ok()
            } else {
                None
            }
        });

        match parent_pid {
            Some(parent) if parent > 1 => current_pid = parent,
            _ => break, // Reached init or invalid PID
        }
    }

    (has_gamescope, has_gamemode)
}

/// Get terminal width for responsive formatting
pub fn get_terminal_width() -> usize {
    match crossterm::terminal::size() {
        Ok((width, _)) => width as usize,
        Err(_) => 80, // Default width
    }
}
