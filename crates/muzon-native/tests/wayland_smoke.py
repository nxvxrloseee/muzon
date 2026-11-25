#!/usr/bin/env python3
"""Own Weston headless compositor and private D-Bus; never touch desktop/accounts.
Usage: python3 tests/wayland_smoke.py /absolute/path/to/muzon-native
Requires weston, dbus-run-session, gdbus. Checks Wayland protocol + 1x/2x
startup at minimum window size, control.sock and graceful MPRIS Quit.
--gpu requires the render_probe example, wgpu hardware adapter and buffer commits.
This is software/headless validation, not physical GPU/suspend validation.
"""
import json
import os
from pathlib import Path
import socket
import subprocess as sp
import sys
import tempfile
import time


def wait_until(check, process, seconds=40):
    deadline = time.monotonic() + seconds
    while not check():
        if process.poll() is not None:
            raise AssertionError(f'process exited early: {process.returncode}')
        if time.monotonic() >= deadline:
            raise AssertionError('startup deadline expired')
        time.sleep(.1)


def rpc(path):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(3)
        sock.connect(str(path))
        sock.sendall(b'{"id":1,"method":"status"}\n')
        value = json.loads(sock.makefile().readline())
        assert 'result' in value, value


def worker(binary, root):
    root = Path(root)
    runtime = root / 'runtime'
    runtime.mkdir(mode=0o700)
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), MUZON_TRAY='0',
               WINIT_UNIX_BACKEND='wayland', WAYLAND_DISPLAY='muzon-test',
               XDG_DATA_HOME=str(root/'data'), XDG_CONFIG_HOME=str(root/'config'),
               XDG_CACHE_HOME=str(root/'cache'), GIO_USE_VFS='local')
    env.pop('DISPLAY', None)
    gpu = os.environ.get('MUZON_SMOKE_GPU') == '1'
    if gpu:
        env.update(ICED_BACKEND='wgpu', WGPU_BACKEND='vulkan', WAYLAND_DEBUG='client')
    for scale in (1, 2):
        profile = root / f'profile-{scale}'
        (profile/'config').mkdir(parents=True)
        (profile/'config/window.json').write_text(json.dumps({'width':800,'height':480,'maximized':False}))
        env['MUZON_MPRIS_NAME'] = f'muzon.test.scale{scale}'
        with (root/f'weston-{scale}.log').open('w') as log:
            compositor = sp.Popen(['weston', '--backend=headless', ('--renderer=gl' if gpu else '--renderer=pixman'), '--no-config',
                                   '--socket=muzon-test', '--idle-time=0', '--width=2000',
                                   '--height=1200', f'--scale={scale}'], env=env, stdout=log, stderr=log)
            try:
                wait_until(lambda: (runtime/'muzon-test').exists(), compositor)
                with (root/f'native-{scale}.log').open('w') as app_log:
                    app = sp.Popen([binary,'--profile',str(profile)], env=env, stdout=app_log, stderr=app_log)
                    try:
                        wait_until(lambda: (profile/'control.sock').exists(), app)
                        rpc(profile/'control.sock')
                        bus = 'org.mpris.MediaPlayer2.' + env['MUZON_MPRIS_NAME']
                        def mpris_ready():
                            result = sp.run(['gdbus','call','--session','--dest',bus,'--object-path',
                                             '/org/mpris/MediaPlayer2','--method',
                                             'org.freedesktop.DBus.Peer.Ping'],env=env,capture_output=True,timeout=5)
                            return result.returncode == 0
                        wait_until(mpris_ready, app)
                        time.sleep(2)
                        assert app.poll() is None
                        if gpu:
                            text = (root/f'native-{scale}.log').read_text()
                            import re
                            selected = re.search(r'Selected: AdapterInfo \{(.*?)\}', text, re.S)
                            assert selected, 'no selected wgpu adapter diagnostics'
                            info = selected[1]
                            assert 'device_type: Cpu' not in info and not any(token in info.lower() for token in ('llvmpipe', 'lavapipe', 'softpipe')), info
                            assert 'backend: Vulkan' in info, info
                            # Match the actual Muzon toplevel, not a cursor or winit's
                            # temporary adapter-selection surface.
                            title = re.search(r'(xdg_toplevel[#@]\d+)\.set_title\("Muzon"\)', text)
                            assert title, 'Muzon toplevel missing from protocol trace'
                            xdg = re.search(r'(xdg_surface[#@]\d+)\.get_toplevel\(new id ' + re.escape(title[1]) + r'\)', text)
                            assert xdg, 'Muzon xdg_surface missing'
                            surface = re.search(r'get_xdg_surface\(new id ' + re.escape(xdg[1]) + r', (wl_surface[#@]\d+)\)', text)
                            assert surface, 'Muzon wl_surface missing'
                            actual = re.escape(surface[1])
                            assert re.search(actual + r'\.attach\(wl_buffer[#@]\d+.*?' + actual + r'\.commit\(', text, re.S), 'Muzon window did not commit a rendered buffer'
                            name = re.search(r'name: "([^"]+)"',info)
                            print('GPU:',name[1] if name else info,flush=True)
                        sp.run(['gdbus','call','--session','--dest',bus,'--object-path',
                                '/org/mpris/MediaPlayer2','--method','org.mpris.MediaPlayer2.Quit'],
                               env=env,check=True,capture_output=True,timeout=5)
                        assert app.wait(timeout=20) == 0
                        assert not (profile/'control.sock').exists(), 'stale control socket after quit'
                    finally:
                        if app.poll() is None:
                            app.terminate()
                            app.wait(timeout=20)
            finally:
                compositor.terminate()
                compositor.wait(timeout=10)
        print(f'PASS: {"hardware wgpu + Wayland commits" if gpu else "headless Wayland"} scale {scale}, 800x480, control and MPRIS quit',flush=True)


if __name__ == '__main__':
    if len(sys.argv)==4 and sys.argv[1]=='--worker':
        worker(sys.argv[2],sys.argv[3])
    elif len(sys.argv) in (2,3) and (len(sys.argv)==2 or sys.argv[2]=='--gpu'):
        binary = str(Path(sys.argv[1]).resolve(strict=True))
        root = tempfile.mkdtemp(prefix='muzon-wayland-smoke-')
        print('Scratch:',root,flush=True)
        env = dict(os.environ)
        env['MUZON_SMOKE_GPU'] = '1' if len(sys.argv)==3 else '0'
        raise SystemExit(sp.run(['dbus-run-session','--',sys.executable,__file__,'--worker',binary,root],env=env).returncode)
    else:
        raise SystemExit(__doc__)
