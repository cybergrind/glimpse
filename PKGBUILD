pkgname=glimpse-panel-git
_pkgname=glimpse
_srcname=glimpse-panel-git-src
pkgver=0.1.0.r252.g739def5
pkgrel=1
pkgdesc="Wayland status panel and wallpaper daemon for the Glimpse ecosystem"
arch=('x86_64' 'aarch64')
license=('custom:unknown')
makedepends=('cargo' 'git' 'pkgconf')
depends=('gtk4' 'libadwaita' 'gtk4-layer-shell' 'libheif')
provides=('glimpse-panel' 'glimpse-wallpaper')
conflicts=('glimpse-panel' 'glimpse-wallpaper')
source=("$_srcname::git+file://$PWD")
b2sums=('SKIP')

pkgver() {
    cd "$srcdir/$_srcname"

    local base_version=0.1.0
    local revision
    local short_hash

    revision=$(git rev-list --count HEAD)
    short_hash=$(git rev-parse --short HEAD)

    printf '%s.r%s.g%s\n' "$base_version" "$revision" "$short_hash"
}

build() {
    cd "$srcdir/$_srcname"
    export CARGO_TARGET_DIR=target

    cargo build --release -p glimpse --bin glimpse-panel --no-default-features
    cargo build --release -p glimpse-wallpaper
}

package() {
    cd "$srcdir/$_srcname"

    install -Dm755 "target/release/glimpse-panel" "$pkgdir/usr/bin/glimpse-panel"
    install -Dm755 "target/release/glimpse-wallpaper" "$pkgdir/usr/bin/glimpse-wallpaper"
    install -Dm644 "data/glimpse-panel.service" \
        "$pkgdir/usr/lib/systemd/user/glimpse-panel.service"
    install -Dm644 "data/glimpse-wallpaper.service" \
        "$pkgdir/usr/lib/systemd/user/glimpse-wallpaper.service"
}
