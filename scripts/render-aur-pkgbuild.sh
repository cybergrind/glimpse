#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: scripts/render-aur-pkgbuild.sh <version> <x86_64-b2sum>}"
x86_64_b2sum="${2:?usage: scripts/render-aur-pkgbuild.sh <version> <x86_64-b2sum>}"

sed -E \
    -e "s/^pkgver=.*/pkgver=${version}/" \
    -e "s/^b2sums_x86_64=.*/b2sums_x86_64=('${x86_64_b2sum}')/" \
    PKGBUILD
