# Maintainer: fibsussy <fibsussy@tuta.io>
pkgname=keymux-local
pkgver=1.0.9
pkgrel=1
pkgdesc="Keyboard middleware for gaming with low-level input interception (local build)"
arch=('x86_64')
url="https://github.com/fibsussy/keymux"
license=('MIT')
depends=('gcc-libs' 'systemd' 'libevdev')
makedepends=('cargo' 'git')
options=('!lto')
install='keymux.install'
provides=('keymux')
conflicts=('keymux-bin' 'keymux-git')
source=("git+https://github.com/fibsussy/keymux.git")
sha256sums=('SKIP')

prepare() {
    cd "keymux"
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "keymux"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release --frozen --all-targets
}

package() {
    cd "keymux"
    install -Dm755 "target/release/keymux" "$pkgdir/usr/bin/keymux"

    install -Dm644 "keymux.service" "$pkgdir/usr/lib/systemd/system/keymux.service"
    install -Dm644 "keymux-niri.service" "$pkgdir/usr/lib/systemd/user/keymux-niri.service"
    install -Dm644 "config.example.ron" "$pkgdir/usr/share/doc/keymux/config.example.ron"
    install -Dm644 "README.md" "$pkgdir/usr/share/doc/keymux/README.md"
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/keymux/LICENSE"
    
    # Generate shell completions
    install -dm755 "$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "$pkgdir/usr/share/fish/vendor_completions.d"
    
    "$pkgdir/usr/bin/keymux" completion bash > "$pkgdir/usr/share/bash-completion/completions/keymux"
    "$pkgdir/usr/bin/keymux" completion zsh > "$pkgdir/usr/share/zsh/site-functions/_keymux"
    "$pkgdir/usr/bin/keymux" completion fish > "$pkgdir/usr/share/fish/vendor_completions.d/keymux.fish"
    
    install -dm755 "$pkgdir/etc/skel/.config/keymux"
}