"""Visual/interaction regression for --scrollbars on the test's owned Xvfb."""
import ctypes as c
import json
import re
import socket
import subprocess as sp
import time
from PIL import Image


def probe(x, xt, display, window, root, profile, command):
    x.XResizeWindow(display, window, 1180, 760)
    x.XFlush(display)
    time.sleep(.6)
    geometry = sp.run(['xwininfo', '-id', hex(window)], capture_output=True, text=True, check=True).stdout
    ox = int(re.search(r'Absolute upper-left X:\s+(-?\d+)', geometry)[1])
    oy = int(re.search(r'Absolute upper-left Y:\s+(-?\d+)', geometry)[1])
    xt.XTestFakeMotionEvent.argtypes = [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_ulong]
    xt.XTestFakeButtonEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
    def move(px, py):
        xt.XTestFakeMotionEvent(display, -1, px + ox, py + oy, 0)
        x.XFlush(display)
    def button(down, number=1):
        xt.XTestFakeButtonEvent(display, number, down, 0)
        x.XFlush(display)
    def click(px, py):
        move(px, py); button(1); button(0); time.sleep(.25)
    def drag(px, start, end):
        move(px, start); time.sleep(.15); button(1); time.sleep(.1)
        for i in range(1, 16):
            move(px, round(start + (end - start) * i / 15)); time.sleep(.025)
        time.sleep(.1); button(0); time.sleep(.6)
    def shot(name):
        path = root / name
        sp.run(['import', '-window', hex(window), str(path)], check=True)
        return Image.open(path).convert('RGB')
    def rpc(method):
        with socket.socket(socket.AF_UNIX) as sock:
            sock.settimeout(3); sock.connect(str(profile / 'control.sock'))
            sock.sendall((json.dumps({'id': 1, 'method': method})+'\n').encode())
            response = json.loads(sock.makefile().readline())
            assert 'error' not in response, response
            return response['result']
    def rail(image, right, top):
        # The rail is 6px and has its own 16px gutter, with empty space before it.
        colors = [image.getpixel((px, top + 12)) for px in range(right - 16, right)]
        assert 1 <= colors.count((38, 38, 47)) <= 6, ('thumb not thin/theme-colored', colors)
        assert image.getpixel((right - 12, top + 12)) == (11, 11, 15), 'rail overlaps content'

    command('library'); time.sleep(1)
    top = shot('library-scroll-top.png')
    cover_y = next(y for y in range(180, 300) if top.getpixel((320, y))[0] > 180 and top.getpixel((320, y))[1] < 100)
    rail(top, 1152, cover_y - 4)
    # Move beyond the rail end so reaching the bottom does not depend on
    # which fraction of the thumb was grabbed by the event loop.
    drag(1147, cover_y + 8, 675)
    bottom = shot('library-scroll-bottom.png')
    click(400, 632)
    status = rpc('status')
    assert status['track']['path'].endswith('Track 147.wav'), status
    # Fixtures use zero volume, and pausing avoids the natural end during QA.
    code = x.XKeysymToKeycode(display, ord(' '))
    xt.XTestFakeKeyEvent(display, code, 1, 0); xt.XTestFakeKeyEvent(display, code, 0, 0); x.XFlush(display)
    time.sleep(.25)
    move(800, 350)
    for _ in range(8): button(1, 4); button(0, 4)
    time.sleep(.6)
    wheel = shot('library-scroll-wheel.png')
    assert bottom.crop((340, 260, 900, 620)).tobytes() != wheel.crop((340, 260, 900, 620)).tobytes(), 'wheel did not update virtual rows'

    command('artists'); time.sleep(.7)
    artists = shot('artists-scroll-top.png')
    rail(artists, 468, 82)
    count_pixels = sum(1 for yy in range(94, 112) for xx in range(432, 450) if max(artists.getpixel((xx, yy))) > 75)
    assert count_pixels > 2, 'artist count is hidden beneath the thumb'
    drag(463, 100, 675)
    click(340, 632)
    selected = shot('artists-scroll-bottom.png')
    ys = [yy for yy in range(82, 220) for xx in range(500, 550) if all(abs(a-b) < 3 for a, b in zip(selected.getpixel((xx, yy)), (124, 92, 255)))]
    assert ys, 'selected artist has no play button'
    click(520, (min(ys)+max(ys))//2)
    queue = rpc('queue')
    assert queue['length'] == 1 and queue['items'][0]['path'].endswith('Track 147.wav'), queue
    print('PASS: 148 tracks/artists; 6px themed rails; counts/actions outside gutter; thumb drag and wheel reach real rows', flush=True)
