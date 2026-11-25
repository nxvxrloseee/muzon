"""Real KSNI client integration on window_smoke's private Xvfb/D-Bus only."""
import ctypes as c
import json
import subprocess as sp
import time
import dbus


def wait(check, process, message, seconds=12):
    deadline = time.monotonic() + seconds
    while not check():
        assert process.poll() is None, f'{message}: app exited {process.returncode}'
        assert time.monotonic() < deadline, message
        time.sleep(.1)


def item_for(process, root):
    bus = dbus.SessionBus()
    path = root / 'tray-items.json'
    def find():
        for item in json.loads(path.read_text()) if path.exists() else []:
            service, object_path = item.split('/', 1)
            try:
                obj = bus.get_object(service, '/' + object_path)
                props = dbus.Interface(obj, 'org.freedesktop.DBus.Properties')
                if str(props.Get('org.kde.StatusNotifierItem', 'Id')) == f'muzon_native_{process.pid}':
                    return obj
            except dbus.DBusException:
                continue
        return None
    wait(lambda: find() is not None, process, 'tray failed to register with host')
    return bus, find()


def mapped(window):
    info = sp.run(['xwininfo', '-id', hex(window)], capture_output=True, text=True)
    return info.returncode == 0 and 'Map State: IsViewable' in info.stdout


def close_request(x, display, window):
    class Data(c.Union):
        _fields_ = [('b', c.c_char * 20), ('s', c.c_short * 10), ('l', c.c_long * 5)]
    class ClientMessage(c.Structure):
        _fields_ = [('type', c.c_int), ('serial', c.c_ulong), ('send_event', c.c_int),
                    ('display', c.c_void_p), ('window', c.c_ulong), ('message_type', c.c_ulong),
                    ('format', c.c_int), ('data', Data)]
    class Event(c.Union):
        _fields_ = [('client', ClientMessage), ('pad', c.c_long * 24)]
    x.XInternAtom.argtypes = [c.c_void_p, c.c_char_p, c.c_int]
    x.XInternAtom.restype = c.c_ulong
    x.XSendEvent.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_long, c.POINTER(Event)]
    event = Event()
    event.client.type = 33
    event.client.display = display
    event.client.window = window
    event.client.message_type = x.XInternAtom(display, b'WM_PROTOCOLS', 0)
    event.client.format = 32
    event.client.data.l[0] = x.XInternAtom(display, b'WM_DELETE_WINDOW', 0)
    assert x.XSendEvent(display, window, 0, 0, c.byref(event))
    x.XFlush(display)


def exercise(x, display, window, root, process, command):
    bus, item = item_for(process, root)
    player = bus.get_object('org.mpris.MediaPlayer2.muzon.test.tray', '/org/mpris/MediaPlayer2')
    props = dbus.Interface(player, 'org.freedesktop.DBus.Properties')
    # Health must have observed registration before close-to-tray becomes enabled.
    time.sleep(1)
    command('fixture')
    wait(lambda: str(props.Get('org.mpris.MediaPlayer2.Player', 'PlaybackStatus')) == 'Playing', process, 'fixture did not start')
    wait(lambda: int(props.Get('org.mpris.MediaPlayer2.Player', 'Position')) > 250000, process, 'playhead did not advance')
    before = int(props.Get('org.mpris.MediaPlayer2.Player', 'Position'))
    close_request(x, display, window)
    wait(lambda: not mapped(window), process, 'close did not hide window')
    time.sleep(.4)
    assert str(props.Get('org.mpris.MediaPlayer2.Player', 'PlaybackStatus')) == 'Playing'
    assert int(props.Get('org.mpris.MediaPlayer2.Player', 'Position')) > before
    dbus.Interface(player, 'org.mpris.MediaPlayer2').Raise()
    wait(lambda: mapped(window), process, 'MPRIS Raise did not restore window')
    close_request(x, display, window)
    wait(lambda: not mapped(window), process, 'second close did not hide window')
    dbus.Interface(item, 'org.kde.StatusNotifierItem').Activate(dbus.Int32(0), dbus.Int32(0))
    wait(lambda: mapped(window), process, 'tray activation did not restore window')
    (root / 'tray-exercised').touch()
    print('PASS: real KSNI registration, close-to-tray keeps playing, MPRIS Raise and tray Activate restore window', flush=True)


def quit_by_menu(process, root, reopened):
    bus, item = item_for(process, root)
    props = dbus.Interface(item, 'org.freedesktop.DBus.Properties')
    if reopened:
        player = bus.get_object('org.mpris.MediaPlayer2.muzon.test.tray', '/org/mpris/MediaPlayer2')
        playback = dbus.Interface(player, 'org.freedesktop.DBus.Properties')
        assert str(playback.Get('org.mpris.MediaPlayer2.Player', 'PlaybackStatus')) == 'Paused'
        assert int(playback.Get('org.mpris.MediaPlayer2.Player', 'Position')) > 0
    path = props.Get('org.kde.StatusNotifierItem', 'Menu')
    menu = dbus.Interface(bus.get_object(item.bus_name, path), 'com.canonical.dbusmenu')
    _, layout = menu.GetLayout(dbus.Int32(0), dbus.Int32(-1), dbus.Array([], signature='s'))
    def find(node):
        item_id, values, children = node
        if str(values.get('label', '')) == 'Выйти':
            return int(item_id)
        for child in children:
            found = find(child)
            if found is not None:
                return found
        return None
    quit_id = find(layout)
    assert quit_id is not None, 'tray quit item not found'
    menu.Event(dbus.Int32(quit_id), 'clicked', dbus.Int32(0), dbus.UInt32(0))
    assert process.wait(timeout=20) == 0, 'tray quit did not finish cleanly'
    if reopened:
        print('PASS: tray menu quits and session reopens paused at saved position', flush=True)
