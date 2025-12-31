use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::keyboard_id::{find_all_keyboards, KeyboardId};

pub fn run_list() -> Result<()> {
    println!();
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!("  {}", "Detected Keyboards".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!();

    // Load current config to check enabled keyboards
    let config_path = Config::default_path()?;
    let config = Config::load(&config_path)?;
    let enabled_set: Vec<String> = config.enabled_keyboards.unwrap_or_default();

    // Find all keyboards
    let keyboards = find_all_keyboards();

    if keyboards.is_empty() {
        println!("  {} {}", "✗".bright_red().bold(), "No keyboards found!".red());
        println!();
        println!("  {}", "Make sure you're in the 'input' group:".dimmed());
        println!("  {}", "sudo usermod -a -G input $USER".dimmed());
        println!("  {}", "(Then log out and back in)".dimmed());
        println!();
        return Ok(());
    }

    // Sort keyboards by name
    let mut items: Vec<(KeyboardId, String)> = keyboards
        .into_iter()
        .map(|(id, logical_kb)| (id, logical_kb.name))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    println!("  {}", format!("Found {} keyboard(s):", items.len()).bright_white().bold());
    println!();

    for (id, name) in &items {
        let is_enabled = enabled_set.contains(&id.to_string());

        if is_enabled {
            println!("    {} {}", "✓".bright_green().bold(), name.green());
        } else {
            println!("    {} {}", "○".dimmed(), name.dimmed());
        }
        println!("      {} {}", "ID:".dimmed(), id.to_string().dimmed());
        println!();
    }

    println!("{}", "  ─────────────────────────────────────".dimmed());
    println!();

    // Summary
    let enabled_count = items.iter().filter(|(id, _)| enabled_set.contains(&id.to_string())).count();
    let disabled_count = items.len() - enabled_count;

    if enabled_count == 0 {
        println!("  {} {}", "⚠".bright_yellow(), "No keyboards enabled".yellow());
        println!("  {} Run {} to enable keyboards", "Tip:".bright_yellow().bold(), "keyboard-middleware toggle".bright_white());
    } else {
        println!("  {} {} enabled, {} disabled", "Status:".bright_white().bold(), enabled_count.to_string().green(), disabled_count.to_string().dimmed());
    }

    println!();

    Ok(())
}
