#!/bin/bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 /path/to/Despatch.app" >&2
    exit 64
fi

bundle_dir="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
contents_dir="$bundle_dir/Contents"
plist="$contents_dir/Info.plist"
binary="$contents_dir/MacOS/Despatch"

[[ -d "$bundle_dir" ]] || { echo "missing app bundle: $bundle_dir" >&2; exit 1; }
[[ -f "$plist" ]] || { echo "missing Info.plist" >&2; exit 1; }
[[ -x "$binary" ]] || { echo "missing executable: $binary" >&2; exit 1; }

plutil -lint "$plist" >/dev/null

[[ "$(plutil -extract CFBundleIdentifier raw "$plist")" == "com.despatch.app" ]] || {
    echo "unexpected bundle identifier" >&2
    exit 1
}
[[ "$(plutil -extract CFBundleName raw "$plist")" == "Despatch" ]] || {
    echo "unexpected bundle name" >&2
    exit 1
}
[[ "$(plutil -extract CFBundleExecutable raw "$plist")" == "Despatch" ]] || {
    echo "unexpected bundle executable" >&2
    exit 1
}
[[ "$(plutil -extract LSMinimumSystemVersion raw "$plist")" == "13.0" ]] || {
    echo "unexpected minimum macOS version" >&2
    exit 1
}

codesign --verify --deep --strict --verbose=2 "$bundle_dir"

# Checked with `lipo` rather than `file` because a release bundle is universal, and `file` reports
# a fat binary as an archive rather than naming the slices inside it.
archs="$(lipo -archs "$binary")"
grep -qw arm64 <<<"$archs" || {
    echo "bundle has no arm64 slice (found: $archs)" >&2
    exit 1
}

# A release ships both architectures; a debug build is host-only by design.
if [[ "$bundle_dir" == */release/* ]]; then
    grep -qw x86_64 <<<"$archs" || {
        echo "release bundle is not universal (found: $archs)" >&2
        exit 1
    }
fi

echo "verified $bundle_dir ($archs)"

