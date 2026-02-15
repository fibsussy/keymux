use crate::config::{KeyAction, Layer};
use crate::event_processor::actions::{
    handle_dt_action, handle_dt_release, handle_mt_action, handle_osm_action, handle_osm_release,
    handle_socd_action, DtProcessor, MtProcessor, OsmProcessor, SocdProcessor, SocdResolution,
};
use crate::event_processor::layer_stack::LayerStack;
use crate::keycode::KeyCode;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProcessResult {
    EmitKey(KeyCode, bool),
    TapKeyPressRelease(KeyCode),
    MultipleEvents(Vec<(KeyCode, bool)>),
    TypeString(String, bool),
    RunCommand(String),
    None,
}

impl From<SocdResolution> for ProcessResult {
    fn from(res: SocdResolution) -> Self {
        match res {
            SocdResolution::EmitKey(key, pressed) => Self::EmitKey(key, pressed),
            SocdResolution::MultipleEvents(events) => Self::MultipleEvents(events),
            SocdResolution::None => Self::None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum HeldAction {
    RegularKey(KeyCode),
    Layer(Layer),
    MtManaged,
    SocdManaged,
    DtManaged,
    OsmManaged,
}

pub struct HandleContext<'a> {
    pub mt_processor: &'a mut MtProcessor,
    pub dt_processor: &'a mut DtProcessor,
    pub osm_processor: &'a mut OsmProcessor,
    pub socd_processor: &'a mut SocdProcessor,
    pub layer_stack: &'a mut LayerStack,
}

impl KeyAction {
    pub fn handle(
        &self,
        keycode: KeyCode,
        ctx: HandleContext<'_>,
    ) -> (ProcessResult, Option<HeldAction>) {
        match self {
            Self::Key(output_key) => {
                let events = ctx
                    .mt_processor
                    .on_other_key_press_for_resolutions(*output_key);
                if !events.is_empty() {
                    let mut all_events = ctx.mt_processor.resolutions_to_events(&events);
                    all_events.push((*output_key, true));
                    (
                        ProcessResult::MultipleEvents(all_events),
                        Some(HeldAction::RegularKey(*output_key)),
                    )
                } else {
                    (
                        ProcessResult::EmitKey(*output_key, true),
                        Some(HeldAction::RegularKey(*output_key)),
                    )
                }
            }
            Self::MT(tap_action, hold_action) => {
                let (events, _) =
                    handle_mt_action(ctx.mt_processor, keycode, tap_action, hold_action);
                if events.is_empty() {
                    (ProcessResult::None, Some(HeldAction::MtManaged))
                } else if events.len() == 1 {
                    (
                        ProcessResult::EmitKey(events[0].0, events[0].1),
                        Some(HeldAction::MtManaged),
                    )
                } else {
                    (
                        ProcessResult::MultipleEvents(events),
                        Some(HeldAction::MtManaged),
                    )
                }
            }
            Self::TO(layer) => {
                ctx.layer_stack.activate_layer(layer.clone());
                (ProcessResult::None, Some(HeldAction::Layer(layer.clone())))
            }
            Self::TG(layer) => {
                if ctx.layer_stack.layers().contains(layer) {
                    ctx.layer_stack.deactivate_layer(layer);
                } else {
                    ctx.layer_stack.activate_layer(layer.clone());
                }
                (ProcessResult::None, Some(HeldAction::Layer(layer.clone())))
            }
            Self::MO(layer) => {
                ctx.layer_stack.activate_layer(layer.clone());
                (ProcessResult::None, Some(HeldAction::Layer(layer.clone())))
            }
            Self::SOCD(this_action, _) => {
                let result = handle_socd_action(ctx.socd_processor, keycode, this_action);
                (result.into(), Some(HeldAction::SocdManaged))
            }
            Self::CMD(command) => (ProcessResult::RunCommand(command.clone()), None),
            Self::OSM(modifier_action) => {
                let _ = handle_osm_action(ctx.osm_processor, keycode, modifier_action);
                (ProcessResult::None, Some(HeldAction::OsmManaged))
            }
            Self::DT(tap_action, double_tap_action) => {
                let events =
                    handle_dt_action(ctx.dt_processor, keycode, tap_action, double_tap_action);
                if events.is_empty() {
                    (ProcessResult::None, Some(HeldAction::DtManaged))
                } else if events.len() == 1 {
                    if events[0].1 {
                        (
                            ProcessResult::EmitKey(events[0].0, true),
                            Some(HeldAction::DtManaged),
                        )
                    } else {
                        (
                            ProcessResult::EmitKey(events[0].0, false),
                            Some(HeldAction::DtManaged),
                        )
                    }
                } else {
                    (
                        ProcessResult::MultipleEvents(events),
                        Some(HeldAction::DtManaged),
                    )
                }
            }
            Self::Transparent => {
                let resolutions = ctx.mt_processor.on_other_key_press_for_resolutions(keycode);
                if !resolutions.is_empty() {
                    let mut events = ctx.mt_processor.resolutions_to_events(&resolutions);
                    events.push((keycode, true));
                    (
                        ProcessResult::MultipleEvents(events),
                        Some(HeldAction::RegularKey(keycode)),
                    )
                } else {
                    (
                        ProcessResult::EmitKey(keycode, true),
                        Some(HeldAction::RegularKey(keycode)),
                    )
                }
            }
        }
    }
}

pub fn handle_action_release(
    action: HeldAction,
    keycode: KeyCode,
    mt_processor: &mut MtProcessor,
    dt_processor: &mut DtProcessor,
    osm_processor: &mut OsmProcessor,
    socd_processor: &mut SocdProcessor,
    layer_stack: &mut LayerStack,
) -> ProcessResult {
    match action {
        HeldAction::RegularKey(key) => ProcessResult::EmitKey(key, false),
        HeldAction::Layer(layer) => {
            layer_stack.deactivate_layer(&layer);
            ProcessResult::None
        }
        HeldAction::MtManaged => mt_processor
            .handle_release(keycode)
            .map_or(ProcessResult::None, |resolution| {
                apply_mt_resolution(keycode, resolution, mt_processor)
            }),
        HeldAction::SocdManaged => {
            let result: ProcessResult = socd_processor.handle_release(keycode).into();
            result
        }
        HeldAction::DtManaged => handle_dt_release(dt_processor, keycode),
        HeldAction::OsmManaged => handle_osm_release(osm_processor, keycode),
    }
}

fn apply_mt_resolution(
    _source_key: KeyCode,
    resolution: crate::event_processor::actions::MtResolution,
    _mt_processor: &mut MtProcessor,
) -> ProcessResult {
    use crate::event_processor::actions::MtAction;
    match resolution.action {
        MtAction::TapPress(key) => ProcessResult::EmitKey(key, true),
        MtAction::TapPressRelease(key) => ProcessResult::TapKeyPressRelease(key),
        MtAction::HoldPress(key) => ProcessResult::EmitKey(key, true),
        MtAction::HoldPressRelease(key) => {
            ProcessResult::MultipleEvents(vec![(key, true), (key, false)])
        }
        MtAction::ReleaseHold(key) => ProcessResult::EmitKey(key, false),
    }
}
