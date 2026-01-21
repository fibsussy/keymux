# Maintainer: fibsussy <fibsussy@tuta.io>
pkgname=keymux
pkgver=1.0.0+()
pkgrel=1
pkgdesc="Keyboard middleware for gaming with low-level input interception"
arch=('x86_64')
url="https://github.com/fibsussy/keymux"
license=('MIT')
depends=('gcc-libs' 'systemd' 'evdev')
makedepends=('cargo' 'git')
options=('!lto')
install='keymux.install'
source=("git+https://github.com/fibsussy/keymux.git#tag=v1.0.0
sha256sums=('SKIP')

prepare() {
    cd "$pkgname"
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release --frozen --all-targets
}

package() {
    cd "$pkgname"
    install -Dm755 "target/$CARCH-unknown-linux-gnu/release/keymux" "$pkgdir/usr/bin/keymux"
    install -Dm755 "target/$CARCH-unknown-linux-gnu/release/keymux-niri" "$pkgdir/usr/bin/keymux-niri"
    install -Dm644 "keymux.service" "$pkgdir/usr/lib/systemd/system/keymux.service"
    install -Dm644 "keymux-niri.service" "$pkgdir/usr/lib/systemd/user/keymux-niri.service"
    install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
    install -d "$pkgdir/etc/keymux"
}