# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Evolution Data Server for the UI tests: one address book with a
contact of the group Clients, one of no group, and the person's own card,
named as the system names the person. It prints "ready" once it owns the
names of the server.
"""

from gi.repository import Gio, GLib

SOURCES = "org.gnome.evolution.dataserver.Sources5"
SOURCES_PATH = "/org/gnome/evolution/dataserver/SourceManager"
BOOKS = "org.gnome.evolution.dataserver.AddressBook10"
FACTORY_PATH = "/org/gnome/evolution/dataserver/AddressBookFactory"
BOOK_PATH = "/org/gnome/evolution/dataserver/Subprocess/Backend/AddressBook/1/1"
NODE = """
<node>
  <interface name="org.freedesktop.DBus.ObjectManager">
    <method name="GetManagedObjects">
      <arg type="a{oa{sa{sv}}}" name="objects" direction="out"/>
    </method>
  </interface>
  <interface name="org.gnome.evolution.dataserver.AddressBookFactory">
    <method name="OpenAddressBook">
      <arg type="s" name="source_uid" direction="in"/>
      <arg type="s" name="object_path" direction="out"/>
      <arg type="s" name="bus_name" direction="out"/>
    </method>
  </interface>
  <interface name="org.gnome.evolution.dataserver.AddressBook">
    <method name="Open">
      <arg type="as" name="properties" direction="out"/>
    </method>
    <method name="GetContactList">
      <arg type="s" name="query" direction="in"/>
      <arg type="as" name="vcards" direction="out"/>
    </method>
    <method name="Close"/>
  </interface>
</node>
"""
BOOK_DATA = "[Data Source]\nDisplayName=Personal\nEnabled=true\n\n[Address Book]\nBackendName=local\n"
CALENDAR_DATA = "[Data Source]\nDisplayName=Calendar\nEnabled=true\n\n[Calendar]\nBackendName=local\n"


def card(name, first, company, categories=None):
    lines = ["BEGIN:VCARD", "VERSION:3.0", f"FN:{name}", f"N:;{first};;;", f"ORG:{company}"]
    if categories:
        lines.append(f"CATEGORIES:{categories}")
    lines.append("END:VCARD")
    return "\r\n".join(lines) + "\r\n"


def main():
    loop = GLib.MainLoop()
    info = Gio.DBusNodeInfo.new_for_xml(NODE)
    own = GLib.get_real_name()
    cards = [
        card("Ana Souza", "Ana", "Clientes Ltda.", "Clients"),
        card("Bruno Lima", "Bruno", "Lima SA"),
        card(own, own.split()[0] if own.split() else own, "Desk & Co."),
    ]

    def call(connection, sender, path, interface, method, parameters, invocation):
        if method == "GetManagedObjects":
            objects = {
                f"{SOURCES_PATH}/Source_{uid.replace('-', '_')}": {
                    "org.gnome.evolution.dataserver.Source": {
                        "UID": GLib.Variant("s", uid),
                        "Data": GLib.Variant("s", data),
                    }
                }
                for uid, data in (("system-address-book", BOOK_DATA), ("calendar", CALENDAR_DATA))
            }
            invocation.return_value(GLib.Variant("(a{oa{sa{sv}}})", (objects,)))
        elif method == "OpenAddressBook":
            invocation.return_value(GLib.Variant("(ss)", (BOOK_PATH, connection.get_unique_name())))
        elif method == "Open":
            invocation.return_value(GLib.Variant("(as)", ([],)))
        elif method == "GetContactList":
            invocation.return_value(GLib.Variant("(as)", (cards,)))
        else:
            invocation.return_value(None)

    def acquired(connection, _name):
        connection.register_object(SOURCES_PATH, info.interfaces[0], call, None, None)
        connection.register_object(FACTORY_PATH, info.interfaces[1], call, None, None)
        connection.register_object(BOOK_PATH, info.interfaces[2], call, None, None)

    owned = []

    def on_owned(_connection, name):
        owned.append(name)
        if len(owned) == 2:
            print("ready", flush=True)

    # Both names on one connection, which serves the three objects.
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    acquired(connection, None)
    for name in (SOURCES, BOOKS):
        Gio.bus_own_name_on_connection(
            connection, name, Gio.BusNameOwnerFlags.REPLACE, on_owned, lambda *_: loop.quit()
        )
    loop.run()


if __name__ == "__main__":
    main()
