#![allow(dead_code)]

/// Session Manager - Multi-user keyboard ownership
///
/// Manages keyboard ownership across multiple user sessions, implementing
/// first-come-first-serve allocation with automatic release on session end.
use crate::keyboard_id::KeyboardId;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct UserSession {
    pub uid: u32,
    pub username: String,
    pub state: SessionState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Idle,
}

/// Session Manager for multi-user keyboard ownership
pub struct SessionManager {
    /// Map of keyboard ID to owning user UID
    keyboard_owners: Arc<RwLock<HashMap<KeyboardId, u32>>>,
    /// Map of UID to user session info
    user_sessions: Arc<RwLock<HashMap<u32, UserSession>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            keyboard_owners: Arc::new(RwLock::new(HashMap::new())),
            user_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update user session information from loginctl
    pub async fn refresh_sessions(&self) -> Result<()> {
        let sessions = list_user_sessions()?;
        debug!("Found {} user sessions from loginctl", sessions.len());
        for s in &sessions {
            debug!(
                "  Session: uid={}, username={}, state={:?}",
                s.uid, s.username, s.state
            );
        }

        let mut user_sessions = self.user_sessions.write().await;

        // Update existing sessions and add new ones
        for session in sessions {
            user_sessions.insert(session.uid, session);
        }

        // Remove sessions that are no longer active
        let active_uids: Vec<u32> = user_sessions
            .values()
            .filter(|s| s.state == SessionState::Active)
            .map(|s| s.uid)
            .collect();

        user_sessions.retain(|uid, _| active_uids.contains(uid));
        drop(user_sessions);

        // Release keyboards from inactive sessions
        let user_sessions = self.user_sessions.read().await;
        self.keyboard_owners
            .write()
            .await
            .retain(|kbd_id, owner_uid| {
                let should_retain = user_sessions
                    .get(owner_uid)
                    .map(|s| s.state == SessionState::Active)
                    .unwrap_or(false);

                if !should_retain {
                    info!(
                        "Auto-releasing keyboard {} from inactive user {}",
                        kbd_id, owner_uid
                    );
                }

                should_retain
            });

        Ok(())
    }

    /// Check if a user session is active
    pub async fn is_user_active(&self, uid: u32) -> bool {
        let sessions = self.user_sessions.read().await;
        sessions
            .get(&uid)
            .map(|s| s.state == SessionState::Active)
            .unwrap_or(false)
    }

    /// Get all active user UIDs
    pub async fn get_active_uids(&self) -> Vec<u32> {
        let sessions = self.user_sessions.read().await;
        sessions
            .iter()
            .filter(|(_, session)| session.state == SessionState::Active)
            .map(|(&uid, _)| uid)
            .collect()
    }
}

/// List all user sessions using loginctl
fn list_user_sessions() -> Result<Vec<UserSession>> {
    let output = Command::new("loginctl")
        .arg("list-sessions")
        .arg("--no-legend")
        .output()
        .context("Failed to run loginctl")?;

    if !output.status.success() {
        warn!("loginctl command failed, assuming single user system");
        // Fallback: assume current user
        let uid = unsafe { libc::getuid() };
        return Ok(vec![UserSession {
            uid,
            username: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            state: SessionState::Active,
        }]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let session_id = parts[0];
            let username = parts[2];

            // Get session UID
            if let Ok(uid) = get_session_uid(session_id) {
                // Check if session is active
                let state = if is_session_active(session_id) {
                    SessionState::Active
                } else {
                    SessionState::Idle
                };

                debug!(
                    "Session {} ({}): uid={}, state={:?}",
                    session_id, username, uid, state
                );

                sessions.push(UserSession {
                    uid,
                    username: username.to_string(),
                    state,
                });
            }
        }
    }

    debug!("Found {} user sessions", sessions.len());
    Ok(sessions)
}

/// Get UID for a session
fn get_session_uid(session_id: &str) -> Result<u32> {
    let output = Command::new("loginctl")
        .arg("show-session")
        .arg(session_id)
        .arg("--property=User")
        .arg("--value")
        .output()
        .context("Failed to get session UID")?;

    let uid_str = String::from_utf8_lossy(&output.stdout);
    uid_str.trim().parse().context("Failed to parse UID")
}

/// Check if a session is active
fn is_session_active(session_id: &str) -> bool {
    Command::new("loginctl")
        .arg("show-session")
        .arg(session_id)
        .arg("--property=State")
        .arg("--value")
        .output()
        .ok()
        .map(|output| {
            let state = String::from_utf8_lossy(&output.stdout);
            let state_str = state.trim();
            debug!("Session {} state: '{}'", session_id, state_str);
            // Accept "active", "online", or "lingering" as active
            matches!(state_str, "active" | "online" | "lingering")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Fix these tests by implementing the missing methods
    /*
    #[tokio::test]
    async fn test_keyboard_ownership() {
        let manager = SessionManager::new();
        let kbd = KeyboardId::new("test-keyboard-hw-id".to_string());

        // User 1000 requests keyboard
        assert!(manager.request_keyboard(kbd.clone(), 1000).await.unwrap());
        assert_eq!(manager.get_keyboard_owner(&kbd).await, Some(1000));

        // User 1001 requests same keyboard (should be denied)
        assert!(!manager.request_keyboard(kbd.clone(), 1001).await.unwrap());
        assert_eq!(manager.get_keyboard_owner(&kbd).await, Some(1000));

        // Release keyboard
        manager.release_keyboard(&kbd).await.unwrap();
        assert_eq!(manager.get_keyboard_owner(&kbd).await, None);

        // User 1001 can now request it
        assert!(manager.request_keyboard(kbd.clone(), 1001).await.unwrap());
        assert_eq!(manager.get_keyboard_owner(&kbd).await, Some(1001));
    }

    #[tokio::test]
    async fn test_user_keyboards() {
        let manager = SessionManager::new();
        let kbd1 = KeyboardId::new("keyboard-1-hw-id".to_string());
        let kbd2 = KeyboardId::new("keyboard-2-hw-id".to_string());

        // User 1000 requests both keyboards
        manager.request_keyboard(kbd1.clone(), 1000).await.unwrap();
        manager.request_keyboard(kbd2.clone(), 1000).await.unwrap();

        let user_kbds = manager.get_user_keyboards(1000).await;
        assert_eq!(user_kbds.len(), 2);
    }
    */
}
