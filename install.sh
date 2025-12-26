#!/bin/bash

set -euo pipefail

# Show help
show_help() {
    cat <<EOF
Keyboard Middleware Installer

Usage: $0 [OPTION]

Options:
  (none)    Smart default: local build in repo, precompiled binary remotely
  local     Force build from source (requires rust/cargo)
  bin       Force precompiled binary install
  --help    Show this help message

Examples:
  # Local: Build from source (default when in repo)
  $0

  # Local: Force precompiled binary
  $0 bin

  # Remote: Install precompiled binary (default)
  curl -fsSL https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/install.sh | bash

  # Remote: Build from source (requires rust/cargo)
  curl -fsSL https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/install.sh | bash -s local
EOF
    exit 0
}

# Parse arguments
MODE=""
if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    show_help
elif [ "${1:-}" = "bin" ]; then
    MODE="bin"
elif [ "${1:-}" = "local" ]; then
    MODE="local"
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
    # ====================================================================
    # LOCAL INSTALL
    # ====================================================================
    echo "Detected local repository, using local files..."

    # Default to local build if no mode specified
    if [ -z "$MODE" ]; then
        MODE="local"
    fi

    if [ "$MODE" = "bin" ]; then
        # Local precompiled binary install
        echo "Installing precompiled binary (using PKGBUILD.bin)..."

        (
            # Create atomic temp directory with guaranteed cleanup
            TMP_DIR=$(mktemp -d -t keyboard-middleware-install.XXXXXX)
            trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

            # Copy minimal files needed
            cp "$START_DIR/PKGBUILD.bin" "$TMP_DIR/PKGBUILD"
            cp "$START_DIR/keyboard-middleware.install" "$TMP_DIR/"

            # Build and install in subshell
            cd "$TMP_DIR"
            makepkg -si
        )

        echo "keyboard-middleware installed successfully via pacman (precompiled binary)"
    else
        # Local source build - run directly in repo (no temp dir needed)
        echo "Building from source (using PKGBUILD)..."
        makepkg -si
        echo "keyboard-middleware installed successfully via pacman (built from source)"
    fi
else
    # ====================================================================
    # REMOTE INSTALL
    # ====================================================================
    echo "Remote install detected..."

    # Default to precompiled binary if no mode specified
    if [ -z "$MODE" ]; then
        MODE="bin"
    fi

    if [ "$MODE" = "bin" ]; then
        echo "Installing precompiled binary from GitHub..."

        (
            # Create atomic temp directory with guaranteed cleanup
            TMP_DIR=$(mktemp -d -t keyboard-middleware-install.XXXXXX)
            trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

            # Download minimal files needed
            echo "Downloading PKGBUILD.bin and install script..."
            curl -fsSL -o "$TMP_DIR/PKGBUILD" "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/PKGBUILD.bin"
            curl -fsSL -o "$TMP_DIR/keyboard-middleware.install" "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/keyboard-middleware.install"

            # Build and install in subshell
            cd "$TMP_DIR"
            makepkg -si --noconfirm
        )

        echo "keyboard-middleware installed successfully via pacman (precompiled binary)"
    else
        echo "Building from source from GitHub..."

        (
            # Create atomic temp directory with guaranteed cleanup
            TMP_DIR=$(mktemp -d -t keyboard-middleware-install.XXXXXX)
            trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

            # Clone the repository
            echo "Cloning repository..."
            git clone https://github.com/fibsussy/keyboard-middleware.git "$TMP_DIR/repo"

            # Build and install in subshell
            cd "$TMP_DIR/repo"
            makepkg -si --noconfirm
        )

        echo "keyboard-middleware installed successfully via pacman (built from source)"
    fi
fi
