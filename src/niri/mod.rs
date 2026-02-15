pub mod niri;
pub mod niri_daemon;

pub use niri::{
    is_niri_available, should_enable_gamemode, start_niri_monitor, start_niri_monitor_sync,
    NiriEvent, WindowInfo,
};
pub use niri_daemon::run_niri_daemon;
