# Arch package: Muzon Native

This recipe builds `muzon-native` from a pinned, published GitHub source
snapshot. It does not build React/Tauri, use npm, install the old `muzon`
command or replace its desktop entry. The two packages can coexist.

```bash
cd ~/Documents/projects/Muzon/packaging/aur-native
makepkg -si
muzon-native
```

`makepkg -si` asks pacman to install missing build dependencies and then the
finished package. To build **without installing**, use `makepkg` instead.
The output is `muzon-native-*.pkg.tar.zst` in this directory (or `$PKGDEST`).
Do not run makepkg as root.

The launcher defaults to a separate `$XDG_DATA_HOME/muzon-native` profile,
normally `~/.local/share/muzon-native`. An explicit `--profile DIR` works too.
The installed Tauri profile is never selected automatically.

## Contents

- `/usr/bin/muzon-native`: launcher;
- `/usr/lib/muzon-native/muzon-native`: executable;
- `/usr/share/applications/muzon-native.desktop`: menu entry;
- `/usr/share/icons/hicolor/{128x128,256x256}/apps/muzon-native.png`: icons;
- `/usr/share/muzon-native/tools/import_profile.py`: explicit profile importer.

To import an old profile, close the old player and specify its actual paths:

```bash
python /usr/share/muzon-native/tools/import_profile.py \
  --data /path/to/old/data --config /path/to/old/config \
  --destination /path/to/new/native-profile
muzon-native --profile /path/to/new/native-profile
```

Credentials/account queues are not imported. Runtime integrations use optional
packages: `libsecret` for account credentials, a desktop portal FileChooser
backend (GTK/KDE or another one supplied by the desktop) for folder selection,
and `zenity` as fallback. The portal frontend is a required dependency.
The user chooses a GPU driver appropriate for their hardware.

## Source version and checks

The package version `0.1.0.r32.gef59594` identifies a native-player snapshot,
not a new Tauri release. `_commit`, the source URL and SHA-256 pin the precise
source; editing files in the checkout does not change what makepkg builds.
To package a newer pushed commit, update `_commit`, `pkgver` and checksums,
then regenerate `.SRCINFO`:

```bash
updpkgsums
makepkg --printsrcinfo > .SRCINFO
```

All four standalone crates are fetched using their lockfiles in `prepare()`;
`build()` and `check()` run offline with `--frozen`. Checks cover the Rust
crates, portable launcher and profile importer. Full window/portal smoke tests
are a separate QA step because they require Xvfb and a private D-Bus session.
GCC LTO is disabled for compatibility with bundled SQLite and Rust's linker.

`native-app-id.patch` sets the Linux application ID to `muzon-native` in both
real and demo modes, makes the desktop entry match it and updates CLI help
to the installed command name. It is applied only
inside makepkg's extracted source, not to the checkout.

Packaging conventions:
[Arch Rust guidelines](https://wiki.archlinux.org/title/Rust_package_guidelines),
[Arch package guidelines](https://wiki.archlinux.org/title/Arch_package_guidelines).

After building, inspect the artifact without installing it:

```bash
python test_package.py ./muzon-native-*.pkg.tar.zst
namcap PKGBUILD
namcap ./muzon-native-*.pkg.tar.zst
```

The Python check unpacks the artifact into a temporary directory, runs only
`--help`, validates desktop metadata and verifies the packaged launcher using
a temporary stub. It never opens an installed music library. Static dependency
analysis cannot see the X11/Wayland/Vulkan/OpenGL/portal libraries loaded at
runtime; those are explicitly declared in the recipe.

Expected diagnostics: Python is optional because only the importer needs it;
GStreamer plugins and graphics/portal backends are runtime-loaded dependencies.
makepkg may report embedded Rust source paths in panic-location strings; these
are diagnostic metadata, not files the installed application needs to open.

Package revision 2 also applies `native-scrollbars.patch`: one theme-aware,
6px embedded rail with a reserved gutter across the native UI. The patch is
kept separately until a newer pinned upstream snapshot includes the fix.
