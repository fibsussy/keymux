use super::adaptive::AdaptiveProcessor;
use crate::config::{Config, KeyAction};
use crate::event_processor::actions::{
    handle_action_release, EmitResult, HandleContext, HeldAction, ProcessResult, TdResolution,
};
use crate::event_processor::layer_stack::LayerStack;
use crate::keycode::KeyCode;
use std::collections::HashMap;

pub struct KeymapProcessor {
    held_keys: HashMap<KeyCode, Vec<HeldAction>>,
    layer_stack: LayerStack,
    mt_processor: crate::event_processor::actions::MtProcessor,
    dt_processor: crate::event_processor::actions::DtProcessor,
    osm_processor: crate::event_processor::actions::OsmProcessor,
    socd_processor: crate::event_processor::actions::SocdProcessor,
    adaptive_processor: AdaptiveProcessor,
}

impl KeymapProcessor {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            held_keys: HashMap::new(),
            layer_stack: LayerStack::new(config),
            mt_processor: crate::event_processor::actions::MtProcessor::new(config),
            dt_processor: crate::event_processor::actions::DtProcessor::new(config),
            osm_processor: crate::event_processor::actions::OsmProcessor::new(config),
            socd_processor: crate::event_processor::actions::SocdProcessor::from_config(config),
            adaptive_processor: AdaptiveProcessor::new(),
        }
    }

    pub fn set_game_mode(&mut self, active: bool) {
        self.layer_stack.set_game_mode(active);
        self.mt_processor.set_game_mode(active);
    }

    pub fn check_dt_timeouts(&mut self) -> ProcessResult {
        let events = self.dt_processor.handle_check_timeouts();
        if events.is_empty() {
            ProcessResult::None
        } else {
            ProcessResult::MultipleEvents(events)
        }
    }

    pub fn get_held_keys(&self) -> Vec<KeyCode> {
        self.held_keys.keys().copied().collect()
    }

    pub fn save_adaptive_stats(&self, user_id: u32) -> Result<(), std::io::Error> {
        self.adaptive_processor.save_adaptive_stats(user_id)
    }

    pub fn load_adaptive_stats(&mut self, user_id: u32) -> Result<(), std::io::Error> {
        self.adaptive_processor.load_adaptive_stats(user_id)
    }

    #[allow(dead_code)]
    pub fn get_all_key_stats(
        &self,
    ) -> HashMap<KeyCode, crate::event_processor::actions::RollingStats> {
        self.adaptive_processor.get_all_key_stats()
    }

    pub fn process_key(&mut self, keycode: KeyCode, pressed: bool) -> ProcessResult {
        if pressed {
            self.process_key_press(keycode)
        } else {
            self.process_key_release(keycode)
        }
    }

    fn process_key_press(&mut self, keycode: KeyCode) -> ProcessResult {
        self.adaptive_processor.record_key_press(keycode);

        let dt_timeout_events = self.dt_processor.handle_check_timeouts();

        // Notify DT of other key press for permissive hold
        let dt_permissive_events = self.dt_processor.on_other_key_press(keycode);

        let action = self.lookup_action(keycode).cloned();

        let (result, key_action) = match action {
            Some(KeyAction::DT(tap_action, double_tap_action)) => {
                self.handle_dt_press(keycode, &tap_action, &double_tap_action)
            }
            Some(action) => {
                let mut ctx = self.make_context();
                action.emit(keycode, &mut ctx)
            }
            None => {
                let mut ctx = self.make_context();
                KeyAction::Key(keycode).emit(keycode, &mut ctx)
            }
        };

        if let Some(ka) = key_action {
            self.held_keys.insert(keycode, vec![ka]);
        }

        // Combine timeout events and permissive hold events
        let mut all_dt_events = dt_timeout_events;
        all_dt_events.extend(dt_permissive_events);

        self.combine_with_timeouts(all_dt_events, result.to_process_result())
    }

    fn handle_dt_press(
        &mut self,
        keycode: KeyCode,
        tap_action: &KeyAction,
        double_tap_action: &KeyAction,
    ) -> (EmitResult, Option<HeldAction>) {
        let resolution =
            self.dt_processor
                .resolve_action(keycode, tap_action, double_tap_action, false);

        match resolution {
            TdResolution::EmitAction(action) => {
                let mut ctx = self.make_context();
                let (emit_result, held) = action.emit(keycode, &mut ctx);
                (emit_result, held)
            }
            TdResolution::HoldFirst => {
                let mut ctx = self.make_context();
                let (emit_result, held) = tap_action.emit(keycode, &mut ctx);
                (emit_result, held)
            }
            _ => (
                EmitResult::None,
                Some(HeldAction::DtManaged {
                    tap_action: (*tap_action).clone(),
                    double_tap_action: (*double_tap_action).clone(),
                }),
            ),
        }
    }

    fn handle_dt_release(
        &mut self,
        keycode: KeyCode,
        _tap_action: &KeyAction,
        _double_tap_action: &KeyAction,
    ) -> ProcessResult {
        if let Some(last_emitted) = self.dt_processor.get_last_emitted_action(keycode) {
            let mut ctx = self.make_context();
            let emit_result =
                last_emitted.unemit(HeldAction::RegularKey(keycode), keycode, &mut ctx);
            return emit_result.to_process_result();
        }

        ProcessResult::None
    }

    fn process_key_release(&mut self, keycode: KeyCode) -> ProcessResult {
        self.adaptive_processor
            .record_key_release(keycode, self.layer_stack.is_game_mode_active());

        let dt_timeout_events = self.dt_processor.handle_check_timeouts();

        if let Some(actions) = self.held_keys.remove(&keycode) {
            let mut events = Vec::new();

            for action in actions {
                let ctx = self.make_context();
                let result = handle_action_release(action, keycode, ctx);

                match result {
                    ProcessResult::EmitKey(key, pressed) => events.push((key, pressed)),
                    ProcessResult::MultipleEvents(mut evts) => events.append(&mut evts),
                    ProcessResult::TapKeyPressRelease(key) => {
                        events.push((key, true));
                        events.push((key, false));
                        return self.combine_with_timeouts(
                            dt_timeout_events,
                            ProcessResult::MultipleEvents(events),
                        );
                    }
                    ProcessResult::None => {}
                    _ => {}
                }
            }

            if events.is_empty() {
                ProcessResult::None
            } else if events.len() == 1 {
                ProcessResult::EmitKey(events[0].0, events[0].1)
            } else {
                ProcessResult::MultipleEvents(events)
            }
        } else {
            ProcessResult::None
        }
    }

    const fn make_context(&mut self) -> HandleContext<'_> {
        HandleContext {
            mt_processor: &mut self.mt_processor,
            dt_processor: &mut self.dt_processor,
            osm_processor: &mut self.osm_processor,
            socd_processor: &mut self.socd_processor,
            layer_stack: &mut self.layer_stack,
        }
    }

    fn lookup_action(&self, keycode: KeyCode) -> Option<&KeyAction> {
        if self.layer_stack.is_game_mode_active() {
            if let Some(action) = self.layer_stack.game_mode_remaps().get(&keycode) {
                return Some(action);
            }
        }

        for layer in self.layer_stack.layers().iter().rev() {
            if let Some(config) = self.layer_stack.layer_configs().get(layer) {
                if let Some(action) = config.remaps.get(&keycode) {
                    if action.is_transparent() {
                        continue;
                    }
                    return Some(action);
                }
            }
        }

        self.layer_stack.base_remaps().get(&keycode)
    }

    fn combine_with_timeouts(
        &self,
        timeout_events: Vec<(KeyCode, bool)>,
        result: ProcessResult,
    ) -> ProcessResult {
        if timeout_events.is_empty() {
            return result;
        }

        match result {
            ProcessResult::None => ProcessResult::MultipleEvents(timeout_events),
            ProcessResult::EmitKey(key, pressed) => {
                let mut all_events = timeout_events;
                all_events.push((key, pressed));
                ProcessResult::MultipleEvents(all_events)
            }
            ProcessResult::TapKeyPressRelease(key) => {
                let mut all_events = timeout_events;
                all_events.push((key, true));
                all_events.push((key, false));
                ProcessResult::MultipleEvents(all_events)
            }
            ProcessResult::MultipleEvents(mut events) => {
                let mut all_events = timeout_events;
                all_events.append(&mut events);
                ProcessResult::MultipleEvents(all_events)
            }
            other => other,
        }
    }
}
