"""StatusNotifier test host, used exclusively on an owned private D-Bus."""
import json
import sys
from pathlib import Path
import dbus
import dbus.service
import dbus.mainloop.glib
from gi.repository import GLib

root = Path(sys.argv[1])
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
name = dbus.service.BusName('org.kde.StatusNotifierWatcher', bus)

class Watcher(dbus.service.Object):
    def __init__(self):
        super().__init__(bus, '/StatusNotifierWatcher')
        self.items = []

    @dbus.service.method('org.kde.StatusNotifierWatcher', in_signature='s', out_signature='', sender_keyword='sender')
    def RegisterStatusNotifierItem(self, service, sender=None):
        item = str(sender) + str(service) if service.startswith('/') else str(service) + '/StatusNotifierItem'
        if item not in self.items:
            self.items.append(item)
            self.StatusNotifierItemRegistered(item)
        temporary = root / 'tray-items.tmp'
        temporary.write_text(json.dumps(self.items))
        temporary.replace(root / 'tray-items.json')

    @dbus.service.method('org.kde.StatusNotifierWatcher', in_signature='s', out_signature='')
    def RegisterStatusNotifierHost(self, service):
        pass

    @dbus.service.signal('org.kde.StatusNotifierWatcher', signature='s')
    def StatusNotifierItemRegistered(self, item):
        pass

    @dbus.service.method('org.freedesktop.DBus.Properties', in_signature='ss', out_signature='v')
    def Get(self, interface, prop):
        return self.GetAll(interface)[prop]

    @dbus.service.method('org.freedesktop.DBus.Properties', in_signature='s', out_signature='a{sv}')
    def GetAll(self, interface):
        if interface != 'org.kde.StatusNotifierWatcher':
            return {}
        return {'RegisteredStatusNotifierItems': dbus.Array(self.items, signature='s'),
                'IsStatusNotifierHostRegistered': dbus.Boolean(True),
                'ProtocolVersion': dbus.Int32(0)}

watcher = Watcher()
(root / 'tray-host-ready').touch()
GLib.MainLoop().run()
