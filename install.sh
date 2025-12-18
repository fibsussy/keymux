#!/bin/bash
set -euo pipefail

# Get sudo password early so we don't interrupt later
sudo -v

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Track what we've done for atomic rollback
INSTALLED_PACKAGE=false
SERVICE_ENABLED=false

# Cleanup function
cleanup() {
    local exit_code=$?

    if [ $exit_code -ne 0 ]; then
        log_error "Installation failed! Rolling back..."

        # Stop service if it was running
        if systemctl --user is-active keyboard-middleware &>/dev/null; then
            log_warn "Stopping service..."
            systemctl --user stop keyboard-middleware 2>/dev/null || true
        fi

        # Disable service if we enabled it
        if [ "$SERVICE_ENABLED" = true ]; then
            log_warn "Disabling service..."
            systemctl --user disable keyboard-middleware 2>/dev/null || true
        fi

        # Remove package if we installed it
        if [ "$INSTALLED_PACKAGE" = true ]; then
            log_warn "Removing installed package..."
            sudo pacman -R --noconfirm keyboard-middleware 2>/dev/null || true
        fi

        log_error "Installation aborted!"
        exit $exit_code
    fi
}

# Set up trap for cleanup
trap cleanup EXIT INT TERM

# Main installation process
main() {
    log_info "Starting keyboard-middleware installation..."
    echo

    # Check if we're in the right directory
    if [ ! -f "Cargo.toml" ]; then
        log_error "Cargo.toml not found! Run this script from the project root."
        exit 1
    fi

    # Check if user is in input group
    if ! groups | grep -q "\binput\b"; then
        log_warn "You are not in the 'input' group!"
        log_warn "Add yourself with: sudo usermod -a -G input $USER"
        log_warn "Then log out and log back in."
        echo
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi

    # Run cargo check first (non-invasive validation)
    log_info "Validating code with cargo check..."
    if ! cargo check --release 2>&1 | tee /tmp/cargo-check-keyboard-middleware.log; then
        log_error "cargo check failed! Check /tmp/cargo-check-keyboard-middleware.log"
        exit 1
    fi
    log_success "Code validation passed"
    echo

    # Build release
    log_info "Building release binary..."
    if ! cargo build --release 2>&1 | tee /tmp/cargo-build-keyboard-middleware.log; then
        log_error "cargo build failed! Check /tmp/cargo-build-keyboard-middleware.log"
        exit 1
    fi
    log_success "Binary built successfully"
    echo

    # Stop existing service before installing binary (prevents "Text file busy" error)
    if systemctl --user is-active keyboard-middleware &>/dev/null; then
        log_info "Stopping existing service before installation..."
        systemctl --user stop keyboard-middleware
        log_success "Service stopped"
        echo
    fi

    # Install binary
    log_info "Installing binary to /usr/bin/keyboard-middleware..."
    if ! sudo cp target/release/keyboard-middleware /usr/bin/keyboard-middleware; then
        log_error "Failed to copy binary!"
        exit 1
    fi
    sudo chmod +x /usr/bin/keyboard-middleware
    INSTALLED_PACKAGE=true
    log_success "Binary installed"
    echo

    # Install systemd service
    log_info "Installing systemd service..."
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"

    if [ -f "keyboard-middleware.service" ]; then
        cp keyboard-middleware.service "$service_dir/"
        log_success "Service file installed to $service_dir"
    else
        log_warn "keyboard-middleware.service not found, skipping service installation"
    fi
    echo

    # Reload systemd user daemon
    log_info "Reloading systemd user daemon..."
    systemctl --user daemon-reload
    log_success "Systemd daemon reloaded"
    echo

    # Enable service (doesn't affect running service)
    log_info "Enabling keyboard-middleware service..."
    if ! systemctl --user enable keyboard-middleware; then
        log_error "Failed to enable service!"
        exit 1
    fi
    SERVICE_ENABLED=true
    log_success "Service enabled"
    echo

    # Start service
    log_info "Starting keyboard-middleware service..."
    if ! systemctl --user start keyboard-middleware; then
        log_error "Failed to start service!"
        log_error "Check logs with: journalctl --user -u keyboard-middleware -f"
        exit 1
    fi
    log_success "Service started"
    echo

    # Wait a moment for service to start
    sleep 1

    # Check service status
    if systemctl --user is-active keyboard-middleware &>/dev/null; then
        log_success "Service is running!"
        echo

        # Show service status
        log_info "Service status:"
        systemctl --user status keyboard-middleware --no-pager -n 5 || true
        echo

        log_success "Installation complete!"
        echo
        log_info "Next steps:"
        echo "  - View logs: journalctl --user -u keyboard-middleware -f"
        echo "  - Stop service: systemctl --user stop keyboard-middleware"
        echo "  - Disable service: systemctl --user disable keyboard-middleware"
        echo
        log_info "Features:"
        echo "  - Game mode: niri gamescope detection + WASD fallback"
        echo "  - Nav layer: Hold Left Alt for arrow keys (HJKL) and modifiers (ASDF)"
        echo "  - Mouse: Left Alt + arrow keys for mouse buttons"
        echo
    else
        log_error "Service failed to start!"
        log_error "Check logs with: journalctl --user -u keyboard-middleware -xe"
        exit 1
    fi
}

# Run main
main

# If we got here, everything succeeded
exit 0
