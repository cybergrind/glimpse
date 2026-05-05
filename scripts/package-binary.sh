#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
if [[ -z "$version" ]]; then
    version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
fi

arch="$(uname -m)"
case "$arch" in
    x86_64) ;;
    *)
        echo "unsupported binary release architecture: $arch" >&2
        exit 1
        ;;
esac

asset="glimpse-${version}-${arch}.tar.zst"
pkgroot="dist/pkgroot"

rm -rf "$pkgroot"
mkdir -p "$pkgroot/usr/bin" "$pkgroot/usr/lib/systemd/user" dist

cargo build --release --locked -p glimpse --bin glimpse-panel --no-default-features
cargo build --release --locked -p glimpse-shell
cargo build --release --locked -p glimpse-idle
cargo build --release --locked -p glimpse-sunset
cargo build --release --locked -p glimpse-wallpaper

test "$(target/release/glimpse-shell --version)" = "glimpse-shell $version"
test "$(target/release/glimpse-idle --version)" = "glimpse-idle $version"
test "$(target/release/glimpse-sunset --version)" = "glimpse-sunset $version"
test "$(target/release/glimpse-wallpaper --version)" = "glimpse-wallpaper $version"

install -Dm755 target/release/glimpse-panel "$pkgroot/usr/bin/glimpse-panel"
install -Dm755 target/release/glimpse-shell "$pkgroot/usr/bin/glimpse-shell"
install -Dm755 target/release/glimpse-idle "$pkgroot/usr/bin/glimpse-idle"
install -Dm755 target/release/glimpse-sunset "$pkgroot/usr/bin/glimpse-sunset"
install -Dm755 target/release/glimpse-wallpaper "$pkgroot/usr/bin/glimpse-wallpaper"
install -Dm644 data/glimpse-shell.service "$pkgroot/usr/lib/systemd/user/glimpse-shell.service"
install -Dm644 data/glimpse-idle.service "$pkgroot/usr/lib/systemd/user/glimpse-idle.service"
install -Dm644 data/glimpse-sunset.service "$pkgroot/usr/lib/systemd/user/glimpse-sunset.service"
install -Dm644 data/glimpse-wallpaper.service "$pkgroot/usr/lib/systemd/user/glimpse-wallpaper.service"

if [[ -f LICENSE ]]; then
    install -Dm644 LICENSE "$pkgroot/LICENSE"
fi

tar --zstd -cf "dist/$asset" -C "$pkgroot" .
b2sum "dist/$asset" > "dist/$asset.b2"

echo "dist/$asset"
