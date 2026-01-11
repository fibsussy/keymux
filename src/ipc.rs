#![allow(clippy::cast_possible_truncation)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// IPC message from client to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    /// Check if daemon is alive
    Ping,
    /// List all detected keyboards with their status
    ListKeyboards,
    /// Toggle enabled status of keyboards (returns current list for interactive selection)
    ToggleKeyboards,
    /// Enable specific keyboard by hardware ID
    EnableKeyboard(String),
    /// Disable specific keyboard by hardware ID
    DisableKeyboard(String),
    /// Set game mode state (true = on, false = off)
    SetGameMode(bool),
    /// Shutdown daemon
    Shutdown,
}

/// IPC response from daemon to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    /// Daemon is alive
    Pong,
    /// List of keyboards with their info
    KeyboardList(Vec<KeyboardInfo>),
    /// Operation succeeded
    Ok,
    /// Operation failed with error message
    Error(String),
}

/// Information about a detected keyboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardInfo {
    /// Persistent hardware ID (constructed from USB vendor/product or other unique info)
    pub hardware_id: String,
    /// Human-readable name
    pub name: String,
    /// Current device path (can change between boots)
    pub device_path: String,
    /// Whether this keyboard is currently enabled
    pub enabled: bool,
    /// Whether this keyboard is currently connected
    pub connected: bool,
}

/// Get the IPC socket path for root daemon
pub fn get_root_socket_path() -> PathBuf {
    Path::new("/run").join("keyboard-middleware.sock")
}

/// Get the IPC socket path for user daemon (legacy, for compatibility)
pub fn get_user_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    Path::new(&runtime_dir).join("keyboard-middleware.sock")
}

/// Get the IPC socket path (tries root first, falls back to user)
pub fn get_socket_path() -> PathBuf {
    let root_sock = get_root_socket_path();
    if root_sock.exists() {
        root_sock
    } else {
        get_user_socket_path()
    }
}

/// Send an IPC request and receive response
pub fn send_request(request: &IpcRequest) -> Result<IpcResponse> {
    let socket_path = get_socket_path();
    let mut stream =
        UnixStream::connect(&socket_path).context("Failed to connect to daemon. Is it running?")?;

    // Serialize and send request
    let encoded = bincode::serialize(request)?;
    let len = (encoded.len() as u32).to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(&encoded)?;
    stream.flush()?;

    // Read response length
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    // Read response
    let mut response_buf = vec![0u8; len];
    stream.read_exact(&mut response_buf)?;
    let response: IpcResponse = bincode::deserialize(&response_buf)?;

    Ok(response)
}
