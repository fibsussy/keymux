#!/bin/bash
set -euo pipefail

show_help() {
    cat <<EOF_HELP
Keyboard Middleware Installer

Usage: $0 [OPTION] [VERSION]

Options:
  local     Build local WIP version from current directory
  git       Build from git source (clone repo)
  bin       Install precompiled binary (minimum steps)
  -v, --version VERSION  Install specific version
  --help    Show this help message

Examples:
  $0 local            # Build from current directory
  $0 git              # Clone and build from git
  $0 bin              # Install latest binary release
  $0 git -v v1.2.0   # Clone specific version and build
  $0 bin -v v1.2.0    # Install specific binary version
EOF_HELP
    exit 0
}

MODE=""
VERSION=""
START_DIR=$(pwd)

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
        local|git|bin)
            MODE="$1"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            show_help
            ;;
    esac
done

# Validate options
if [ -n "$VERSION" ] && [ "$MODE" = "local" ]; then
    echo "Error: --version is incompatible with local mode"
    exit 1
fi

# Auto-detect mode if not specified
if [ -z "$MODE" ]; then
    if [ -d "$START_DIR/.git" ]; then
        if ! git -C "$START_DIR" diff --quiet || ! git -C "$START_DIR" diff --cached --quiet 2>/dev/null; then
            MODE="local"
        else
            MODE="git"
        fi
    else
        MODE="bin"
    fi
fi

local_build() {
    echo "Building local version from $START_DIR"
    
    # Create temp directory with cleanup trap
    temp_dir=$(mktemp -d -t keymux.XXXXXX)
    cd "$temp_dir"
    trap 'rm -rf "$temp_dir"' EXIT
    
    # Copy tracked and untracked files (excluding .git, target, etc.)
    echo "Copying source files..."
    mkdir keymux-src
    cd keymux-src
    
    # Copy git tracked files
    git -C "$START_DIR" ls-files | while IFS= read -r file; do
        mkdir -p "$(dirname "$file")"
        cp "$START_DIR/$file" "$file"
    done
    
    # Copy untracked files (excluding common build/cache dirs)
    git -C "$START_DIR" ls-files --others --exclude-standard | while IFS= read -r file; do
        if [[ ! "$file" =~ ^(target/|pkg/|*.pkg\.tar\.zst|src/.*/target/) ]]; then
            mkdir -p "$(dirname "$file")"
            cp "$START_DIR/$file" "$file"
        fi
    done
    
    # Create PKGBUILD for local build
    cat > PKGBUILD << 'EOF'
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
source=("keymux.tar.gz")
sha256sums=('SKIP')

pkgver() {
    local version=$(grep '^version = ' Cargo.toml | head -n1 | cut -d'"' -f2)
    local commit=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
    local date=$(date +%Y%m%d)
    echo "$version+r$commit.$date+wip"
}

prepare() {
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release
}

package() {
    local binary_name="keymux"
    
    install -Dm755 "target/release/$binary_name" "$pkgdir/usr/bin/$binary_name"
    install -Dm644 "$binary_name.service" "$pkgdir/usr/lib/systemd/system/$binary_name.service"
    install -Dm644 "$binary_name-niri.service" "$pkgdir/usr/lib/systemd/user/$binary_name-niri.service"
    install -Dm644 "config.example.ron" "$pkgdir/usr/share/doc/$binary_name/config.example.ron"
    install -Dm644 README.md "$pkgdir/usr/share/doc/$binary_name/README.md"
    
    install -dm755 "$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "$pkgdir/usr/share/fish/vendor_completions.d"
    
    "target/release/$binary_name" completion bash > "$pkgdir/usr/share/bash-completion/completions/$binary_name"
    "target/release/$binary_name" completion zsh > "$pkgdir/usr/share/zsh/site-functions/_$binary_name"
    "target/release/$binary_name" completion fish > "$pkgdir/usr/share/fish/vendor_completions.d/$binary_name.fish"
    
    install -dm755 "$pkgdir/etc/skel/.config/$binary_name"
}
EOF
    
    # Create tarball and install
    tar czf ../keymux.tar.gz --exclude=PKGBUILD .
    cd ..
    cp keymux-src/PKGBUILD .
    cp keymux-src/keymux.install . 2>/dev/null || echo "Warning: keymux.install not found"
    
    echo "Building and installing package..."
    makepkg -si
}

git_build() {
    echo "Building from git source"
    
    # Create temp directory with cleanup trap
    temp_dir=$(mktemp -d -t keymux.XXXXXX)
    cd "$temp_dir"
    trap 'rm -rf "$temp_dir"' EXIT
    
    # Clone repository
    if [ -n "$VERSION" ]; then
        echo "Cloning version $VERSION..."
        git clone --depth 1 --branch "$VERSION" https://github.com/fibsussy/keymux.git
    else
        echo "Cloning main branch..."
        git clone --depth 1 https://github.com/fibsussy/keymux.git
    fi
    
    cd keymux
    
    # Build and install using PKGBUILD-git
    if [ -f PKGBUILD-git ]; then
        cp PKGBUILD-git PKGBUILD
    fi
    
    echo "Building and installing package..."
    makepkg -si
}

bin_install() {
    echo "Installing binary package"
    
    # Create temp directory with cleanup trap
    temp_dir=$(mktemp -d -t keymux.XXXXXX)
    cd "$temp_dir"
    trap 'rm -rf "$temp_dir"' EXIT
    
    # Download PKGBUILD-bin
    if [ -n "$VERSION" ]; then
        echo "Downloading PKGBUILD for version $VERSION..."
        curl -fsSL -o PKGBUILD "https://raw.githubusercontent.com/fibsussy/keymux/main/PKGBUILD-bin"
        sed -i "s/pkgver=.*/pkgver=${VERSION#v}/" PKGBUILD
    else
        echo "Downloading latest PKGBUILD..."
        curl -fsSL -o PKGBUILD "https://raw.githubusercontent.com/fibsussy/keymux/main/PKGBUILD-bin"
    fi
    
    curl -fsSL -o keymux.install "https://raw.githubusercontent.com/fibsussy/keymux/main/keymux.install"
    
    echo "Installing package..."
    makepkg -si
}

# Main execution
case "$MODE" in
    local)
        if [ ! -d "$START_DIR/.git" ]; then
            echo "Error: local mode requires being in a git repository"
            exit 1
        fi
        local_build
        ;;
    git)
        git_build
        ;;
    bin)
        bin_install
        ;;
    *)
        echo "Error: Unknown mode '$MODE'"
        show_help
        ;;
esac

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Installation complete!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━"
