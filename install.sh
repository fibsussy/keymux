#!/bin/bash
set -euo pipefail

show_help() {
    cat <<EOF_HELP
Keyboard Middleware Installer

Usage: $0 [OPTION]

Options:
  local     Build from source (default if in repo)
  bin       Install precompiled binary
  --help    Show this help message
EOF_HELP
    exit 0
}

MODE=""
if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    show_help
elif [ "${1:-}" = "bin" ]; then
    MODE="bin"
elif [ "${1:-}" = "local" ]; then
    MODE="local"
fi

START_DIR=$(pwd)

if [ -f "$START_DIR/PKGBUILD" ] && [ -f "$START_DIR/Cargo.toml" ]; then
    echo "Detected local repository..."
    [ -z "$MODE" ] && MODE="local"

    if [ "$MODE" = "bin" ]; then
        TMP_DIR=$(mktemp -d)
        trap 'rm -rf "$TMP_DIR"' EXIT
        cp "$START_DIR/PKGBUILD.bin" "$TMP_DIR/PKGBUILD"
        cp "$START_DIR/keyboard-middleware.install" "$TMP_DIR/"
        cd "$TMP_DIR"
        makepkg -si
    else
        TMP_DIR=$(mktemp -d)
        trap 'rm -rf "$TMP_DIR"' EXIT
        cd "$START_DIR"
        git ls-files -z | xargs -0 tar cf - | (cd "$TMP_DIR" && tar xf -)
        cd "$TMP_DIR"
        echo "Building package as normal user..."
        makepkg

        echo "Installing package as root..."
        sudo -v
        sudo pacman -U --noconfirm *.pkg.tar.zst
    fi
else
    echo "Remote install..."
    [ -z "$MODE" ] && MODE="bin"
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT
    cd "$TMP_DIR"
    if [ "$MODE" = "bin" ]; then
        curl -fsSL -o PKGBUILD "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/PKGBUILD.bin"
        curl -fsSL -o keyboard-middleware.install "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/keyboard-middleware.install"
    else
        git clone https://github.com/fibsussy/keyboard-middleware.git repo
        cd repo
        curl -fsSL -o PKGBUILD "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/PKGBUILD"
        curl -fsSL -o keyboard-middleware.install "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/keyboard-middleware.install"
    fi

    echo "Building package as normal user..."
    makepkg

    echo "Installing package as root..."
    sudo -v
    sudo pacman -U --noconfirm *.pkg.tar.zst
fi
