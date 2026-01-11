/// Config Manager - Smart hot-reload capabilities
///
/// Handles configuration loading for multi-user daemon.
use crate::config::Config;
use anyhow::{Context, Result};

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration manager with hot-reload support
#[derive(Clone)]
pub struct ConfigManager {
    /// Current active configuration
    config: Arc<RwLock<Config>>,
}

impl ConfigManager {
    /// Create a new config manager
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let config = Config::load(&config_path)
            .with_context(|| format!("Failed to load config from {:?}", config_path))?;

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
        })
    }

    /// Get the current configuration
    pub async fn get_config(&self) -> Config {
        self.config.read().await.clone()
    }
}
