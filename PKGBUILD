pkgname=glimpse
pkgver=0.1.0
pkgrel=1
pkgdesc="Wayland status panel and wallpaper daemon for the Glimpse ecosystem"
arch=('x86_64')
url="https://github.com/alex-oleshkevich/glimpse"
license=('custom:unknown')
depends=('gtk4' 'libadwaita' 'gtk4-layer-shell' 'libheif')
provides=('glimpse-panel' 'glimpse-wallpaper')
conflicts=('glimpse-panel' 'glimpse-wallpaper')
source_x86_64=("$pkgname-$pkgver-x86_64.tar.zst::$url/releases/download/v$pkgver/$pkgname-$pkgver-x86_64.tar.zst")
b2sums_x86_64=('SKIP')

package() {
    cp -a "$srcdir/usr" "$pkgdir/"
    if [[ -f "$srcdir/LICENSE" ]]; then
        install -Dm644 "$srcdir/LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
    fi
}
