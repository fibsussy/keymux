use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::ipc::{send_request, IpcRequest};
use crate::keyboard_id::find_all_keyboards;
use crate::window::{get_all_windows, get_terminal_width, GameModeState};

pub fn run_debug() -> Result<()> {
    println!();
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!("  {}", "Debug Information".bright_cyan().bold());
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    // Thread and process info
    println!("{}", "🧵 Process Info:".bright_yellow().bold());
    println!("  PID: {}", std::process::id().to_string().bright_white());
    println!("  Thread ID: {:?}", std::thread::current().id());

    let (actual_uid, is_sudo) = keyboard_middleware::get_actual_user_uid();

    if is_sudo {
        println!(
            "  User: {} {}",
            std::env::var("SUDO_USER")
                .unwrap_or_else(|_| "unknown".to_string())
                .bright_white(),
            format!("(UID: {})", actual_uid).dimmed()
        );
        println!("  Running with: {}", "sudo".bright_yellow());
    } else {
        println!(
            "  User: {}",
            std::env::var("USER")
                .unwrap_or_else(|_| "unknown".to_string())
                .bright_white()
        );
        println!(
            "  UID: {}",
            unsafe { libc::getuid() }.to_string().bright_white()
        );
    }
    println!();

    // Config info
    println!("{}", "⚙️  Configuration:".bright_yellow().bold());
    let config_path = Config::default_path()?;
    println!(
        "  Config Path: {}",
        config_path.display().to_string().bright_white()
    );

    if config_path.exists() {
        println!("  Status: {}", "✓ Exists".bright_green());
        match Config::load(&config_path) {
            Ok(config) => {
                if let Some(enabled_keyboards) = &config.enabled_keyboards {
                    println!(
                        "  Enabled Keyboards: {}",
                        enabled_keyboards.len().to_string().bright_blue()
                    );
                    for kb in enabled_keyboards {
                        println!("    - {}", kb.bright_white());
                    }
                } else {
                    println!("  Enabled Keyboards: {}", "All (none specified)".dimmed());
                }

                println!(
                    "  Tapping Term: {}ms",
                    config.tapping_term_ms.to_string().bright_blue()
                );

                if config.mt_config.double_tap_then_hold {
                    println!(
                        "  Double Tap Window: {}ms",
                        config
                            .mt_config
                            .double_tap_window_ms
                            .to_string()
                            .bright_blue()
                    );
                }

                println!(
                    "  Layers: {}",
                    config.layers.len().to_string().bright_blue()
                );
                println!(
                    "  Base Remaps: {}",
                    config.remaps.len().to_string().bright_blue()
                );
                println!(
                    "  Game Mode Remaps: {}",
                    config.game_mode.remaps.len().to_string().bright_blue()
                );
            }
            Err(e) => {
                println!("  Status: {}", format!("✗ Error: {}", e).bright_red());
            }
        }
    } else {
        println!("  Status: {}", "✗ Not found".bright_red());
        println!("  Hint: Copy config.example.ron to {:?}", config_path);
    }
    println!();

    // Device watching info
    println!("{}", "👀 Device Watching:".bright_yellow().bold());

    // Check /dev/input directory
    if Path::new("/dev/input").exists() {
        println!("  /dev/input: {}", "✓ Accessible".bright_green());

        if let Ok(entries) = fs::read_dir("/dev/input") {
            let mut event_files = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("event") {
                        event_files.push(path);
                    }
                }
            }

            println!(
                "  Event Files Found: {}",
                event_files.len().to_string().bright_blue()
            );

            // Only show keyboard event files
            for event_file in &event_files {
                if let Ok(device) = evdev::Device::open(event_file) {
                    // Check if it's a keyboard
                    if let Some(keys) = device.supported_keys() {
                        let has_letter_keys = keys.contains(evdev::Key::KEY_A)
                            && keys.contains(evdev::Key::KEY_Z)
                            && keys.contains(evdev::Key::KEY_SPACE);

                        if has_letter_keys {
                            let display = event_file.display().to_string();
                            println!("    - {}", display.bright_white());

                            if let Some(name) = device.name() {
                                println!("      Name: {}", name.dimmed());
                            }

                            let input_id = device.input_id();
                            println!(
                                "      ID: {:04x}:{:04x}:{:04x}:{:04x}",
                                input_id.vendor(),
                                input_id.product(),
                                input_id.version(),
                                input_id.bus_type().0
                            );
                        }
                    }
                }
            }
        }
    } else {
        println!("  /dev/input: {}", "✗ Not accessible".bright_red());
        println!("  Make sure you're in the 'input' group");
    }
    println!();

    // Keyboard mapping info
    println!("{}", "🗺️  Keyboard Mapping:".bright_yellow().bold());
    let keyboards = find_all_keyboards();

    if keyboards.is_empty() {
        println!("  {}", "No keyboards found".bright_white());
    } else {
        println!(
            "  Logical keyboards: {}",
            keyboards.len().to_string().bright_blue()
        );
        println!();

        for (id, logical_kb) in keyboards {
            println!("    {}", logical_kb.name.bright_white());
            println!(
                "    {} {}",
                "Hardware ID:".dimmed(),
                id.to_string().dimmed()
            );
            println!(
                "    {} {} device(s)",
                "Devices:".dimmed(),
                logical_kb.devices.len().to_string().bright_blue()
            );

            for (i, (path, _)) in logical_kb.devices.iter().enumerate() {
                let device_type = if i == 0 { "primary" } else { "secondary" };
                println!("      - {} ({})", path.display(), device_type.dimmed());
            }
            println!();
        }
    }

    // Permissions info
    println!("{}", "🔒 Permissions:".bright_yellow().bold());

    // Check input group
    if let Ok(output) = std::process::Command::new("groups").output() {
        let groups_str = String::from_utf8_lossy(&output.stdout);
        let in_input_group = groups_str.contains("input");

        if in_input_group {
            println!("  Input Group: {}", "✓ Member".bright_green());
        } else {
            println!("  Input Group: {}", "✗ Not member".bright_red());
            println!("  Run: {}", "sudo usermod -a -G input $USER".bright_white());
        }
    }

    // Check if running as root
    let is_root = unsafe { libc::getuid() } == 0;
    if is_root {
        println!("  Root Access: {}", "✓ Running as root".bright_green());
    } else {
        println!("  Root Access: {}", "○ Running as user".dimmed());
    }

    println!();

    // Daemon status
    println!("{}", "📡 Daemon Status:".bright_yellow().bold());

    match send_request(&IpcRequest::Ping) {
        Ok(_) => {
            println!("  Status: {}", "✓ Running".bright_green());

            // Get keyboard list from daemon
            match send_request(&IpcRequest::ListKeyboards) {
                Ok(crate::ipc::IpcResponse::KeyboardList(keyboards)) => {
                    println!(
                        "  Active Keyboards: {}",
                        keyboards.len().to_string().bright_blue()
                    );
                    for kbd in keyboards {
                        let status = if kbd.enabled {
                            "✓ Enabled".bright_green()
                        } else {
                            "○ Disabled".dimmed()
                        };
                        println!("    - {} ({})", kbd.name.bright_white(), status);
                        println!("      HW ID: {}", kbd.hardware_id.dimmed());
                        println!("      Path: {}", kbd.device_path.dimmed());
                    }
                }
                Ok(other) => {
                    println!("  Unexpected response: {:?}", other);
                }
                Err(e) => {
                    println!("  Failed to get keyboard list: {}", e);
                }
            }
        }
        Err(e) => {
            println!("  Status: {}", "✗ Not running".bright_red());
            println!("  Error: {}", e.to_string().dimmed());
            println!(
                "  Start: {}",
                "sudo systemctl start keyboard-middleware".bright_white()
            );
        }
    }

    // Window info
    println!("{}", "🪟 Window Info:".bright_yellow().bold());

    match get_all_windows() {
        Ok(windows) => {
            let terminal_width = get_terminal_width();

            println!(
                "  Total Windows: {}",
                windows.len().to_string().bright_blue()
            );
            println!();

            // Calculate required width for table format
            let windows_with_gamemode: Vec<_> =
                windows.iter().map(|w| (w, w.game_mode_state())).collect();

            // Calculate column widths based on content
            let mut max_id_width = 6;
            let mut max_title_width = 5; // "Title"
            let mut max_app_id_width = 7; // "App ID"
            let mut max_pid_width = 3; // "PID"
            let mut max_focused_width = 7; // "Focused"
            let mut max_game_mode_width = 9; // "Game Mode"

            for (window, game_state) in &windows_with_gamemode {
                max_id_width = max_id_width.max(window.id.to_string().len());
                max_title_width = max_title_width.min(20).max(window.title.len().min(20));
                max_app_id_width = max_app_id_width.max(window.app_id.len().min(15));
                max_pid_width = max_pid_width.max(window.pid.to_string().len());

                let game_mode_len = match game_state {
                    GameModeState::Normal => 6,                          // "Normal"
                    GameModeState::GameMode(reason) => reason.len() + 2, // "✓ " + reason
                };
                max_game_mode_width = max_game_mode_width.max(game_mode_len);
            }

            // Add spacing between columns (2 spaces)
            let total_table_width = max_id_width
                + max_title_width
                + max_app_id_width
                + max_pid_width
                + max_focused_width
                + max_game_mode_width
                + 10; // 2 spaces * 5 gaps

            // Determine if table fits
            let use_table_format = total_table_width + 4 <= terminal_width; // 4 for "  " margin

            if use_table_format && !windows.is_empty() {
                // Table format
                let title_header = "Title".bright_white().bold();
                let app_id_header = "App ID".bright_white().bold();
                let focused_header = "Focused".bright_white().bold();
                let game_mode_header = "Game Mode".bright_white().bold();

                // Header row
                println!(
                    "  {:<width_id$}  {:<width_title$}  {:<width_app$}  {:<width_pid$}  {:<width_focused$}  {:<width_game$}",
                    "ID".bright_white().bold(),
                    title_header,
                    app_id_header,
                    "PID".bright_white().bold(),
                    focused_header,
                    game_mode_header,
                    width_id = max_id_width,
                    width_title = max_title_width,
                    width_app = max_app_id_width,
                    width_pid = max_pid_width,
                    width_focused = max_focused_width,
                    width_game = max_game_mode_width
                );

                // Separator line
                let separator = "─".repeat(total_table_width);
                println!("  {}", separator.dimmed());

                // Data rows
                for (window, game_state) in &windows_with_gamemode {
                    let focused = if window.is_focused { "✓" } else { "○" };
                    let game_mode = match game_state {
                        GameModeState::Normal => "○ Normal".dimmed().to_string(),
                        GameModeState::GameMode(reason) => {
                            format!("✓ {}", reason.bright_green())
                        }
                    };

                    let title = if window.title.len() > max_title_width {
                        format!("{}...", &window.title[..max_title_width.saturating_sub(3)])
                    } else {
                        window.title.clone()
                    };

                    println!(
                        "  {:<width_id$}  {:<width_title$}  {:<width_app$}  {:<width_pid$}  {:<width_focused$}  {:<width_game$}",
                        window.id.to_string().bright_white(),
                        title,
                        window.app_id,
                        window.pid.to_string(),
                        focused,
                        game_mode,
                        width_id = max_id_width,
                        width_title = max_title_width,
                        width_app = max_app_id_width,
                        width_pid = max_pid_width,
                        width_focused = max_focused_width,
                        width_game = max_game_mode_width
                    );
                }
            } else {
                // Paragraph format for narrow terminals or no windows
                for window in &windows {
                    let game_state = window.game_mode_state();
                    let game_info = match game_state {
                        GameModeState::Normal => {
                            format!("○ Normal")
                        }
                        GameModeState::GameMode(reason) => {
                            format!("✓ {}", reason.bright_green())
                        }
                    };

                    println!("  Window ID: {}", window.id.to_string().bright_white());
                    println!("    Title: {}", window.title.bright_white());
                    println!("    App ID: {}", window.app_id.bright_white());
                    println!("    PID: {}", window.pid.to_string().bright_white());
                    println!("    Game Mode: {}", game_info);
                    println!();
                }
            }
        }
        Err(e) => {
            println!("  {}", format!("✗ Error: {}", e).bright_red());
        }
    }

    println!();

    // Session info
    println!("{}", "👤 User Sessions:".bright_yellow().bold());

    if let Ok(output) = std::process::Command::new("loginctl")
        .arg("list-sessions")
        .arg("--no-legend")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let sessions: Vec<_> = stdout.lines().collect();

        if sessions.is_empty() {
            println!("  {}", "No sessions found".dimmed());
        } else {
            println!(
                "  Active Sessions: {}",
                sessions.len().to_string().bright_blue()
            );
            for line in sessions {
                println!("    {}", line.bright_white());
            }
        }
    } else {
        println!(
            "  {} {}",
            "Could not query sessions".dimmed(),
            "(loginctl not available)".dimmed()
        );
    }

    println!();
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    Ok(())
}
