# Muzon Native — Linux bundle

Run `bin/muzon-native` from the extracted directory. With no arguments the
launcher creates a separate profile at `$XDG_DATA_HOME/muzon-native` (normally
`~/.local/share/muzon-native`). It never discovers the installed Tauri profile.
Pass `--profile /absolute/path` to choose another profile, or `--demo` for a
synthetic library. The underlying binary still requires an explicit mode.
Do not run both shells against the same database.

This is a dynamically linked Linux bundle, not an AppImage. The machine needs
GStreamer and decoding plugins, the renderer's Linux windowing libraries and
a session D-Bus. Native folder selection uses XDG Desktop Portal with a
FileChooser backend and libdbus; Zenity is the fallback. `secret-tool` (libsecret) is required only when connecting
S3/ListenBrainz/Last.fm accounts. Credentials are namespaced by the canonical
profile directory; moving the profile requires reconnecting accounts.

Nothing is installed by this archive. To add it to a launcher, put its `bin`
directory on PATH, copy `share/applications/muzon-native.desktop` into your
user applications directory and copy its icon to your user icon directory.
Do not remove the adjacent `libexec` directory. The launcher and executable
must remain inside the same extracted bundle.

From a source checkout, build with:

```sh
./packaging/native/build.sh release
# Smaller verification cycle, larger executable:
./packaging/native/build.sh debug /tmp/muzon-native-bundles
```

The script builds using Cargo.lock, writes a tar.gz and SHA-256 checksum,
and never installs, publishes, modifies the profile or performs git operations.
Build on the oldest Linux distribution you intend to support: a build from
newer glibc is not guaranteed to run on older systems. `debug` is for testing;
use `release` when distributing the application.

## Import an existing library explicitly

Close the old player first so its session/config files are fully saved. Identify
its data and config directories yourself; the tool never discovers them:

```sh
python3 tools/import_profile.py --data /path/to/old/data \
  --config /path/to/old/config --destination /path/to/new/native-profile
bin/muzon-native --profile /path/to/new/native-profile
```

From a source checkout use `packaging/native/import_profile.py` instead.
The destination must not exist. SQLite backup includes committed WAL data and
preserves IDs, history, favourites and playlists. Session, playback settings,
hotkeys and the saved theme are copied; music remains at its original paths.
The source SQLite connection is read-only; normal SQLite WAL/SHM locking may
still occur. Account configs, service queues, credentials, cache and window state
are not copied. A failed import removes only its newly created destination.
Reconnect accounts manually. Do not point an old and new shell at one profile.

Verification from the checkout:

```sh
python3 packaging/native/test_launcher.py
python3 packaging/native/test_import_profile.py
python3 crates/muzon-native/tests/window_smoke.py \
  target/release/muzon-native
```

The window test needs Python dbus and PyGObject (gi). It creates its own Xvfb,
private D-Bus, mock FileChooser portal and scratch profile with synthetic music.
It verifies folder selection/cancel, scanning a path with spaces, thumbnail
loading, settings categories, command palette, second-launch activation, window
persistence and clean shutdown/reopen, and rejects unexpected credential access.
It does not measure GPU performance or test authenticated cloud accounts.

## Arch package

A separate [PKGBUILD](../arch/PKGBUILD) builds and packages the current native
workspace with makepkg:

```sh
cd packaging/arch
makepkg -si
```

See [the Arch package instructions](../arch/README.md). The package is
named `muzon-native` and can coexist with the Tauri package `muzon`.

## Complete local acceptance command

From the source checkout, `./packaging/native/verify.sh` checks Rust, Python,
virtualized lists, real KSNI tray integration and private Wayland. Add `--gpu`
for strict Vulkan hardware-renderer verification (software adapters fail), or
`--loopback` for an already-loaded snd-aloop card and byte-level PCM comparison.
A missing Loopback exits 77; it is not a passing audio verification. The runner
never loads modules, suspends the machine, changes display settings, accesses
installed profiles/account credentials, or performs Git operations.
