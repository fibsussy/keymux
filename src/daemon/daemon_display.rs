use crate::ipc::{send_request, IpcRequest};
use colored::Colorize;

pub struct DaemonDisplay {
    pub terminal_width: usize,
}

impl Default for DaemonDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonDisplay {
    pub fn new() -> Self {
        Self {
            terminal_width: crate::ui::window::get_terminal_width(),
        }
    }

    pub fn print_daemon_status(&self) {
        println!("{}", "📡 Daemon Status:".bright_yellow().bold());

        match send_request(&IpcRequest::Ping) {
            Ok(_) => {
                println!("  Status: {}", "✓ Running".bright_green());

                // Get keyboard list from daemon
                match send_request(&IpcRequest::ListKeyboards) {
                    Ok(crate::ipc::IpcResponse::KeyboardList(keyboards)) => {
                        self.print_keyboard_list(&keyboards);
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
                self.print_daemon_not_running(&e);
            }
        }
    }

    fn print_keyboard_list(&self, keyboards: &[crate::ipc::KeyboardInfo]) {
        println!(
            "  Active Keyboards: {}",
            keyboards.len().to_string().bright_blue()
        );

        // Calculate if table format fits
        let use_table_format =
            self.calculate_keyboard_table_width(keyboards) <= self.terminal_width;

        if use_table_format && !keyboards.is_empty() {
            self.print_keyboard_table(keyboards);
        } else {
            self.print_keyboard_list_paragraph(keyboards);
        }
    }

    fn calculate_keyboard_table_width(&self, keyboards: &[crate::ipc::KeyboardInfo]) -> usize {
        if keyboards.is_empty() {
            return 0;
        }

        let mut max_name_width = 4; // "Name"
        let mut max_hw_id_width = 5; // "HW ID"
        let mut max_status_width = 8; // "Status"

        for kbd in keyboards {
            max_name_width = max_name_width.max(kbd.name.len());
            max_hw_id_width = max_hw_id_width.max(kbd.hardware_id.len());
            max_status_width = max_status_width.max(Self::status_str(kbd).len());
        }

        max_name_width + max_hw_id_width + max_status_width + 8 // spacing
    }

    fn status_str(kbd: &crate::ipc::KeyboardInfo) -> String {
        match (&kbd.enabled, &kbd.matched_rule) {
            (true, Some(rule)) => format!("✓ Enabled by \"{}\"", rule),
            (true, None) => "✓ Enabled implicitly".to_string(),
            (false, Some(rule)) => format!("○ Disabled by \"{}\"", rule),
            (false, None) => "○ Disabled implicitly".to_string(),
        }
    }

    fn print_keyboard_table(&self, keyboards: &[crate::ipc::KeyboardInfo]) {
        let mut max_name_width = 4;
        let mut max_hw_id_width = 5; // "HW ID"

        for kbd in keyboards {
            max_name_width = max_name_width.max(kbd.name.len());
            max_hw_id_width = max_hw_id_width.max(kbd.hardware_id.len());
        }

        // Header
        println!(
            "  {:<width_name$}  {:<width_hw$}  {}",
            "Name".bright_white().bold(),
            "HW ID".bright_white().bold(),
            "Status".bright_white().bold(),
            width_name = max_name_width,
            width_hw = max_hw_id_width
        );

        // Separator
        let separator_width = max_name_width + max_hw_id_width + 16;
        println!("  {}", "─".repeat(separator_width).dimmed());

        // Data rows
        for kbd in keyboards {
            let status = match (&kbd.enabled, &kbd.matched_rule) {
                (true, Some(rule)) => format!("✓ Enabled by \"{}\"", rule).bright_green(),
                (true, None) => "✓ Enabled implicitly".bright_green(),
                (false, Some(rule)) => format!("○ Disabled by \"{}\"", rule).dimmed(),
                (false, None) => "○ Disabled implicitly".dimmed(),
            };

            println!(
                "  {:<width_name$}  {:<width_hw$}  {}",
                kbd.name.bright_white(),
                kbd.hardware_id.dimmed(),
                status,
                width_name = max_name_width,
                width_hw = max_hw_id_width,
            );
        }
    }

    fn print_keyboard_list_paragraph(&self, keyboards: &[crate::ipc::KeyboardInfo]) {
        for kbd in keyboards {
            let status = match (&kbd.enabled, &kbd.matched_rule) {
                (true, Some(rule)) => format!("✓ Enabled by \"{}\"", rule).bright_green(),
                (true, None) => "✓ Enabled implicitly".bright_green(),
                (false, Some(rule)) => format!("○ Disabled by \"{}\"", rule).dimmed(),
                (false, None) => "○ Disabled implicitly".dimmed(),
            };

            println!("    - {} ({})", kbd.name.bright_white(), status);
            println!("      HW ID: {}", kbd.hardware_id.dimmed());
            println!("      Path: {}", kbd.device_path.dimmed());
        }
    }

    fn print_daemon_not_running(&self, e: &anyhow::Error) {
        println!("  Status: {}", "✗ Not running".bright_red());
        println!("  Error: {}", e.to_string().dimmed());
        println!("  Start: {}", "sudo systemctl start keymux".bright_white());
    }
}
