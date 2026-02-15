use crate::config::{Config, KeyAction, Layer, LayerConfig};
use crate::keycode::KeyCode;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LayerStack {
    layers: Vec<Layer>,
    layer_configs: HashMap<Layer, LayerConfig>,
    base_remaps: HashMap<KeyCode, KeyAction>,
    game_mode_active: bool,
    game_mode_remaps: HashMap<KeyCode, KeyAction>,
}

impl LayerStack {
    pub fn new(config: &Config) -> Self {
        let mut layer_configs = HashMap::new();
        for (layer, layer_config) in &config.layers {
            layer_configs.insert(layer.clone(), layer_config.clone());
        }

        Self {
            layers: vec![Layer::base()],
            layer_configs,
            base_remaps: config.remaps.clone(),
            game_mode_active: false,
            game_mode_remaps: config.game_mode.remaps.clone(),
        }
    }

    #[allow(dead_code)]
    pub fn current_layer(&self) -> Layer {
        self.layers.last().cloned().unwrap_or_else(Layer::base)
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn activate_layer(&mut self, layer: Layer) {
        if !self.layers.contains(&layer) {
            self.layers.push(layer);
        }
    }

    pub fn deactivate_layer(&mut self, layer: &Layer) {
        if !layer.is_base() {
            self.layers.retain(|l| l != layer);
        }
    }

    #[allow(dead_code)]
    pub fn toggle_layer(&mut self, layer: Layer) {
        if self.layers.contains(&layer) {
            self.deactivate_layer(&layer);
        } else {
            self.activate_layer(layer);
        }
    }

    pub const fn set_game_mode(&mut self, active: bool) {
        self.game_mode_active = active;
    }

    pub const fn is_game_mode_active(&self) -> bool {
        self.game_mode_active
    }

    pub const fn base_remaps(&self) -> &HashMap<KeyCode, KeyAction> {
        &self.base_remaps
    }

    pub const fn game_mode_remaps(&self) -> &HashMap<KeyCode, KeyAction> {
        &self.game_mode_remaps
    }

    pub const fn layer_configs(&self) -> &HashMap<Layer, LayerConfig> {
        &self.layer_configs
    }
}
