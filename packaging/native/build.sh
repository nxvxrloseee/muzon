#!/usr/bin/env bash
# Build a portable Linux bundle without installing or changing git state.
set -euo pipefail
repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
mode=${1:-release}
case "$mode" in release|debug) ;; *) echo 'Usage: build.sh [release|debug] [output-directory]' >&2; exit 2;; esac
output_dir=${2:-"$repo_dir/dist/native"}
mkdir -p -- "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
manifest="$repo_dir/crates/muzon-ui-prototype/Cargo.toml"
args=(build --locked --manifest-path "$manifest")
if [[ "$mode" == release ]]; then args+=(--release); fi
cargo "${args[@]}"
# cargo metadata respects CARGO_TARGET_DIR and the standalone manifest.
target_dir=$(cargo metadata --no-deps --format-version 1 --manifest-path "$manifest" | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
bundle_name="muzon-native-$(uname -m)-linux-$mode"
stage_dir=$(mktemp -d "${TMPDIR:-/tmp}/muzon-native-package.XXXXXXXX")
trap 'rm -rf -- "$stage_dir"' EXIT
bundle_dir="$stage_dir/$bundle_name"
mkdir -p -- "$bundle_dir/bin" "$bundle_dir/libexec" "$bundle_dir/share/applications" "$bundle_dir/share/icons/hicolor/256x256/apps" "$bundle_dir/tools"
install -m755 -- "$target_dir/$mode/muzon-ui-prototype" "$bundle_dir/libexec/muzon-native"
install -m755 -- "$repo_dir/packaging/native/muzon-native" "$bundle_dir/bin/muzon-native"
install -m644 -- "$repo_dir/packaging/native/muzon-native.desktop" "$bundle_dir/share/applications/"
install -m644 -- "$repo_dir/src-tauri/icons/128x128@2x.png" "$bundle_dir/share/icons/hicolor/256x256/apps/muzon-native.png"
install -m644 -- "$repo_dir/packaging/native/import_profile.py" "$bundle_dir/tools/import_profile.py"
install -m644 -- "$repo_dir/packaging/native/README.md" "$bundle_dir/README.md"
if [[ -f "$repo_dir/LICENSE" ]]; then install -m644 -- "$repo_dir/LICENSE" "$bundle_dir/LICENSE"; fi
archive_path="$output_dir/$bundle_name.tar.gz"
temporary_path=$(mktemp "$output_dir/.muzon-native.XXXXXXXX.tar.gz")
trap 'rm -rf -- "$stage_dir"; rm -f -- "$temporary_path"' EXIT
tar -czf "$temporary_path" -C "$stage_dir" "$bundle_name"
chmod 644 -- "$temporary_path"
mv -- "$temporary_path" "$archive_path"
(cd -- "$output_dir" && sha256sum "$bundle_name.tar.gz" > "$bundle_name.tar.gz.sha256")
printf 'Bundle: %s\n' "$archive_path"
