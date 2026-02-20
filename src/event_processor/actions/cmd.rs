use crate::config::KeyAction;
use crate::event_processor::actions::{EmitResult, HandleContext, HeldAction};
use crate::keycode::KeyCode;

pub fn emit_cmd(
    action: &KeyAction,
    _keycode: KeyCode,
    _ctx: &mut HandleContext<'_>,
) -> (EmitResult, Option<HeldAction>) {
    match action {
        KeyAction::CMD(command) => {
            let cmd = command.clone();
            std::thread::spawn(move || {
                let _ = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            });
            (EmitResult::None, None)
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
