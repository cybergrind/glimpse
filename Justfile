set shell := ["bash", "-euo", "pipefail", "-c"]

aur_pkg := "glimpse-desktop-bin"
aur_remote := "ssh://aur@aur.archlinux.org/glimpse-desktop-bin.git"
github_repo := "alex-oleshkevich/glimpse"

default:
    @just --list

version:
    @awk -F'"' '/^version = / { print $2; exit }' Cargo.toml

sync-pkgver:
    sed -i -E "s/^pkgver=.*/pkgver=$(just version)/" PKGBUILD

verify-release: sync-pkgver
    cargo test --locked -p glimpse-core
    cargo test --locked -p glimpse-sunset
    cargo test --locked -p glimpse-wallpaper
    cargo check --locked -p glimpse-shell
    cargo check --locked -p glimpse-sunset
    cargo check --locked -p glimpse-wallpaper

binary-package: verify-release
    scripts/package-binary.sh "$(just version)"

aur-pkgbuild:
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(just version)"
    asset="glimpse-${version}-$(uname -m).tar.zst"
    url="https://github.com/{{github_repo}}/releases/download/v${version}/${asset}"
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT
    curl -fsSL "$url" -o "$tmpdir/$asset"
    checksum="$(b2sum "$tmpdir/$asset" | awk '{ print $1 }')"
    scripts/render-aur-pkgbuild.sh "$version" "$checksum" > dist/PKGBUILD

aur-srcinfo: aur-pkgbuild
    makepkg -p dist/PKGBUILD --printsrcinfo > dist/.SRCINFO

aur-publish: aur-pkgbuild
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(just version)"
    asset="dist/glimpse-${version}-$(uname -m).tar.zst"
    test -f "$asset"
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT
    git clone "{{aur_remote}}" "$tmpdir"
    cp dist/PKGBUILD "$tmpdir"/
    cd "$tmpdir"
    makepkg --printsrcinfo > .SRCINFO
    git add PKGBUILD .SRCINFO
    if git diff --cached --quiet; then
        echo "AUR package {{aur_pkg}} already up to date"
    else
        git commit -m "Release ${version}"
        git push origin master
    fi

github-release: binary-package
    #!/usr/bin/env bash
    set -euo pipefail
    tag="v$(just version)"
    asset="dist/glimpse-$(just version)-$(uname -m).tar.zst"
    gh release create "$tag" "$asset" --verify-tag --title "$tag" --notes "Glimpse $(just version)" || gh release upload "$tag" "$asset" --clobber

release-local: binary-package
    #!/usr/bin/env bash
    set -euo pipefail
    tag="v$(just version)"
    asset="dist/glimpse-$(just version)-$(uname -m).tar.zst"
    git diff --quiet
    git diff --cached --quiet
    git rev-parse "$tag" >/dev/null 2>&1 || git tag -a "$tag" -m "Release $tag"
    git push origin HEAD
    git push origin "$tag"
    gh release create "$tag" "$asset" --verify-tag --title "$tag" --notes "Glimpse $(just version)" || gh release upload "$tag" "$asset" --clobber
    just aur-publish

act-ci:
    act push -W .github/workflows/ci.yml

act-release:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="v$(just version)"
    act push -W .github/workflows/release.yml -e <(printf '{"ref":"refs/tags/%s","ref_name":"%s"}\n' "$tag" "$tag")

watch-runs:
    #!/usr/bin/env bash
    set -euo pipefail
    gh run list --limit 10
    run_id="$(gh run list --limit 1 --json databaseId --jq '.[0].databaseId')"
    test -n "$run_id"
    gh run watch "$run_id" --exit-status
