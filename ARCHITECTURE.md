# Keyboard Middleware - Clean Architecture

## Overview

This document describes the clean architecture for the keyboard middleware system, designed to provide high-performance key processing with robust management capabilities.

## Design Goals

1. **Zero-latency hot path**: Key event processing must be synchronous and allocation-free
2. **Async management**: Device discovery, configuration, and user management should be async
3. **Multi-user support**: Proper session-based keyboard ownership
4. **Self-healing**: Automatic recovery from failures and clean error handling
5. **Hot-reload**: Configuration changes without service interruption

## Architecture Components

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Root Daemon   │◄──►│  Session Manager │◄──►│  User Sessions  │
│  (Async Event)  │    │   (Ownership)    │    │   (Per User)    │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ Device Pool     │    │  Config Manager  │    │   IPC Server   │
│ (Hotplug Mgr)   │    │  (Hot Reload)    │    │  (Multi-user)  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ Thread Pool     │    │  Game Mode Mgr   │    │  Niri Monitor   │
│ (Per Device)    │    │ (Smart Detection)│    │ (Auto-restart) │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## Key Design Decisions

### 1. Hybrid Sync/Async Architecture

**Hot Path (Synchronous)** - MAXIMUM SPEED
```
Physical Device → Event Processor (sync) → Virtual Device
         ↓                ↓                    ↓
   evdev::fetch()  Keymap Processing    evdev::emit()
   (non-blocking)   (zero alloc)       (instant)
```

**Cold Path (Asynchronous)** - ROBUST MANAGEMENT
```
Root Daemon (async) ←→ IPC Server ←→ Config Manager
      ↓                    ↓              ↓
Hotplug Events      User Commands    File Watcher
Session Changes     Status Queries   Hot Reload
```

### 2. Session-Based Keyboard Ownership

- **First-come-first-serve**: Keyboards assigned to first requesting active user
- **Automatic release**: Keyboards freed when user session ends
- **Priority system**: Admin/system users can override regular users
- **Multi-session support**: Multiple users can have different keyboards

### 3. Smart Hot-Reload

**Before (Destructive)**:
```
Config Change → Kill ALL Threads → Reload Config → Restart ALL Threads
```

**After (Smart)**:
```
Config Change → Analyze Changes → Restart ONLY Affected Threads
```

### 4. Thread Lifecycle Management

- **Graceful shutdown**: Threads receive signals and clean up properly
- **Device release**: Proper ungrab before thread exit
- **Zombie cleanup**: Automatic detection and cleanup of dead threads
- **Resource tracking**: Monitor thread health and device state

## Component Details

### Session Manager (`session_manager.rs`)

**Purpose**: Manage multi-user keyboard ownership

**Key Features**:
- Monitor active user sessions via `loginctl`
- Track keyboard ownership per user
- Handle session changes (login/logout)
- First-come-first-serve assignment

**API**:
```rust
pub async fn request_keyboard(&self, keyboard_id: KeyboardId, uid: u32) -> Result<bool>
pub async fn release_keyboard(&self, keyboard_id: &KeyboardId) -> Result<()>
pub async fn get_keyboard_owner(&self, keyboard_id: &KeyboardId) -> Option<u32>
pub async fn get_user_keyboards(&self, uid: u32) -> Vec<KeyboardId>
```

### Async Daemon (`async_daemon.rs`)

**Purpose**: Main orchestrator with async management layer

**Key Features**:
- `tokio::select!` for efficient event handling
- Background sync services (hotplug, IPC, niri, config watcher)
- Smart hot-reload without service interruption
- Thread pool management with proper cleanup

**Event Loop**:
```rust
tokio::select! {
    _ = tokio::time::sleep(Duration::from_millis(10)) => {
        // Check hotplug events
    }
    _ = tokio::time::sleep(Duration::from_millis(10)) => {
        // Check IPC commands  
    }
    _ = tokio::time::sleep(Duration::from_millis(10)) => {
        // Check niri events
    }
    _ = tokio::time::sleep(Duration::from_millis(100)) => {
        // Check config changes
    }
    _ = tokio::time::sleep(Duration::from_secs(30)) => {
        // Periodic cleanup
    }
}
```

