use crate::config::KeyAction;
use crate::event_processor::actions::{EmitResult, HandleContext, HeldAction};
use crate::keycode::KeyCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdResolution {
    Run(String),
    None,
}

pub struct CmdProcessor;

impl CmdProcessor {
    pub const fn new() -> Self {
        Self
    }

    pub const fn execute(&self, command: String) -> CmdResolution {
        CmdResolution::Run(command)
    }
}

impl Default for CmdProcessor {
    fn default() -> Self {
        Self::new()
    }
}

pub fn emit_cmd(
    action: &KeyAction,
    _keycode: KeyCode,
    _ctx: &mut HandleContext<'_>,
) -> (EmitResult, Option<HeldAction>) {
    match action {
        KeyAction::CMD(command) => {
            let cmd_processor = CmdProcessor::new();
            let result = cmd_processor.execute(command.clone());
            match result {
                CmdResolution::Run(cmd) => (EmitResult::Command(cmd), None),
                CmdResolution::None => (EmitResult::None, None),
            }
        }
        _ => (EmitResult::None, None),
    }
}

pub fn unemit_cmd(
    _action: &KeyAction,
    _held_action: HeldAction,
    _keycode: KeyCode,
    _ctx: &mut HandleContext<'_>,
) -> EmitResult {
    EmitResult::None
}
