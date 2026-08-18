#!/bin/bash
set -euo pipefail

# Packages the app bundle as a compressed disk image with the usual drag-to-install layout: the
# app on the left, a symlink to /Applications on the right.

project_root="$(cd "$(dirname "$0")/.." && pwd)"
profile="release"
bundle_args=(--release)

if [[ "${1:-}" == "--debug" ]]; then
    profile="debug"
    bundle_args=()
elif [[ -n "${1:-}" ]]; then
    echo "usage: $0 [--debug]" >&2
    exit 64
fi

# bash 3.2 (what macOS ships) treats an empty array as unset under `set -u`.
bundle_dir="$("$project_root/scripts/bundle-app.sh" ${bundle_args[@]+"${bundle_args[@]}"} | tail -n 1)"
"$project_root/scripts/verify-app.sh" "$bundle_dir" >/dev/null

version="$(plutil -extract CFBundleShortVersionString raw "$bundle_dir/Contents/Info.plist")"
volume_name="Despatch $version"
dmg_path="$project_root/target/$profile/Despatch-$version.dmg"
staging_dir="$project_root/target/$profile/dmg-staging"

if [[ "$staging_dir" != "$project_root"/target/*/dmg-staging ]]; then
    echo "refusing to replace unexpected staging path: $staging_dir" >&2
    exit 1
fi

rm -rf "$staging_dir"
mkdir -p "$staging_dir"
cp -R "$bundle_dir" "$staging_dir/"
ln -s /Applications "$staging_dir/Applications"

# A stale image left over from an interrupted run would still be mounted, and hdiutil would fail
# on the busy path rather than on anything the caller can act on.
if hdiutil info | grep -q "^image-path.*$(basename "$dmg_path")$"; then
    hdiutil detach "/Volumes/$volume_name" >/dev/null 2>&1 || true
fi

rm -f "$dmg_path"
hdiutil create \
    -volname "$volume_name" \
    -srcfolder "$staging_dir" \
    -fs HFS+ \
    -format UDZO \
    -imagekey zlib-level=9 \
    -quiet \
    "$dmg_path"

rm -rf "$staging_dir"

# An unreadable image is worse than a missing one: it fails at the user's machine, not here.
hdiutil verify -quiet "$dmg_path"

echo "$dmg_path"
