use anyhow::Result;
use dialoguer::MultiSelect;

use crate::config::Config;
use crate::keyboard_id::{find_all_keyboards, KeyboardId};

pub fn run_toggle() -> Result<()> {
    println!("Keyboard Middleware - Toggle Configuration\n");

    // Load current config
    let config_path = Config::default_path()?;
    let mut config = Config::load(&config_path)?;

    // Find all keyboards
    let keyboards = find_all_keyboards();

    if keyboards.is_empty() {
        println!("No keyboards found!");
        return Ok(());
    }

    // Build list of keyboard items for selection
    let mut items: Vec<(KeyboardId, String)> = keyboards
        .into_iter()
        .map(|(id, (_device, name))| (id, name))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    // Get current enabled keyboards set
    let enabled_set: Vec<String> = config
        .enabled_keyboards
        .clone()
        .unwrap_or_default();

    // Build display items and track which are currently enabled
    let display_items: Vec<String> = items
        .iter()
        .map(|(id, name)| format!("{} ({})", name, id))
        .collect();

    // Determine which items are pre-selected (currently enabled)
    let defaults: Vec<bool> = items
        .iter()
        .map(|(id, _name)| enabled_set.contains(&id.to_string()))
        .collect();

    println!("Select keyboards to enable (Space to toggle, Enter to confirm):\n");

    // Show multi-select dialog
    let selections = MultiSelect::new()
        .items(&display_items)
        .defaults(&defaults)
        .interact()?;

    // Update config with selected keyboards
    let enabled_keyboards: Vec<String> = selections
        .iter()
        .map(|&i| items[i].0.to_string())
        .collect();

    config.enabled_keyboards = Some(enabled_keyboards.clone());

    // Save config
    config.save(&config_path)?;

    println!("\n✓ Configuration saved!");
    println!("\nEnabled keyboards:");
    for id in &enabled_keyboards {
        if let Some((_, name)) = items.iter().find(|(kid, _)| kid.to_string() == *id) {
            println!("  - {} ({})", name, id);
        }
    }

    if enabled_keyboards.is_empty() {
        println!("  (none)");
    }

    Ok(())
}
