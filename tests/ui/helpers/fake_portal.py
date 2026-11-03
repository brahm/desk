# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""A Screenshot portal for the UI tests: it answers PickColor with the
color given on the command line, as red, green and blue from 0 to 1, as
the desktop would once the user picks one. It prints "ready" when it owns
the name of the portal.
"""

import sys

from gi.repository import Gio, GLib

NAME = "org.freedesktop.portal.Desktop"
PATH = "/org/freedesktop/portal/desktop"
INTERFACE = """
<node>
  <interface name="org.freedesktop.portal.Screenshot">
    <method name="PickColor">
      <arg type="s" name="parent_window" direction="in"/>
      <arg type="a{sv}" name="options" direction="in"/>
      <arg type="o" name="handle" direction="out"/>
    </method>
  </interface>
</node>
"""


def main():
    color = tuple(float(c) for c in sys.argv[1:4])
    loop = GLib.MainLoop()
    info = Gio.DBusNodeInfo.new_for_xml(INTERFACE)

    def call(connection, sender, path, interface, method, parameters, invocation):
        _, options = parameters.unpack()
        token = options.get("handle_token", "desk")
        handle = f"{PATH}/request/{sender.lstrip(':').replace('.', '_')}/{token}"
        invocation.return_value(GLib.Variant("(o)", (handle,)))

        def respond():
            results = GLib.Variant("a{sv}", {"color": GLib.Variant("(ddd)", color)})
            connection.emit_signal(
                sender,
                handle,
                "org.freedesktop.portal.Request",
                "Response",
                GLib.Variant.new_tuple(GLib.Variant("u", 0), results),
            )
            return False

        GLib.timeout_add(100, respond)

    def acquired(connection, _name):
        connection.register_object(PATH, info.interfaces[0], call, None, None)

    def owned(_connection, _name):
        print("ready", flush=True)

    Gio.bus_own_name(
        Gio.BusType.SESSION,
        NAME,
        Gio.BusNameOwnerFlags.REPLACE,
        acquired,
        owned,
        lambda *_: loop.quit(),
    )
    loop.run()


if __name__ == "__main__":
    main()
