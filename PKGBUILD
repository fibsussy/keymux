# Maintainer: fibsussy <noahlykins@gmail.com>
# Local build - builds from current directory without network requests
pkgname=keyboard-middleware
pkgver=0.2.6
pkgrel=1
pkgdesc="QMK-inspired keyboard middleware with home row mods, layers, SOCD, and game mode"
arch=('x86_64' 'aarch64')
url="https://github.com/fibsussy/keyboard-middleware"
license=('MIT')
depends=('systemd')
makedepends=('rust' 'cargo')
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

    # Install systemd user service
    install -Dm644 "$pkgname.service" "$pkgdir/usr/lib/systemd/user/$pkgname.service"

    # Install example config
    install -Dm644 "config.example.ron" "$pkgdir/usr/share/doc/$pkgname/config.example.ron"

    # Generate and install shell completions
    install -dm755 "$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "$pkgdir/usr/share/fish/vendor_completions.d"

    "$pkgdir/usr/bin/$pkgname" completion bash > "$pkgdir/usr/share/bash-completion/completions/$pkgname"
    "$pkgdir/usr/bin/$pkgname" completion zsh > "$pkgdir/usr/share/zsh/site-functions/_$pkgname"
    "$pkgdir/usr/bin/$pkgname" completion fish > "$pkgdir/usr/share/fish/vendor_completions.d/$pkgname.fish"

    # Create config directory structure in package
    install -dm755 "$pkgdir/etc/skel/.config/$pkgname"
}
