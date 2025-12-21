#![allow(clippy::cast_possible_truncation)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
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

/// Get the IPC socket path
pub fn get_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    Path::new(&runtime_dir).join("keyboard-middleware.sock")
}

/// Send an IPC request and receive response
pub fn send_request(request: &IpcRequest) -> Result<IpcResponse> {
    let socket_path = get_socket_path();
    let mut stream = UnixStream::connect(&socket_path)
        .context("Failed to connect to daemon. Is it running?")?;

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

/// IPC server for daemon
pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    pub fn new() -> Result<Self> {
        let socket_path = get_socket_path();

        // Remove existing socket if present
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)
            .context("Failed to bind IPC socket")?;

        // Set socket permissions (owner only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&socket_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&socket_path, perms)?;
        }

        listener.set_nonblocking(true)?;

        Ok(Self { listener })
    }

    /// Try to accept a connection (non-blocking)
    pub fn try_accept(&self) -> Result<Option<(IpcRequest, UnixStream)>> {
        match self.listener.accept() {
            Ok((mut stream, _)) => {
                // Read request length
                let mut len_buf = [0u8; 4];
                stream.read_exact(&mut len_buf)?;
                let len = u32::from_le_bytes(len_buf) as usize;

                // Read request
                let mut request_buf = vec![0u8; len];
                stream.read_exact(&mut request_buf)?;
                let request: IpcRequest = bincode::deserialize(&request_buf)?;

                Ok(Some((request, stream)))
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Send a response to a client
    pub fn send_response(mut stream: UnixStream, response: &IpcResponse) -> Result<()> {
        let encoded = bincode::serialize(response)?;
        let len = (encoded.len() as u32).to_le_bytes();
        stream.write_all(&len)?;
        stream.write_all(&encoded)?;
        stream.flush()?;
        Ok(())
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let socket_path = get_socket_path();
        let _ = std::fs::remove_file(socket_path);
    }
}
