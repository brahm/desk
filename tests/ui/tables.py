# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Drives Desk Tables and checks its window of interface B through the
accessibility tree, as a screen reader sees it: the sheets in the
sidebar, the header bar, the formula bar, the cell in focus, the bottom
bar, the Inspector, and the shortcuts decided for Desk Tables.

Run it with ci/ui-test.sh, which starts a display and the accessibility
bus. It reads with PyGObject and the AT-SPI typelib (gir1.2-atspi-2.0)
and types with xdotool.
"""

import os
import subprocess
import sys
import time
from pathlib import Path

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
TABLES = ROOT / "target" / "debug" / "desk-tables"
TIMEOUT = 20
# The window of a new document, which xdotool finds by its title.
TITLE = "Untitled – Desk Tables"
# The buttons of the header bar of interface B, by their names (NINT-07).
HEADER = ("Sidebar", "Insert", "Find", "Inspector", "Main Menu")
OBJECTS = ("Table", "Chart", "Text Box", "Shape", "Media", "Comment")


def wait_for(what, find):
    """Calls find until it returns something true, or fails."""
    deadline = time.monotonic() + TIMEOUT
    while time.monotonic() < deadline:
        found = find()
        if found:
            return found
        time.sleep(0.2)
    fail(f"{what} did not happen")


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


def check(what, condition):
    if not condition:
        fail(what)
    print(f"ok - {what}")


def walk(node, depth=0):
    yield node
    if depth > 40:
        return
    for i in range(node.get_child_count()):
        child = node.get_child_at_index(i)
        if child is not None:
            yield from walk(child, depth + 1)


def app_named(name):
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app is not None and app.get_name() == name:
            return app
    return None


def showing(node):
    return node is not None and node.get_state_set().contains(Atspi.StateType.SHOWING)


def showing_named(app, name):
    """The node of a name that shows, if there is one."""
    try:
        return next((n for n in walk(app) if n.get_name() == name and showing(n)), None)
    except GLib.GError:
        return None


def extents(node):
    return Atspi.Component.get_extents(node, Atspi.CoordType.WINDOW)


def press(*keys):
    for key in keys:
        if key == "F9":
            # The xdotool of Ubuntu 26.10 sends a function key with Alt;
            # AT-SPI sends it plain.
            Atspi.generate_keyboard_event(0xFFC6, None, Atspi.KeySynthType.SYM)
        else:
            subprocess.run(["xdotool", "key", key], check=True)
        time.sleep(0.08)
    time.sleep(0.3)


def type_text(text):
    subprocess.run(["xdotool", "type", "--delay", "60", text], check=True)
    time.sleep(0.3)


def window_call(method, *arguments):
    """Calls a method of the actions of the first window, as GTK puts them
    on the session bus."""
    return subprocess.run(
        [
            "gdbus", "call", "--session", "--dest", "br.com.brahm.Desk.Tables",
            "--object-path", "/br/com/brahm/Desk/Tables/window/1",
            "--method", f"org.gtk.Actions.{method}", *arguments,
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout


def activate(action, value=None):
    """Activates an action of the first window, as its menu item does: the
    items of GTK's menus have no names for the accessibility tree."""
    parameter = "[]" if value is None else f"[<'{value}'>]"
    window_call("Activate", action, parameter, "{}")


def action_state(name):
    """The state of an action of the first window, as text: "true"."""
    out = window_call("Describe", name)
    inside = out[out.rfind("[") + 1 : out.rfind("]")]
    return inside.strip().removeprefix("<").removesuffix(">")


def resize(width, height):
    subprocess.run(
        ["xdotool", "search", "--name", TITLE, "windowsize", str(width), str(height)],
        check=True,
    )


def table_size(app):
    """The rows and columns of the table selected, as the sheet describes
    it to a screen reader: "Table 1, 45 rows by 13 columns"."""
    try:
        sheet = next(n for n in walk(app) if n.get_role() == Atspi.Role.TABLE and showing(n))
    except (StopIteration, GLib.GError):
        return None
    words = (sheet.get_description() or "").replace(",", "").split()
    numbers = [int(w) for w in words if w.isdigit()]
    return tuple(numbers[-2:]) if len(numbers) >= 2 else None


