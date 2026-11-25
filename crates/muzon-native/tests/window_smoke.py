#!/usr/bin/env python3
"""Owned Xvfb/private D-Bus only. No installed profiles or real keyring; playback fixtures are silent.
Usage: python3 tests/window_smoke.py /absolute/path/to/native-binary [--scrollbars|--tray]
--scrollbars adds a 148-track visual test; requires ImageMagick and Pillow.
--tray uses a private StatusNotifier host to test the real tray client.
Needs: dbus-run-session, xvfb-run, xwininfo, libX11, libXtst, Python dbus/gi.
"""
import ctypes as c
import json
import os
from pathlib import Path
import re
import subprocess as sp
import sys
import tempfile
import sqlite3
import wave
import struct
import zlib
import time


def main():
    if len(sys.argv) not in (2, 3) or len(sys.argv) == 3 and sys.argv[2] not in ('--scrollbars', '--tray'):
        raise SystemExit(__doc__)
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    root = Path(tempfile.mkdtemp(prefix="muzon-repo-smoke-"))
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("DISPLAY", None)
    env.update(WINIT_UNIX_BACKEND="x11", MUZON_TRAY="0", GIO_USE_VFS="local",
               GST_REGISTRY=str(root / "gst-registry.bin"),
               XDG_CONFIG_HOME=str(root / "xdg-config"),
               XDG_DATA_HOME=str(root / "xdg-data"), XDG_CACHE_HOME=str(root / "cache"))
    fakebin = root / "fakebin"
    fakebin.mkdir()
    spy = root / "keyring-access"
    tool = fakebin / "secret-tool"
    tool.write_text('#!/bin/sh\nprintf attempted >> "$MUZON_TEST_KEYRING_SPY"\nexit 99\n')
    tool.chmod(0o755)
    env["PATH"] = str(fakebin) + os.pathsep + env["PATH"]
    env["MUZON_TEST_KEYRING_SPY"] = str(spy)
    if len(sys.argv) == 3 and sys.argv[2] == '--scrollbars':
        env['MUZON_SMOKE_SCROLLBARS'] = '1'
    else:
        env.pop('MUZON_SMOKE_SCROLLBARS', None)
    if len(sys.argv) == 3 and sys.argv[2] == '--tray':
        env['MUZON_SMOKE_TRAY'] = '1'
        env['MUZON_TRAY'] = '1'
        env['MUZON_MPRIS_NAME'] = 'muzon.test.tray'
    else:
        env.pop('MUZON_SMOKE_TRAY', None)
    result = sp.run(["dbus-run-session", "--", "xvfb-run", "-a", "-s",
                     "-screen 0 1400x900x24", sys.executable, __file__,
                     "--owned-worker", binary, str(root)], env=env)
    print("Scratch profile/logs:", root)
    raise SystemExit(result.returncode)


def _worker(binary, root):
    # Only main() creates this worker on its own fresh Xvfb; never select a desktop.
    root = Path(root)
    x = c.CDLL("libX11.so.6")
    xt = c.CDLL("libXtst.so.6")
    x.XOpenDisplay.argtypes = [c.c_char_p]
    x.XOpenDisplay.restype = c.c_void_p
    display = x.XOpenDisplay(None)
    assert display, "owned Xvfb unavailable"
    x.XFlush.argtypes = [c.c_void_p]
    x.XResizeWindow.argtypes = [c.c_void_p, c.c_ulong, c.c_uint, c.c_uint]
    x.XSetInputFocus.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_ulong]
    x.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
    x.XKeysymToKeycode.restype = c.c_uint
    xt.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]

    def key(symbol, down):
        xt.XTestFakeKeyEvent(display, x.XKeysymToKeycode(display, symbol), down, 0)
        x.XFlush(display)

    def press(symbol):
        key(symbol, 1)
        key(symbol, 0)

    profile = root / "profile"
    with (root / "native.log").open("w") as log:
        for iteration in range(2):
            process = sp.Popen([binary, "--profile", str(profile)], stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 40
                while True:
                    info = sp.run(["xwininfo", "-name", "Muzon"], capture_output=True, text=True)
                    if info.returncode == 0 and "Map State: IsViewable" in info.stdout:
                        break
                    assert process.poll() is None, "native exited before opening its window"
                    assert time.monotonic() < deadline, "window timeout"
                    time.sleep(.1)
                window = int(re.search(r"Window id: (0x[0-9a-f]+)", info.stdout)[1], 16)
                time.sleep(.5)
                x.XSetInputFocus(display, window, 1, 0)
                x.XFlush(display)
                time.sleep(1)
                if iteration == 0:
                    x.XResizeWindow(display, window, 1030, 670)
                    x.XFlush(display)
                    time.sleep(.5)
                    def command(query):
                        key(0xffe3, 1)
                        press(ord('k'))
                        key(0xffe3, 0)
                        time.sleep(.15)
                        for letter in query:
                            press(ord(letter))
                        time.sleep(.2)
                        press(0xff0d)
                        time.sleep(.4)
                    def count(table):
                        with sqlite3.connect(profile / 'muzon.sqlite3') as db:
                            return db.execute('SELECT count(*) FROM '+table).fetchone()[0]
                    command('folder')
                    time.sleep(.6)
                    assert count('roots') == 0, 'cancel changed library roots'
                    command('folder')
                    deadline = time.monotonic() + 15
                    while count('tracks') != (148 if os.environ.get('MUZON_SMOKE_SCROLLBARS') else 1):
                        assert time.monotonic() < deadline, 'chosen folder was not scanned'
                        time.sleep(.1)
                    assert count('roots') == 1
                    time.sleep(.8)
                    assert any((profile / 'covers/rows').glob('*')), 'visible row thumbnail was not loaded'
                    requests = json.loads((root / 'portal-requests.json').read_text())
                    assert len(requests) == 2 and all(r['directory'] and not r['multiple'] for r in requests)
                    for query in ["albums", "audio", "cloud", "scrobble", "appearance", "shortcuts", "neverplayed"]:
                        command(query)
                    if os.environ.get('MUZON_SMOKE_SCROLLBARS'):
                        from scrollbar_probe import probe
                        probe(x, xt, display, window, root, profile, command)
                        x.XResizeWindow(display, window, 800, 480)
                        x.XFlush(display)
                        time.sleep(.5)
                        command("audio")
                        time.sleep(.7)
                        sp.run(["import", "-window", hex(window), str(root / "minimum-audio.png")], check=True)
                        command("nowplaying")
                        sp.run(["import", "-window", hex(window), str(root / "minimum-now-playing.png")], check=True)
                    if os.environ.get('MUZON_SMOKE_TRAY'):
                        from tray_probe import exercise
                        exercise(x, display, window, root, process, command)
                    # Second invocation activates the first, without a second worker.
                    second = sp.run([binary, "--profile", str(profile)], stdout=log, stderr=log, timeout=10)
                    assert second.returncode == 0 and process.poll() is None
                if os.environ.get('MUZON_SMOKE_TRAY'):
                    from tray_probe import quit_by_menu
                    quit_by_menu(process, root, iteration == 1)
                    continue
                # Explicit native quit shortcut through the command palette.
                key(0xffe3, 1)
                press(ord('k'))
                key(0xffe3, 0)
                time.sleep(.2)
                for letter in "quit":
                    press(ord(letter))
                time.sleep(.3)
                press(0xff0d)
                assert process.wait(timeout=20) == 0, "unclean native exit"
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=20)
    assert not (root / "keyring-access").exists(), "disconnected UI accessed credentials"
    assert (profile / "muzon.sqlite3").is_file()
    settings = json.loads((profile / "config/window.json").read_text())
    assert settings["width"] >= 800 and settings["height"] >= 480
    print("PASS: native folder select/cancel, thumbnail cache, settings pages, palette, single instance, clean quit/reopen, no keyring access")


