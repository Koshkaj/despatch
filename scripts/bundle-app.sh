#!/bin/bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
profile="debug"
cargo_args=()

if [[ "${1:-}" == "--release" ]]; then
    profile="release"
    cargo_args+=(--release)
elif [[ -n "${1:-}" ]]; then
    echo "usage: $0 [--release]" >&2
    exit 64
fi

bundle_dir="$project_root/target/$profile/Despatch.app"
contents_dir="$bundle_dir/Contents"

if [[ "$profile" == "release" ]]; then
    # A release is universal. An arm64-only build installs and launches on an Intel Mac only
    # through Rosetta, which is not installed by default, so the app appears broken rather than
    # unsupported. Debug builds stay host-only: the second target doubles the build for an
    # architecture nobody debugs on.
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --package despatch --release --target "$target"
    done
    binary_source="$project_root/target/despatch-universal"
    lipo -create -output "$binary_source" \
        "$project_root/target/aarch64-apple-darwin/release/despatch" \
        "$project_root/target/x86_64-apple-darwin/release/despatch"
else
    # bash 3.2 (what macOS ships) treats an empty array as unset under `set -u`, so the debug
    # build — the one with no extra arguments — has to guard the expansion.
    cargo build --package despatch ${cargo_args[@]+"${cargo_args[@]}"}
    binary_source="$project_root/target/$profile/despatch"
fi
plist_source="$project_root/apps/despatch/resources/Info.plist"
icon_source="$project_root/apps/despatch/resources/icon.png"

if [[ "$bundle_dir" != "$project_root"/target/*/Despatch.app ]]; then
    echo "refusing to replace unexpected bundle path: $bundle_dir" >&2
    exit 1
fi

rm -rf "$bundle_dir"
install -d "$contents_dir/MacOS" "$contents_dir/Resources"
install -m 0755 "$binary_source" "$contents_dir/MacOS/Despatch"
install -m 0644 "$plist_source" "$contents_dir/Info.plist"

# The icon is built here rather than committed as a binary: `icon.png` is the one source of truth,
# and every size in the set is derived from it, so no rendition can go stale on its own.
iconset_dir="$project_root/target/$profile/Despatch.iconset"
rm -rf "$iconset_dir"
mkdir -p "$iconset_dir"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$icon_source" \
        --out "$iconset_dir/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$icon_source" \
        --out "$iconset_dir/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil --convert icns "$iconset_dir" --output "$contents_dir/Resources/Despatch.icns"
rm -rf "$iconset_dir"

# The version is stamped rather than committed into the plist: `apps/despatch/Cargo.toml` is the
# single place it is written down, so a bundle cannot claim a version the workspace never built.
# shellcheck source=scripts/version.sh
source "$project_root/scripts/version.sh"
version="$(despatch_version "$project_root")"
plutil -replace CFBundleShortVersionString -string "$version" "$contents_dir/Info.plist"
plutil -replace CFBundleVersion -string "$version" "$contents_dir/Info.plist"

plutil -lint "$contents_dir/Info.plist" >/dev/null
codesign --force --sign - "$bundle_dir" >/dev/null

echo "$bundle_dir"

