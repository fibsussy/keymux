#!/bin/bash

set -euo pipefail

# Show help
show_help() {
    cat <<EOF
Keyboard Middleware Installer

Usage: $0 [OPTION]

Options:
  (none)    Build and install from source (default)
  bin       Install precompiled binary
  --help    Show this help message

Examples:
  # Build from source (requires rust/cargo)
  $0

  # Install precompiled binary (faster, no build dependencies)
  $0 bin

  # Remote install (build from source)
  curl -fsSL https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/install.sh | bash

  # Remote install (precompiled binary)
  curl -fsSL https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/install.sh | bash -s bin
EOF
    exit 0
}

# Parse arguments
MODE="local"
if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    show_help
elif [ "${1:-}" = "bin" ]; then
    MODE="bin"
fi

# Verify we're on Arch Linux
if [ ! -f /etc/arch-release ]; then
    echo "This script only supports Arch Linux."
    echo "For other distros, download the precompiled binary:"
    echo "  https://github.com/fibsussy/keyboard-middleware/releases/latest"
    exit 1
fi

# Detect if we're running from within the repo
START_DIR=$(pwd)
if [ -f "$START_DIR/PKGBUILD" ] && [ -f "$START_DIR/Cargo.toml" ]; then
    # Running from repo - use local files
    echo "Detected local repository, using local files..."

    if [ "$MODE" = "bin" ]; then
        echo "Installing precompiled binary (using local PKGBUILD.bin)..."
        makepkg -si -p PKGBUILD.bin
        echo "keyboard-middleware installed successfully via pacman (precompiled binary)"
    else
        echo "Building from source (using local PKGBUILD)..."
        makepkg -si
        echo "keyboard-middleware installed successfully via pacman (built from source)"
    fi
else
    # Remote install - download from GitHub
    TMP_DIR=$(mktemp -d -t keyboard-middleware-install.XXXXXX)
    trap 'cd "$START_DIR" && rm -rf "$TMP_DIR"' EXIT INT TERM

    if [ "$MODE" = "bin" ]; then
        echo "Installing precompiled binary..."
        # Download PKGBUILD.bin and install script
        curl -fsSL -o "$TMP_DIR/PKGBUILD" "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/PKGBUILD.bin"
        curl -fsSL -o "$TMP_DIR/keyboard-middleware.install" "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/keyboard-middleware.install"

        cd "$TMP_DIR"
        makepkg -si --noconfirm
        echo "keyboard-middleware installed successfully via pacman (precompiled binary)"
    else
        echo "Building from source..."
        # Download PKGBUILD and install script
        curl -fsSL -o "$TMP_DIR/PKGBUILD" "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/PKGBUILD"
        curl -fsSL -o "$TMP_DIR/keyboard-middleware.install" "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/keyboard-middleware.install"

        cd "$TMP_DIR"
        makepkg -si --noconfirm
        echo "keyboard-middleware installed successfully via pacman (built from source)"
    fi
fi
