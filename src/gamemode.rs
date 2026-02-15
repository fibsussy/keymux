use crate::cli::{GamemodeAction, WindowGamemodeAction};
use anyhow::Result;

pub fn handle_gamemode_action(action: &GamemodeAction) -> Result<()> {
    match action {
        GamemodeAction::Window { action } => {
            handle_window_gamemode_action(action)?;
        }
    }
    Ok(())
}

pub fn handle_window_gamemode_action(action: &WindowGamemodeAction) -> Result<()> {
    use colored::Colorize;

    match action {
        WindowGamemodeAction::Invert => {
            println!("  {} Window invert not implemented yet", "ℹ".bright_blue());
        }
        WindowGamemodeAction::ToggleInvert => {
            println!(
                "  {} Window toggle-invert not implemented yet",
                "ℹ".bright_blue()
            );
        }
        WindowGamemodeAction::Normal => {
            println!("  {} Window normal not implemented yet", "ℹ".bright_blue());
        }
        WindowGamemodeAction::AlwaysOn => {
            println!(
                "  {} Window always-on not implemented yet",
                "ℹ".bright_blue()
            );
        }
        WindowGamemodeAction::AlwaysOff => {
            println!(
                "  {} Window always-off not implemented yet",
                "ℹ".bright_blue()
            );
        }
        WindowGamemodeAction::List => {
            println!("  {} Window list not implemented yet", "ℹ".bright_blue());
        }
    }
    Ok(())
}
