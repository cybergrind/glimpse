pkgname=glimpse-panel-git
_pkgname=glimpse
pkgver=0.1.0.r188.ge23f6e0
pkgrel=1
pkgdesc="Wayland status panel for the Glimpse ecosystem"
arch=('x86_64' 'aarch64')
license=('custom:unknown')
makedepends=('cargo' 'git' 'pkgconf')
depends=('gtk4' 'libadwaita' 'gtk4-layer-shell' 'libheif')
provides=('glimpse-panel')
conflicts=('glimpse-panel')
source=("$_pkgname::git+file://$PWD")
b2sums=('SKIP')

pkgver() {
    cd "$srcdir/$_pkgname"

    local base_version=0.1.0
    local revision
    local short_hash

    revision=$(git rev-list --count HEAD)
    short_hash=$(git rev-parse --short HEAD)

    printf '%s.r%s.g%s\n' "$base_version" "$revision" "$short_hash"
}

build() {
    cd "$srcdir/$_pkgname"
    export CARGO_TARGET_DIR=target

    cargo build --release --locked -p glimpse --bin glimpse-panel --no-default-features
}

package() {
    cd "$srcdir/$_pkgname"

    install -Dm755 "target/release/glimpse-panel" "$pkgdir/usr/bin/glimpse-panel"
    install -Dm644 "data/glimpse-panel.service" \
        "$pkgdir/usr/lib/systemd/user/glimpse-panel.service"
}
