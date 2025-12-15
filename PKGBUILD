# Maintainer: Your Name <your.email@example.com>
pkgname=keyboard-middleware
pkgver=0.1.0
pkgrel=1
pkgdesc="Multi-keyboard middleware with home row mods, SOCD cleaner, and game mode support"
arch=('x86_64')
url="https://github.com/yourusername/keyboard-middleware"
license=('MIT')
depends=('systemd')
makedepends=('cargo' 'rust')
source=()
sha256sums=()

build() {
    cd "$startdir"
    cargo build --release --locked
}

package() {
    cd "$startdir"

    # Install binary
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"

    # Install systemd user service
    install -Dm644 "$pkgname.service" "$pkgdir/usr/lib/systemd/user/$pkgname.service"
}
