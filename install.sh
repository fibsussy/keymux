#!/bin/bash
set -euo pipefail

show_help() {
    cat <<EOF_HELP
Keyboard Middleware Installer

Usage: $0 [OPTION] [VERSION]

Options:
  local     Build local WIP version (auto-detected with uncommitted changes)
  git       Build from git source
  bin       Install precompiled binary (default behavior)
  -v, --version VERSION  Install specific git tag/version
  --help    Show this help message

Examples:
  $0                  # Auto-detect: WIP if dirty, git if clean repo, bin if not a repo
  $0 local             # Force local WIP build (only works with uncommitted changes)
  $0 git               # Build from git source
  $0 bin               # Install latest binary
  $0 -v v1.2.0         # Install version v1.2.0 from git
  $0 bin -v v1.2.0     # Install version v1.2.0 binary
EOF_HELP
    exit 0
}

MODE=""
VERSION=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            ;;
        -v|--version)
            VERSION="$2"
            shift 2
            ;;
        local)
            MODE="local"
            shift
            ;;
        bin)
            MODE="bin"
            shift
            ;;
        git)
            MODE="git"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            show_help
            ;;
    esac
done

START_DIR=$(pwd)

# Detect if we're being piped/redirected (remote execution)
if [ ! -t 0 ]; then
    # We're running via curl | bash, we're remote
    IS_REMOTE=true
else
    # Check if we're in a git repository
    if [ -d "$START_DIR/.git" ]; then
        IS_REMOTE=false
    else
        IS_REMOTE=true
    fi
fi

# Validate incompatible options
if [ -n "$VERSION" ] && [ "$MODE" = "local" ]; then
    echo "Error: --version is incompatible with local mode (local builds are always WIP)"
    exit 1
fi