def main():
    # The buttons of the window are those of GNOME, Close alone.
    gtk = Path(os.environ["XDG_CONFIG_HOME"]) / "gtk-4.0"
    gtk.mkdir(parents=True, exist_ok=True)
    (gtk / "settings.ini").write_text("[Settings]\ngtk-decoration-layout=appmenu:close\n")
    tables = subprocess.Popen([str(TABLES)])
    try:
        app = wait_for("Desk Tables on the accessibility bus", lambda: app_named("desk-tables"))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", TITLE, "windowfocus", "--sync"],
            check=True,
        )
        wait_for("the window", lambda: showing_named(app, "Main Menu"))

        # The window of interface B (NINT-02, NINT-07).
        missing = [b for b in HEADER + OBJECTS if showing_named(app, b) is None]
        check(
            "the header bar has Sidebar, Insert, the six objects, Find, the Inspector and the menu (NINT-07)",
            not missing,
        )
        check(
            "the header bar has nothing to customize it (NINT-07)",
            showing_named(app, "Customize Toolbar") is None,
        )
        for bar in ("Format Bar", "Formula Bar"):
            check(f"the window has the {bar.lower()} (NINT-08, NINT-02)", showing_named(app, bar))
        check(
            "the bottom bar has the zoom and the instant calculations (NINT-04, NGNM-13)",
            showing_named(app, "Zoom") and showing_named(app, "Instant Calculations"),
        )

        # The sheets in the sidebar, with the tables of the sheet shown
        # (NINT-03, NGNM-12).
        check("the sidebar shows the sheet of a new document (NINT-03)", showing_named(app, "Sheet 1"))
        check("and its table under it (NINT-03)", showing_named(app, "Table 1"))
        add = showing_named(app, "Add Sheet")
        check(
            "the sidebar is as high as the window, beside the header bar (NGNM-12)",
            add and abs(extents(add).y - extents(showing_named(app, "Main Menu")).y) < 12,
        )

        # The first cell of the table has the focus, and the arrows move it
        # (NTAB-07).
        wait_for("the cell in focus in the formula bar", lambda: showing_named(app, "A1"))
        check("a new document starts in the first cell of its table", True)
        check("the bottom bar names the sheet (NGNM-13)", showing_named(app, "Sheet 1 (1 of 1)"))
        check("and counts the cells selected (NGNM-13)", showing_named(app, "1 cell"))
        press("Right")
        wait_for("B1 in the formula bar", lambda: showing_named(app, "B1"))
        press("shift+Down", "shift+Down")
        wait_for("a range in the formula bar", lambda: showing_named(app, "B1:B3"))
        check("Shift and the arrows select a range of cells (NTAB-07)", showing_named(app, "3 cells"))

        # Alt and the arrows add rows and columns, as decided for Desk
        # Tables, and Ctrl+Z takes them back (NTAB-06, NDOC-08).
        check("the table of the Blank template has 45 rows and 13 columns", table_size(app) == (45, 13))
        press("alt+Down")
        wait_for("a row added", lambda: table_size(app) == (46, 13))
        check("Alt+Down adds a row below (NTAB-06)", True)
        press("alt+Right")
        wait_for("a column added", lambda: table_size(app) == (46, 14))
        check("Alt+Right adds a column after (NTAB-06)", True)
        press("alt+Up")
        wait_for("a row added above", lambda: showing_named(app, "B2:B4"))
        check("Alt+Up adds a row above, and the cells move down with it (NTAB-06)", table_size(app) == (47, 14))
        press("ctrl+z", "ctrl+z", "ctrl+z")
        wait_for("the rows back", lambda: table_size(app) == (45, 13))
        check("Ctrl+Z takes the rows and the column back (NDOC-08)", showing_named(app, "B1:B3"))

        # The levels of the Inspector: Format, Data and Document, and the
        # name of the table in Format (NINT-09).
        press("ctrl+alt+i")
        wait_for("the Inspector", lambda: showing_named(app, "Show Table Name"))
        for level in ("Format", "Data", "Document"):
            check(f"the Inspector has the level {level} (NINT-09)", showing_named(app, level))
        press("ctrl+alt+i")

        # Sheets are added, shown, renamed and deleted (NTAB-02, NTAB-04).
        activate("add-sheet")
        wait_for("the new sheet", lambda: showing_named(app, "Sheet 2 (2 of 2)"))
        check("the Add button adds a sheet with a table after the one shown (NTAB-02)", showing_named(app, "Sheet 2"))
        press("ctrl+Page_Up")
        wait_for("the first sheet", lambda: showing_named(app, "Sheet 1 (1 of 2)"))
        check("Ctrl+Page Up shows the sheet before (NGNM-13)", True)
        activate("rename-sheet")
        wait_for("the dialog", lambda: showing_named(app, "Rename Sheet"))
        press("ctrl+a")
        type_text("Receitas")
        press("Return")
        wait_for("the new name", lambda: showing_named(app, "Receitas (1 of 2)"))
        check("a sheet takes a new name (NTAB-04)", showing_named(app, "In “Receitas”"))
        activate("duplicate-sheet")
        wait_for("the copy", lambda: showing_named(app, "Receitas (copy) (2 of 3)"))
        check("Duplicate Sheet puts a copy after the sheet (NTAB-02)", True)
        activate("move-sheet", "up")
        wait_for("the copy moved up", lambda: showing_named(app, "Receitas (copy) (1 of 3)"))
        check("a sheet moves up in the list (NTAB-03)", True)
        activate("delete-sheet")
        wait_for("the copy deleted", lambda: showing_named(app, "Receitas (1 of 2)"))
        check("Delete takes the sheet away (NTAB-02)", showing_named(app, "Receitas (copy)") is None)
        press("ctrl+z")
        wait_for("the copy back", lambda: showing_named(app, "Receitas (copy)"))
        check("Ctrl+Z brings the sheet back (NDOC-08)", True)

        # A table goes on the sheet, and Delete takes it away (NTAB-05).
        activate("insert-table")
        wait_for("the new table", lambda: showing_named(app, "Table 2"))
        check("the Table button puts a table on the sheet (NTAB-05)", showing_named(app, "A1"))
        press("Escape")
        wait_for("the table selected", lambda: showing_named(app, "Table 2") and not showing_named(app, "A1"))
        press("Delete")
        wait_for("the table deleted", lambda: showing_named(app, "Table 2") is None)
        check("Escape selects the table, and Delete takes it away (NTAB-05)", True)

        # The pages of the sidebar and their shortcuts.
        press("ctrl+shift+f")
        wait_for("the formula list", lambda: showing_named(app, "No Formulas"))
        check("Ctrl+Shift+F shows the formula list, as decided for Desk Tables", action_state("formula-list") == "true")
        press("ctrl+shift+t")
        wait_for("the table styles", lambda: showing_named(app, "Basic (No Grid)"))
        check("Ctrl+Shift+T shows the table styles (NINT-02)", True)
        press("F9")
        wait_for("the sidebar to hide", lambda: showing_named(app, "Add Sheet") is None)
        check("F9 hides the sidebar (NGNM-12)", action_state("sidebar") == "false")
        press("F9")
        wait_for("the sidebar back", lambda: showing_named(app, "Add Sheet"))

        # Ctrl+/ searches the commands (NGNM-10).
        press("ctrl+slash")
        wait_for("the command search", lambda: showing_named(app, "Search Commands"))
        type_text("blank sheet")
        wait_for("Blank Sheet in the search", lambda: showing_named(app, "Blank Sheet"))
        press("Return")
        wait_for("the command search to close", lambda: showing_named(app, "Search Commands") is None)
        wait_for("the sheet it added", lambda: showing_named(app, "Sheet 1 (2 of 4)"))
        check("Search Commands finds a command and runs it (NGNM-10)", True)

        # Ctrl+? shows the shortcuts of Desk Tables (NINT-16).
        press("ctrl+question")
        wait_for("the shortcuts", lambda: showing_named(app, "Add a row above"))
        check("Ctrl+? shows the shortcuts, Alt and the arrows among them (NINT-16)", True)
        press("Escape")

        # The zoom (NINT-04).
        press("ctrl+plus")
        wait_for("the zoom", lambda: showing_named(app, "125 %") or showing_named(app, "125%"))
        press("ctrl+0")
        wait_for("the zoom back", lambda: showing_named(app, "100 %") or showing_named(app, "100%"))
        check("Ctrl++ and Ctrl+0 zoom the sheet (NINT-04)", True)

        # A narrow window (NGNM-01).
        resize(640, 600)
        wait_for("the objects to leave the header bar", lambda: showing_named(app, "Chart") is None)
        missing = [b for b in HEADER if showing_named(app, b) is None]
        check("below 720 px the header bar keeps Insert and leaves out the objects (NGNM-01)", not missing)
        check("and the format bar leaves out its secondary controls (NGNM-01)", showing_named(app, "Fewer Decimals") is None)
    finally:
        tables.terminate()
        tables.wait(timeout=10)


if __name__ == "__main__":
    main()
