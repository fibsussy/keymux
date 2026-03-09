# Maintainer: fibsussy <fibsussy@tuta.io>
pkgname=keymux
pkgver=1.1.0
pkgrel=1
pkgdesc="Keyboard middleware for gaming with low-level input interception"
arch=('x86_64' 'aarch64')
url="https://github.com/fibsussy/keymux"
license=('MIT')
depends=('systemd' 'udev' 'libevdev')
makedepends=('rust' 'cargo')
optdepends=('niri: automatic game mode detection in Niri compositor')
options=('!debug')

source=()
sha256sums=()

build() {
    cd "$startdir"
    cargo build --release --locked
}

package() {
    cd "$startdir"
    install -Dm755 "target/release/keymux" "$pkgdir/usr/bin/keymux"
    install -Dm644 "keymux.service" "$pkgdir/usr/lib/systemd/system/keymux.service"
    install -Dm644 "keymux-niri.service" "$pkgdir/usr/lib/systemd/user/keymux-niri.service"
    install -Dm644 "config.example.ron" "$pkgdir/usr/share/doc/keymux/config.example.ron"
    install -Dm644 "README.md" "$pkgdir/usr/share/doc/keymux/README.md"
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/keymux/LICENSE"
    install -dm755 "$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "$pkgdir/usr/share/fish/vendor_completions.d"
    "$pkgdir/usr/bin/keymux" completion bash > "$pkgdir/usr/share/bash-completion/completions/keymux"
    "$pkgdir/usr/bin/keymux" completion zsh > "$pkgdir/usr/share/zsh/site-functions/_keymux"
    "$pkgdir/usr/bin/keymux" completion fish > "$pkgdir/usr/share/fish/vendor_completions.d/keymux.fish"
    install -dm755 "$pkgdir/etc/skel/.config/keymux"
}
