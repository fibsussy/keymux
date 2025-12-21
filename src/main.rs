use anyhow::Result;

mod daemon;
mod keyboard_id;

use daemon::Daemon;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    let mut daemon = Daemon::new()?;
    daemon.run()?;

    Ok(())
}