def worker(binary, root):
    root = Path(root)
    chosen = root / 'chosen music folder'
    chosen.mkdir()
    scrolling = bool(os.environ.get('MUZON_SMOKE_SCROLLBARS'))
    for i in range(148 if scrolling else 1):
        path = chosen / (f'Track {i:03}.wav' if scrolling else 'Fixture.wav')
        with wave.open(str(path), 'wb') as audio:
            audio.setnchannels(1); audio.setsampwidth(2); audio.setframerate(8000)
            audio.writeframes(b'\0' * (960000 if os.environ.get('MUZON_SMOKE_TRAY') else 48000))
        if scrolling:
            def info(kind, value):
                payload=value.encode()+b'\0'
                return kind+struct.pack('<I',len(payload))+payload+(b'\0' if len(payload)%2 else b'')
            metadata=b'INFO'+info(b'IART',f'Artist {i:03}')+info(b'INAM',f'Track {i:03}')+info(b'IPRD',f'Album {i//10:02}')
            raw=bytearray(path.read_bytes());raw.extend(b'LIST'+struct.pack('<I',len(metadata))+metadata)
            raw[4:8]=struct.pack('<I',len(raw)-8);path.write_bytes(raw)
    config=root/'profile/config';config.mkdir(parents=True)
    (config/'session.json').write_text(json.dumps({'queuePaths':[],'shuffleOrder':[],'cursor':-1,'shuffle':False,'repeat':'off','positionSecs':0,'volume':0}))
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 32, 32, 8, 2, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress((b'\0' + bytes([220, 60, 80]) * 32) * 32)) + chunk(b'IEND', b'')
    (chosen / 'cover.png').write_bytes(png)
    import contextlib
    with contextlib.ExitStack() as stack:
        if os.environ.get('MUZON_SMOKE_TRAY'):
            tray_log = stack.enter_context((root / 'tray-host.log').open('w'))
            host = sp.Popen([sys.executable, str(Path(__file__).with_name('status_notifier_host.py')), str(root)], stdout=tray_log, stderr=tray_log)
            def stop_host():
                host.terminate()
                host.wait(timeout=5)
            stack.callback(stop_host)
            deadline = time.monotonic() + 10
            while not (root / 'tray-host-ready').exists():
                assert host.poll() is None, 'tray test host exited'
                assert time.monotonic() < deadline, 'tray host startup timeout'
                time.sleep(.1)
        log = stack.enter_context((root / 'portal.log').open('w'))
        portal = sp.Popen([sys.executable, str(Path(__file__).with_name('filechooser_portal.py')), str(root), str(chosen)], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 10
            while not (root / 'portal-ready').exists():
                assert portal.poll() is None, 'test portal exited'
                assert time.monotonic() < deadline, 'test portal startup timeout'
                time.sleep(.1)
            _worker(binary, root)
        finally:
            portal.terminate(); portal.wait(timeout=5)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--owned-worker":
        worker(sys.argv[2], sys.argv[3])
    else:
        main()
