use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub app_id: Option<String>,
    pub pid: Option<u32>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub enum WindowManagerEvent {
    WindowFocusChanged(WindowInfo),
}

pub trait WindowManager: Send + Sync + Clone + 'static {
    fn name(&self) -> &'static str;

    fn is_available(&self) -> bool;

    fn detect_socket(&self) -> Option<PathBuf>;

    fn get_focused_window(&self) -> WindowInfo;

    fn event_stream_args(&self) -> Vec<&'static str>;

    fn parse_event(&self, line: &str) -> Option<WindowInfo>;

    fn start_event_monitor(&self, tx: UnboundedSender<WindowManagerEvent>)
    where
        Self: Sized,
    {
        let wm: &'static Self = Box::leak(Box::new(self.clone()));
        spawn_event_monitor(wm, tx);
    }

    fn start_event_monitor_sync(&self, tx: Sender<WindowManagerEvent>)
    where
        Self: Sized,
    {
        let wm: &'static Self = Box::leak(Box::new(self.clone()));
        spawn_event_monitor_sync(wm, tx);
    }

    fn should_enable_gamemode(&self, window_info: &WindowInfo) -> bool {
        default_should_enable_gamemode(window_info)
    }
}

pub fn spawn_event_monitor<T: WindowManager + 'static>(
    wm: &'static T,
    tx: UnboundedSender<WindowManagerEvent>,
) {
    let _socket_path = if let Some(path) = wm.detect_socket() {
        path
    } else {
        error!("{}: No socket found, cannot start monitor", wm.name());
        return;
    };

    thread::spawn(move || loop {
        info!("{}: Starting event stream monitor...", wm.name());

        let mut child = match std::process::Command::new(wm.name())
            .args(wm.event_stream_args())
            .stdout(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                error!("{}: Failed to spawn: {}", wm.name(), e);
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        let Some(stdout) = child.stdout.take() else {
            error!("{}: Failed to capture stdout", wm.name());
            thread::sleep(Duration::from_secs(5));
            continue;
        };

        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if let Some(window_info) = wm.parse_event(&line) {
                        if let Some(ref app) = window_info.app_id {
                            debug!(
                                "{}: Focus changed → app_id: {}, pid: {:?}",
                                wm.name(),
                                app,
                                window_info.pid
                            );
                        }
                        if tx
                            .send(WindowManagerEvent::WindowFocusChanged(window_info))
                            .is_err()
                        {
                            error!("{}: Channel closed, exiting", wm.name());
                            return;
                        }
                    }
                }
                Err(e) => {
                    error!("{}: Error reading event: {}", wm.name(), e);
                    break;
                }
            }
        }

        error!(
            "{}: Event stream ended, restarting in 5 seconds...",
            wm.name()
        );
        thread::sleep(Duration::from_secs(5));
    });
}

pub fn spawn_event_monitor_sync<T: WindowManager + 'static>(
    wm: &'static T,
    tx: Sender<WindowManagerEvent>,
) {
    let _socket_path = if let Some(path) = wm.detect_socket() {
        path
    } else {
        error!("{}: No socket found, cannot start monitor", wm.name());
        return;
    };

    thread::spawn(move || loop {
        info!("{}: Starting event stream monitor...", wm.name());

        let mut child = match std::process::Command::new(wm.name())
            .args(wm.event_stream_args())
            .stdout(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                error!("{}: Failed to spawn: {}", wm.name(), e);
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        let Some(stdout) = child.stdout.take() else {
            error!("{}: Failed to capture stdout", wm.name());
            thread::sleep(Duration::from_secs(5));
            continue;
        };

        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if let Some(window_info) = wm.parse_event(&line) {
                        if let Some(ref app) = window_info.app_id {
                            debug!(
                                "{}: Focus changed → app_id: {}, pid: {:?}",
                                wm.name(),
                                app,
                                window_info.pid
                            );
                        }
                        if tx
                            .send(WindowManagerEvent::WindowFocusChanged(window_info))
                            .is_err()
                        {
                            error!("{}: Channel closed, exiting", wm.name());
                            return;
                        }
                    }
                }
                Err(e) => {
                    warn!("{}: Error reading event: {}", wm.name(), e);
                    break;
                }
            }
        }

        let _ = child.wait();

        error!(
            "{}: Event stream ended, restarting in 5 seconds...",
            wm.name()
        );
        thread::sleep(Duration::from_secs(5));
    });
}

use std::fs;

fn check_is_game_env(pid: u32) -> bool {
    let env_path = format!("/proc/{pid}/environ");
    if let Ok(contents) = fs::read(&env_path) {
        let env_str = String::from_utf8_lossy(&contents);
        for var in env_str.split('\0') {
            if var == "IS_GAME=1" {
                return true;
            }
        }
    }
    false
}

fn check_process_tree(process_id: u32) -> (bool, bool) {
    let mut has_gamescope = false;
    let mut has_gamemode = false;
    let mut current_pid = process_id;

    for _ in 0..10 {
        let cmdline_path = format!("/proc/{current_pid}/cmdline");
        if let Ok(contents) = fs::read(&cmdline_path) {
            let cmdline = String::from_utf8_lossy(&contents);
            let cmd_lower = cmdline.to_lowercase();

            if cmd_lower.contains("gamescope") || cmd_lower.contains("custom-gamescope") {
                has_gamescope = true;
            }
            if cmd_lower.contains("gamemode") || cmd_lower.contains("gamemoded") {
                has_gamemode = true;
            }
        }

        let stat_path = format!("/proc/{current_pid}/stat");
        let parent_pid = fs::read_to_string(&stat_path).ok().and_then(|stat| {
            let parts: Vec<&str> = stat.rsplitn(2, ')').collect();
            if parts.len() == 2 {
                parts[0].split_whitespace().nth(1)?.parse::<u32>().ok()
            } else {
                None
            }
        });

        match parent_pid {
            Some(parent) if parent > 1 => current_pid = parent,
            _ => break,
        }
    }

    (has_gamescope, has_gamemode)
}

pub fn default_should_enable_gamemode(window_info: &WindowInfo) -> bool {
    if window_info.app_id.as_deref() == Some("gamescope") {
        return true;
    }

    if let Some(app_id) = &window_info.app_id {
        if app_id.starts_with("steam_app_") {
            return true;
        }
    }

    const GAME_APP_IDS: &[&str] = &["org.vinegarhq.Sober"];

    if let Some(app_id) = &window_info.app_id {
        if GAME_APP_IDS.contains(&app_id.as_str()) {
            return true;
        }
    }

    if let Some(pid) = window_info.pid {
        if check_is_game_env(pid) {
            return true;
        }

        let (has_gamescope, has_gamemode) = check_process_tree(pid);
        if has_gamescope || has_gamemode {
            return true;
        }
    }

    false
}
