# Maintainer: fibsussy <noahlykins@gmail.com>
# Local build - builds from current directory without network requests
pkgname=keyboard-middleware
pkgver=0.6.2
pkgrel=1
pkgdesc="QMK-inspired keyboard middleware with home row mods, layers, SOCD, and game mode"
arch=('x86_64' 'aarch64')
url="https://github.com/fibsussy/keyboard-middleware"
license=('MIT')
depends=('systemd' 'udev')
makedepends=('rust' 'cargo')
optdepends=('niri: automatic game mode detection in Niri compositor')
options=('!debug')
install=$pkgname.install

source=()
sha256sums=()

build() {
    cd "$startdir"
    cargo build --release --locked
}

package() {
    cd "$startdir"

    # Install compiled binary
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"

    # Install systemd system service (runs as root)
    install -Dm644 "$pkgname.service" "$pkgdir/usr/lib/systemd/system/$pkgname.service"

    # Install systemd user service (niri watcher)
    install -Dm644 "$pkgname-niri.service" "$pkgdir/usr/lib/systemd/user/$pkgname-niri.service"

    # Install example config
    install -Dm644 "config.example.ron" "$pkgdir/usr/share/doc/$pkgname/config.example.ron"

    # Generate and install shell completions for main binary
    install -dm755 "$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "$pkgdir/usr/share/fish/vendor_completions.d"

    "$pkgdir/usr/bin/$pkgname" completion bash > "$pkgdir/usr/share/bash-completion/completions/$pkgname"
    "$pkgdir/usr/bin/$pkgname" completion zsh > "$pkgdir/usr/share/zsh/site-functions/_$pkgname"
    "$pkgdir/usr/bin/$pkgname" completion fish > "$pkgdir/usr/share/fish/vendor_completions.d/$pkgname.fish"

    # Create config directory structure in package
    install -dm755 "$pkgdir/etc/skel/.config/$pkgname"
}
