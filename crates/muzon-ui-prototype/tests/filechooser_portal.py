"""FileChooser test double; used only on window_smoke.py's private D-Bus."""
import dbus, dbus.service, dbus.mainloop.glib, json, sys
from pathlib import Path
from gi.repository import GLib
root=Path(sys.argv[1]); chosen=Path(sys.argv[2]); requests=[];objects=[]
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus=dbus.SessionBus();name=dbus.service.BusName('org.freedesktop.portal.Desktop',bus)
class Request(dbus.service.Object):
 @dbus.service.signal('org.freedesktop.portal.Request',signature='ua{sv}')
 def Response(self,code,results):pass
class Portal(dbus.service.Object):
 @dbus.service.method('org.freedesktop.portal.FileChooser',in_signature='ssa{sv}',out_signature='o',sender_keyword='sender')
 def OpenFile(self,parent,title,options,sender=None):
  requests.append({'title':str(title),'directory':bool(options.get('directory')),'multiple':bool(options.get('multiple'))});(root/'portal-requests.json').write_text(json.dumps(requests))
  path='/org/freedesktop/portal/desktop/request/'+sender.lstrip(':').replace('.','_')+'/'+str(options['handle_token'])
  request=Request(bus,path);objects.append(request)
  code=0 if len(requests)==2 else 1
  def answer():
   results=dbus.Dictionary({'uris':dbus.Array([chosen.as_uri()],signature='s')} if code==0 else {},signature='sv');request.Response(dbus.UInt32(code),results);return False
  GLib.timeout_add(250,answer)
  return dbus.ObjectPath(path)
p=Portal(bus,'/org/freedesktop/portal/desktop');(root/'portal-ready').touch();GLib.MainLoop().run()
