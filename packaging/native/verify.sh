#!/usr/bin/env bash
# Own test profiles and displays only; no installed profiles, accounts or Git writes.
set -euo pipefail
repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd -- "$repo_dir"
verify_gpu=0
verify_loopback=0
for option in "$@"; do
  case "$option" in
    --gpu) verify_gpu=1 ;;
    --loopback) verify_loopback=1 ;;
    *) echo 'Usage: verify.sh [--gpu] [--loopback]' >&2; exit 2 ;;
  esac
done
cargo fmt --all --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 -m unittest discover -s crates/muzon-core/tests -p 'test_*.py'
python3 packaging/native/test_launcher.py
python3 packaging/native/test_import_profile.py
cargo build --locked -p muzon-native --bins --examples
target_dir=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
python3 crates/muzon-native/tests/window_smoke.py "$target_dir/debug/muzon-native" --scrollbars
python3 crates/muzon-native/tests/window_smoke.py "$target_dir/debug/muzon-native" --tray
python3 crates/muzon-native/tests/wayland_smoke.py "$target_dir/debug/muzon-native"
if (( verify_gpu )); then
  python3 crates/muzon-native/tests/wayland_smoke.py "$target_dir/debug/examples/render_probe" --gpu
fi
if (( verify_loopback )); then
  # Exit 77 from the probe remains nonzero: absent hardware is never a PASS.
  python3 crates/muzon-core/tests/loopback_pcm.py
fi
printf '%s\n' 'PASS: requested native acceptance checks (live accounts/physical displays/suspend are separate)'
