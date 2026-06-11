use std::fs;
use tracing::debug;

/// Result of game mode detection for a window
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameModeState {
    Normal,
    GameMode(String), // Contains reason
}

impl GameModeState {
    pub fn is_game_mode(&self) -> bool {
        matches!(self, GameModeState::GameMode(_))
    }
}

/// Detect whether a window should be in game mode based on app_id, pid, and title.
///
/// This is the single source of truth for game mode detection. Used by:
/// - `keymux debug` (Window Info table)
/// - `keymux niri-daemon` (IPC game mode signaling)
/// - Inline daemon window manager monitor
pub fn detect_game_mode(
    app_id: Option<&str>,
    pid: Option<u32>,
    title: Option<&str>,
) -> GameModeState {
    if let Some(app_id) = app_id {
        // Check app ID first (fastest check)
        if app_id == "gamescope" {
            return GameModeState::GameMode("gamescope window".to_string());
        }

        // Steam games (app ID format: steam_app_<appid>)
        if app_id.starts_with("steam_app_") {
            return GameModeState::GameMode("Steam game".to_string());
        }

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

        // .NET applications (Terraria, Stardew Valley, etc.)
        if app_id == "dotnet" {
            if let Some(title) = title {
                let title_lower = title.to_lowercase();
                let game_name = if title_lower.contains("terraria") {
                    "Terraria"
                } else if title_lower.contains("stardew") || title_lower.contains("stardew valley")
                {
                    "Stardew Valley"
                } else if title_lower.contains("minecraft") {
                    "Minecraft"
                } else if title_lower.contains("hollow knight") {
                    "Hollow Knight"
                } else if title_lower.contains("celeste") {
                    "Celeste"
                } else if title_lower.contains("cuphead") {
                    "Cuphead"
                } else if title_lower.contains("ori") {
                    "Ori series"
                } else if title_lower.contains("dead cells") {
                    "Dead Cells"
                } else if title_lower.contains("hades") {
                    "Hades"
                } else if title_lower.contains("slay the spire") {
                    "Slay the Spire"
                } else {
                    ".NET game"
                };
                return GameModeState::GameMode(game_name.to_string());
            }
            return GameModeState::GameMode(".NET game".to_string());
        }
    }

    // Check environment variables for IS_GAME=1
    if let Some(pid) = pid {
        if check_is_game_env(pid) {
            return GameModeState::GameMode("IS_GAME=1 environment".to_string());
        }

        // Check process tree for gamescope/gamemode
        let (has_gamescope, has_gamemode) = check_process_tree(pid);
        if has_gamescope || has_gamemode {
            let cmdline = get_process_cmdline(pid);
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
    }

    GameModeState::Normal
}

/// Check if a process has `IS_GAME=1` in its environment
fn check_is_game_env(pid: u32) -> bool {
    let env_path = format!("/proc/{pid}/environ");
    match fs::read(&env_path) {
        Ok(contents) => {
            let env_str = String::from_utf8_lossy(&contents);
            for var in env_str.split('\0') {
                if var == "IS_GAME=1" {
                    debug!("Found IS_GAME=1 for PID {pid}");
                    return true;
                }
            }
            false
        }
        Err(e) => {
            debug!("Cannot read environ for PID {pid}: {e}");
            false
        }
    }
}

/// Get process command line for more specific detection
fn get_process_cmdline(pid: u32) -> String {
    let cmdline_path = format!("/proc/{pid}/cmdline");
    fs::read(&cmdline_path).map_or_else(
        |_| String::new(),
        |contents| String::from_utf8_lossy(&contents).replace('\0', " "),
    )
}

/// Check if a process is running through gamescope, gamemode, or custom-gamescope
/// by examining its command line and parent process chain
fn check_process_tree(process_id: u32) -> (bool, bool) {
    let mut has_gamescope = false;
    let mut has_gamemode = false;
    let mut current_pid = process_id;

    // Walk up the process tree (max 10 levels to avoid infinite loops)
    for _ in 0..10 {
        let cmdline_path = format!("/proc/{current_pid}/cmdline");
        if let Ok(contents) = fs::read(&cmdline_path) {
            let cmdline = String::from_utf8_lossy(&contents);
            let cmd_lower = cmdline.to_lowercase();

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
            let parts: Vec<&str> = stat.rsplitn(2, ')').collect();
            if parts.len() == 2 {
                parts[0].split_whitespace().nth(1)?.parse::<u32>().ok()
            } else {
                None
            }
        });

        match parent_pid {
            Some(parent) if parent > 1 => current_pid = parent,
            _ => break,
        }
    }

    (has_gamescope, has_gamemode)
}
