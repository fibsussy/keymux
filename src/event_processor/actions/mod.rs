//! Action processors for keyboard events
//!
//! This module contains all the specialized processors for different action types:
//! - MT (Mod-Tap): Tap/hold dual-function keys
//! - DT (Double-Tap): Tap dance with single/double-tap detection
//! - OSM (OneShot Modifier): One-shot modifiers that auto-release
//! - SOCD (Simultaneous Opposite Cardinal Direction): Handling for opposing keys
//! - Handlers: Main action handling logic that coordinates all processors

pub mod dt;
pub mod handlers;
pub mod mt;
pub mod osm;
pub mod socd;

// Re-export commonly used types
pub use dt::DtProcessor;
pub use handlers::{handle_action_release, HandleContext, HeldAction, ProcessResult};
pub use mt::{MtAction, MtProcessor, MtResolution, RollingStats};
pub use osm::OsmProcessor;
pub use socd::{SocdProcessor, SocdResolution};

// Re-export handler functions
pub use dt::{handle_dt_action, handle_dt_release};
pub use mt::handle_mt_action;
pub use osm::{handle_osm_action, handle_osm_release};
pub use socd::handle_socd_action;
