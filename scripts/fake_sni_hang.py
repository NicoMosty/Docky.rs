#!/usr/bin/env python3
"""SNI falso que NUNCA contesta `GetLayout`: es la "app del tray colgada" de la
verificacion de A3 de AUDIT.md.

Registra un StatusNotifierItem (con `Menu` apuntando a un dbusmenu) en el watcher
del dock, y su `GetLayout` usa `async_callbacks` que nunca se llaman: asi la
llamada del cliente queda colgada pero este loop SIGUE vivo, que es lo que permite
recibir despues el fallback `Activate`.

Imprime con marca de tiempo cada evento, que es el sensor:
  GETLAYOUT ...   el dock pidio el menu (y se va a colgar hasta el method_timeout)
  ACTIVATE  ...   el dock cayo al fallback porque el menu vino vacio

Uso: python3 scripts/fake_sni_hang.py [--name org.dockyrs.FakeSNI] [--path /FakeSNI]
"""
import argparse
import time

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

ITEM_IFACE = "org.kde.StatusNotifierItem"
MENU_IFACE = "com.canonical.dbusmenu"
WATCHER_IFACE = "org.kde.StatusNotifierWatcher"
WATCHER_PATH = "/StatusNotifierWatcher"

T0 = time.time()


def t():
    return time.time() - T0


class Item(dbus.service.Object):
    def __init__(self, bus, path, menu_path):
        super().__init__(bus, path)
        self.menu_path = menu_path

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="ss", out_signature="v")
    def Get(self, iface, prop):
        return self.GetAll(iface)[prop]

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="s", out_signature="a{sv}")
    def GetAll(self, _iface):
        return {
            "IconName": "fake-hang",
            "Menu": dbus.ObjectPath(self.menu_path),
            "Status": "Active",
            "Category": "ApplicationStatus",
            "Id": "fake-hang",
            "Title": "Fake hang",
            "ItemIsMenu": True,
        }

    @dbus.service.method(ITEM_IFACE, in_signature="ii", out_signature="")
    def Activate(self, _x, _y):
        print(f"ACTIVATE  t={t():.3f}s  (fallback: el menu volvio vacio)", flush=True)

    @dbus.service.method(ITEM_IFACE, in_signature="ii", out_signature="")
    def SecondaryActivate(self, _x, _y):
        pass

    @dbus.service.method(ITEM_IFACE, in_signature="ay", out_signature="")
    def NewIcon(self, _data):
        pass


class Menu(dbus.service.Object):
    def __init__(self, bus, path):
        super().__init__(bus, path)

    @dbus.service.method(MENU_IFACE, in_signature="isvu", out_signature="")
    def Event(self, _id, event_id, _data, _timestamp):
        print(f"EVENT     t={t():.3f}s {event_id}", flush=True)

    @dbus.service.method(
        MENU_IFACE,
        in_signature="iias",
        out_signature="u(ia{sv}av)",
        async_callbacks=("reply", "error"),
    )
    def GetLayout(self, parent, depth, names, reply, error):
        # ----- ojo: `reply`/`error` tienen que llamarse ASI (python-dbus los busca
        # por nombre) y NO se llaman nunca a proposito: la llamada del cliente queda
        # colgada hasta su propio timeout mientras este loop sigue atendiendo lo
        # demas, que es lo que permite recibir despues el fallback `Activate`. -----
        _ = (parent, depth, names, reply, error)
        print(f"GETLAYOUT t={t():.3f}s  (no contesto nunca)", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--name", default="org.dockyrs.FakeSNI")
    ap.add_argument("--path", default="/FakeSNI")
    args = ap.parse_args()

    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    # ----- hay que GUARDAR la referencia: `BusName` libera el nombre al recolectarse,
    # y sin esto el dock no puede resolver el item (el watcher lo tiene en su lista
    # porque guarda el string que le mandamos, pero el nombre no existe en el bus).
    # Se ve como "existe el item" y despues no aparece en el tray. -----
    nombre = dbus.service.BusName(args.name, bus)
    item = Item(bus, args.path, args.path + "/Menu")
    menu = Menu(bus, args.path + "/Menu")
    watcher = bus.get_object(WATCHER_IFACE, WATCHER_PATH)
    watcher.RegisterStatusNotifierItem(
        args.name + args.path, dbus_interface=WATCHER_IFACE
    )
    print(f"REGISTRADO t={t():.3f}s  {args.name}{args.path} -> {args.path}/Menu", flush=True)
    keep_alive = (nombre, item, menu)
    _ = keep_alive
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
