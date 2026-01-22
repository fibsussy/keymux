# Maintainer: fibsussy <fibsussy@tuta.io>
pkgname=keymux-local
pkgver=1.0.1+()
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
    install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
    install -Dm644 "../LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
    install -d "$pkgdir/etc/keymux"
}