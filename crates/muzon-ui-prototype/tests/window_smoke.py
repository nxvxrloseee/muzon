#!/usr/bin/env python3
"""Owned Xvfb/private D-Bus only. No installed profiles, audio or real keyring.
Usage: python3 tests/window_smoke.py /absolute/path/to/native-binary
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
    if len(sys.argv) != 2:
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
                    while count('tracks') != 1:
                        assert time.monotonic() < deadline, 'chosen folder was not scanned'
                        time.sleep(.1)
                    assert count('roots') == 1
                    time.sleep(.8)
                    assert any((profile / 'covers/rows').glob('*')), 'visible row thumbnail was not loaded'
                    requests = json.loads((root / 'portal-requests.json').read_text())
                    assert len(requests) == 2 and all(r['directory'] and not r['multiple'] for r in requests)
                    for query in ["albums", "cloud", "scrobble", "appearance", "shortcuts", "neverplayed"]:
                        command(query)
                    # Second invocation activates the first, without a second worker.
                    second = sp.run([binary, "--profile", str(profile)], stdout=log, stderr=log, timeout=10)
                    assert second.returncode == 0 and process.poll() is None
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
    with wave.open(str(chosen / 'Fixture.wav'), 'wb') as audio:
        audio.setnchannels(1); audio.setsampwidth(2); audio.setframerate(8000)
        audio.writeframes(b'\0' * 16000)
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 32, 32, 8, 2, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress((b'\0' + bytes([220, 60, 80]) * 32) * 32)) + chunk(b'IEND', b'')
    (chosen / 'cover.png').write_bytes(png)
    with (root / 'portal.log').open('w') as log:
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