# Atomic build in subshell that self-destructs
build_and_install() {
    (
        # Create temp directory that gets destroyed when subshell exits
        cd "$(mktemp -d)"
        
        # Set up cleanup trap for this subshell
        trap 'rm -rf "$PWD"' EXIT
        
        echo "Working in temporary directory: $PWD"
        
        # Copy/source files from original directory
        if [ "$IS_REMOTE" = "false" ] && [ -f "$START_DIR/PKGBUILD" ] && [ -f "$START_DIR/Cargo.toml" ] && [ -z "$VERSION" ]; then
            echo "Detected keymux repository..."
            
            # Copy entire repository first for all local modes
            cp -r "$START_DIR" ./
            local repo_name=$(basename "$START_DIR")
            
            # Auto-detect mode if not specified
            if [ -z "$MODE" ]; then
                if ! git -C "$repo_name" diff --quiet || ! git -C "$repo_name" diff --cached --quiet 2>/dev/null; then
                    echo "Found uncommitted changes, using local WIP build"
                    MODE="local"
                else
                    echo "Repository is clean, using git build"
                    MODE="git"
                fi
            fi
            
            if [ "$MODE" = "bin" ]; then
                echo "Installing binary package..."
                cp "$START_DIR/PKGBUILD-bin" ./PKGBUILD
                cp "$START_DIR/keymux.install" ./
                makepkg -si
            elif [ "$MODE" = "local" ]; then
                echo "Building local WIP package..."
                # Create a simple PKGBUILD for local build
                cat > PKGBUILD << EOF
# Maintainer: fibsussy <fibsussy@tuta.io>
pkgname=keymux-local
pkgver=1.0.0
pkgrel=1
pkgdesc="Keyboard middleware for gaming with low-level input interception (local WIP version)"
arch=('x86_64' 'aarch64')
url="https://github.com/fibsussy/keymux"
license=('MIT')
depends=('systemd' 'udev')
makedepends=('rust' 'cargo')
optdepends=('niri: automatic game mode detection in Niri compositor')
provides=('keymux')
conflicts=('keymux' 'keymux-bin' 'keymux-git')
options=('!debug')
install=keymux.install
source=("keymux.tar.gz")
sha256sums=('SKIP')

pkgver() {
    cd "$repo_name"
    local version=\$(grep '^version = ' Cargo.toml | head -n1 | cut -d'"' -f2)
    local commit=\$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
    local date=\$(date +%Y%m%d)
    echo "\$version+r\$commit.\$date+wip"
}

prepare() {
    cd "$repo_name"
    cargo fetch --locked --target "\$CARCH-unknown-linux-gnu"
}

build() {
    cd "$repo_name"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release
}

package() {
    cd "$repo_name"
    local binary_name="keymux"
    
    install -Dm755 "target/release/\$binary_name" "\$pkgdir/usr/bin/\$binary_name"
    install -Dm644 "\$binary_name.service" "\$pkgdir/usr/lib/systemd/system/\$binary_name.service"
    install -Dm644 "\$binary_name-niri.service" "\$pkgdir/usr/lib/systemd/user/\$binary_name-niri.service"
    install -Dm644 "config.example.ron" "\$pkgdir/usr/share/doc/\$binary_name/config.example.ron"
    install -Dm644 README.md "\$pkgdir/usr/share/doc/\$binary_name/README.md"
    
    install -dm755 "\$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "\$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "\$pkgdir/usr/share/fish/vendor_completions.d"
    
    "target/release/\$binary_name" completion bash > "\$pkgdir/usr/share/bash-completion/completions/\$binary_name"
    "target/release/\$binary_name" completion zsh > "\$pkgdir/usr/share/zsh/site-functions/_\$binary_name"
    "target/release/\$binary_name" completion fish > "\$pkgdir/usr/share/fish/vendor_completions.d/\$binary_name.fish"
    
    install -dm755 "\$pkgdir/etc/skel/.config/\$binary_name"
}
EOF
                # Create tarball of the source (excluding target directory and other build artifacts)
                tar czf keymux.tar.gz --exclude="$repo_name/target" --exclude="$repo_name/*.tar.gz" --exclude="$repo_name/.PKGINFO" --exclude="$repo_name/pkg" -C . "$repo_name"
                cp "$repo_name/keymux.install" ./
                makepkg -si
            elif [ "$MODE" = "git" ]; then
                echo "Building git package from remote repository..."
                
                # Move to a clean directory for git build to avoid repo detection conflicts
                cd "$(mktemp -d)"
                
                # Copy the fixed git PKGBUILD that clones from remote
                cp "$START_DIR/PKGBUILD-git" ./PKGBUILD
                cp "$START_DIR/keymux.install" ./keymux.install
                echo "Running makepkg to build and install package..."
                if [ -t 0 ] && [ -t 1 ]; then
                    makepkg -si
                else
                    echo "Not building interactively, creating package only..."
                    makepkg -s
                    echo "Package built. To install manually:"
                    echo "  sudo pacman -U keymux-git-*.pkg.tar.zst"
                fi
            fi

        else
            echo "Installing from remote repository..."
            [ -z "$MODE" ] && MODE="bin"
            
            if [ "$MODE" = "bin" ]; then
                echo "Installing binary package..."
                if [ -n "$VERSION" ]; then
                    curl -fsSL -o PKGBUILD "https://raw.githubusercontent.com/fibsussy/keymux/main/PKGBUILD-bin"
                    sed -i "s/pkgver=.*/pkgver=${VERSION#v}/" PKGBUILD
                else
                    curl -fsSL -o PKGBUILD "https://raw.githubusercontent.com/fibsussy/keymux/main/PKGBUILD-bin"
                fi
                curl -fsSL -o keymux.install "https://raw.githubusercontent.com/fibsussy/keymux/main/keymux.install"
            else
                echo "Building git package..."
                curl -fsSL -o PKGBUILD "https://raw.githubusercontent.com/fibsussy/keymux/main/PKGBUILD-git"
                curl -fsSL -o keymux.install "https://raw.githubusercontent.com/fibsussy/keymux/main/keymux.install"
            fi
            
            makepkg -si
        fi
    )
}

# Run the atomic build
build_and_install

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Installation complete! To enable the services:"
echo ""
echo "  Root daemon (required):"
echo "    sudo systemctl enable --now keymux.service"
echo ""
echo "  Niri watcher (optional, for auto game mode):"
echo "    systemctl --user enable --now keymux-niri.service"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
