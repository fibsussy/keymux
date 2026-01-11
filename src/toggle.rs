use anyhow::Result;
use colored::Colorize;
use dialoguer::MultiSelect;

use crate::config::Config;
use crate::ipc::{send_request, IpcRequest, IpcResponse};
use crate::keyboard_id::{find_all_keyboards, KeyboardId};

pub fn run_toggle() -> Result<()> {
    println!();
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!("  {}", "Keyboard Configuration".bright_cyan().bold());
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    // Load current config
    let config_path = Config::default_path()?;
    let mut config = Config::load(&config_path)?;

    // Find all keyboards
    let keyboards = find_all_keyboards();

    if keyboards.is_empty() {
        println!(
            "  {} {}",
            "✗".bright_red().bold(),
            "No keyboards found!".red()
        );
        println!();
        return Ok(());
    }

    // Build list of keyboard items for selection
    let mut items: Vec<(KeyboardId, String)> = keyboards
        .into_iter()
        .map(|(id, logical_kb)| (id, logical_kb.name))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    // Get current enabled keyboards set
    let enabled_set: Vec<String> = config.enabled_keyboards.clone().unwrap_or_default();

    // Show current status
    println!("  {}", "Current Status:".bright_white().bold());
    println!();

    let mut has_enabled = false;
    let mut has_disabled = false;

    for (id, name) in &items {
        if enabled_set.contains(&id.to_string()) {
            println!("    {} {}", "✓".bright_green(), name.green());
            has_enabled = true;
        }
    }

    if !has_enabled {
        println!("    {}", "(none enabled)".dimmed());
    }

    println!();

    for (id, name) in &items {
        if !enabled_set.contains(&id.to_string()) {
            println!("    {} {}", "○".dimmed(), name.dimmed());
            has_disabled = true;
        }
    }

    if !has_disabled {
        println!("    {}", "(all keyboards enabled)".dimmed());
    }

    println!();
    println!("{}", "  ─────────────────────────────────────".dimmed());
    println!();

    // Build simple display items (dialoguer will add styling)
    let display_items: Vec<String> = items.iter().map(|(_id, name)| name.clone()).collect();

    // Determine which items are pre-selected (currently enabled)
    let defaults: Vec<bool> = items
        .iter()
        .map(|(id, _name)| enabled_set.contains(&id.to_string()))
        .collect();

    println!("  {}", "Select keyboards to enable:".bright_white());
    println!("  {}", "(Space to toggle, Enter to confirm)".dimmed());
    println!();

    // Show multi-select dialog with custom theme
    use console::Style;
    use dialoguer::theme::ColorfulTheme;

    let theme = ColorfulTheme {
        checked_item_prefix: Style::new().green().apply_to("✓".to_string()),
        unchecked_item_prefix: Style::new().dim().apply_to("○".to_string()),
        active_item_prefix: Style::new().cyan().apply_to(">".to_string()),
        inactive_item_prefix: Style::new().apply_to(" ".to_string()),
        active_item_style: Style::new().cyan(),
        ..ColorfulTheme::default()
    };

    let selections = MultiSelect::with_theme(&theme)
        .items(&display_items)
        .defaults(&defaults)
        .interact()?;

    // Update config with selected keyboards
    let enabled_keyboards: Vec<String> =
        selections.iter().map(|&i| items[i].0.to_string()).collect();

    config.enabled_keyboards = Some(enabled_keyboards.clone());

    // Save config (preserving format)
    config.save_enabled_keyboards_only(&config_path)?;

    // Send IPC request to daemon to hot-reload
    match send_request(&IpcRequest::ToggleKeyboards) {
        Ok(IpcResponse::Ok) => {
            // Daemon hot-reloaded successfully
        }
        Ok(_) => {
            println!(
                "  {} {}",
                "⚠".bright_yellow(),
                "Unexpected response from daemon".yellow()
            );
        }
        Err(e) => {
            println!(
                "  {} {}",
                "⚠".bright_yellow(),
                format!("Daemon not running: {e}").yellow()
            );
            println!(
                "  {} Start it with: {}",
                "Tip:".bright_yellow().bold(),
                "sudo systemctl start keyboard-middleware".dimmed()
            );
        }
    }

    println!();
    println!(
        "  {} {}",
        "✓".bright_green().bold(),
        "Configuration saved!".green()
    );
    println!();

    if enabled_keyboards.is_empty() {
        println!(
            "  {} {}",
            "⚠".bright_yellow(),
            "No keyboards enabled".yellow()
        );
    } else {
        println!("  {}", "Enabled keyboards:".bright_white());
        for id in &enabled_keyboards {
            if let Some((_, name)) = items.iter().find(|(kid, _)| kid.to_string() == *id) {
                println!("    {} {}", "✓".bright_green(), name.green());
            }
        }
    }

    println!();

    Ok(())
}
