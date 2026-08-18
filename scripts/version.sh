# The version every artifact is named after.
#
# `apps/despatch/Cargo.toml` is the only place it is written down. The bundle's Info.plist is
# stamped from it at build time, so a release cannot ship a disk image and a bundle that disagree
# about what they are.

despatch_version() {
    local project_root="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
    awk -F'"' '/^version = /{print $2; exit}' "$project_root/apps/despatch/Cargo.toml"
}