### Event Processor (`event_processor.rs`)

**Purpose**: High-performance synchronous key processing

**Hot Path Optimizations**:
- **Synchronous only**: No async overhead in key processing
- **Zero allocation**: Reuse buffers and minimize allocations
- **Direct evdev**: Direct device access without abstractions
- **Non-blocking**: Check for commands without blocking

**Command Interface**:
```rust
enum ProcessorCommand {
    ReloadConfig(Config),
    SetGameMode(bool),
    Shutdown,
}
```

### Config Manager

**Purpose**: Smart configuration handling

**Features**:
- **Atomic reload**: Update specific sections without full restart
- **Validation**: Verify config before applying
- **Rollback**: Revert on invalid configurations
- **User-specific**: Per-keyboard overrides

## Performance Characteristics

### Target Metrics

- **Key latency**: < 1ms (hot path priority)
- **Hot-reload**: < 100ms without interruption
- **Device discovery**: < 50ms on hotplug
- **Session tracking**: < 10ms for ownership changes

### Memory Usage

- **Per thread**: Minimal stack + keymap state
- **Shared structures**: Arc<RwLock> for thread-safe access
- **Zero-copy**: Event passing without allocations where possible

## Error Handling Strategy

### Self-Healing Components

1. **Device hotplug**: Automatic rediscovery on connection loss
2. **Thread restart**: Dead threads automatically respawned
3. **Config recovery**: Invalid configs rejected with rollback
4. **Session recovery**: Reclaim keyboards on session restoration

### Graceful Degradation

- **Partial failures**: Single keyboard issues don't affect others
- **Fallback modes**: Continue with reduced functionality
- **User notification**: Clear error reporting and recovery steps

## Security Considerations

### User Isolation

- **Device permissions**: Only owner can control keyboard
- **IPC validation**: Verify user permissions for commands
- **Session boundaries**: Respect user session limits

### Input Validation

- **Config sanitization**: Prevent malicious configurations
- **Event filtering**: Drop malformed input events
- **Resource limits**: Prevent resource exhaustion

## Deployment

### systemd Integration

**Root Daemon Service**:
```ini
[Unit]
Description=Keyboard Middleware Root Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/keyboard-middleware daemon
Restart=always
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target

# User daemon service
sudo cp target/release/keyboard-middleware /usr/local/bin/
sudo systemctl --user daemon-reload
sudo systemctl --user enable --now keyboard-middleware.service

# Root daemon (optional, for system-wide keyboards)
sudo systemctl daemon-reload
sudo systemctl enable --now keyboard-middleware.service

# Setup complete

**User Daemon Service**:
```ini
[Unit]
Description=Keyboard Middleware User Daemon
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/keyboard-middleware daemon --user %i
Restart=on-failure
User=%i

[Install]
WantedBy=default.target
```

## Migration Path

### Phase 1: Core Architecture
- Implement async daemon skeleton
- Add session manager
- Migrate existing event processor

### Phase 2: Smart Features  
- Smart hot-reload implementation
- Enhanced error recovery
- Performance optimizations

### Phase 3: Advanced Features
- Multi-user session support
- Advanced game mode detection
- Configuration validation

## Future Enhancements

### Performance
- **NUMA awareness**: Bind threads to CPU cores
- **Real-time scheduling**: High priority for key processing
- **Memory mapping**: Zero-copy event handling

### Features
- **Device groups**: Logical keyboard groupings
- **Dynamic layers**: Runtime layer creation
- **Macro recording**: User-defined key sequences

### Integration
- **Wayland native**: Direct compositor integration
- **PulseAudio sync**: Audio-visual feedback coordination
- **Network support**: Remote keyboard management

---

This architecture provides a solid foundation for high-performance, multi-user keyboard middleware with robust error handling and future extensibility.
