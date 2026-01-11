use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::ipc::{send_request, IpcRequest};
use crate::keyboard_id::find_all_keyboards;

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

                if let Some(double_tap) = config.double_tap_window_ms {
                    println!(
                        "  Double Tap Window: {}ms",
                        double_tap.to_string().bright_blue()
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
