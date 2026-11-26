# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Drives Desk Writer and checks it through the accessibility tree, as a
screen reader sees it: it opens a document, reads its text and cursor,
types, undoes and saves.

Run it with ci/ui-test.sh, which starts a display and the accessibility
bus. It reads with PyGObject and the AT-SPI typelib (gir1.2-atspi-2.0)
and types with xdotool.
"""

import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
WRITER = ROOT / "target" / "debug" / "desk-writer"
CONVERT = ROOT / "target" / "debug" / "desk-convert"
FIXTURE = ROOT / "corpus" / "fixtures" / "pages" / "est-02-landscape-letter.pages"
TIMEOUT = 20


def wait_for(what, find):
    """Calls find until it returns something true, or fails. A node that
    goes away while find reads the tree, as the widgets of a closing tab
    do, makes it look again."""
    deadline = time.monotonic() + TIMEOUT
    while time.monotonic() < deadline:
        try:
            found = find()
        except GLib.GError:
            found = None
        if found:
            return found
        time.sleep(0.2)
    fail(f"{what} did not happen")


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


def walk(node, depth=0):
    # The popovers of the Format pane hang from its rows, deep in the
    # Inspector: the Collections of the Fonts popover are 44 levels down.
    yield node
    if depth > 60:
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


def find(app, role, name=None):
    for node in walk(app):
        if node.get_role() == role and (name is None or node.get_name() == name):
            return node
    return None


def find_named(app, name):
    for node in walk(app):
        if node.get_name() == name:
            return node
    return None


def showing(node):
    return node is not None and node.get_state_set().contains(Atspi.StateType.SHOWING)


def text_of(node):
    return Atspi.Text.get_text(node, 0, Atspi.Text.get_character_count(node))


def caret_of(node):
    return Atspi.Text.get_caret_offset(node)


# The keysyms of the function keys.
FUNCTION_KEYS = {f"F{n}": 0xFFBD + n for n in range(1, 13)}


def press(*keys):
    for key in keys:
        if key in FUNCTION_KEYS:
            # The xdotool of Ubuntu 26.10 sends a function key with Alt,
            # at the level of the key that has the same keysym; AT-SPI
            # sends it plain.
            Atspi.generate_keyboard_event(FUNCTION_KEYS[key], None, Atspi.KeySynthType.SYM)
            time.sleep(0.08)
        else:
            subprocess.run(["xdotool", "key", key], check=True)
            time.sleep(0.08)
    time.sleep(0.3)


def type_text(text):
    subprocess.run(["xdotool", "type", "--delay", "60", text], check=True)
    time.sleep(0.3)


def check(what, condition):
    if not condition:
        fail(what)
    print(f"ok - {what}")


def main():
    work = Path(tempfile.mkdtemp(prefix="desk-ui-"))
    letter = work / "Letter.odt"
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    # The table of replacements is off on a new install, as in Pages; the
    # test turns it on as a user would. The windows open at 100 %, which
    # the places the test clicks are measured at.
    config = Path(os.environ["XDG_CONFIG_HOME"]) / "desk"
    config.mkdir(parents=True, exist_ok=True)
    (config / "preferences.ini").write_text("[editing]\nreplace-text=true\n[rulers]\nzoom=100\n")
    # The buttons of the window are those of GNOME, Close alone, which the
    # narrow window is measured with (GNM-01).
    gtk = Path(os.environ["XDG_CONFIG_HOME"]) / "gtk-4.0"
    gtk.mkdir(parents=True, exist_ok=True)
    (gtk / "settings.ini").write_text("[Settings]\ngtk-decoration-layout=appmenu:close\n")
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, "Letter.odt"),
        )
        check("the document is named after its file", document.get_name() == "Letter.odt")
        check(
            "a screen reader reads the text of the document",
            text_of(document) == "US Letter, landscape\n",
        )
        check("the cursor starts at the start", caret_of(document) == 0)

        # The document has the keyboard when it opens; the window takes it
        # from the display, which has no window manager to give it.
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Letter.odt", "windowfocus", "--sync"],
            check=True,
        )
        check("the document has the keyboard", document.get_state_set().contains(Atspi.StateType.FOCUSED))
        press("ctrl+End")
        check("the cursor moves to the end", caret_of(document) == len("US Letter, landscape"))
        type_text(" ação")
        wait_for(
            "the typed text in the accessibility tree",
            lambda: text_of(document) == "US Letter, landscape ação\n",
        )
        check("typing with accents reaches the screen reader", True)
        check("the cursor follows the typing", caret_of(document) == len("US Letter, landscape ação"))

        press("shift+Home")
        selection = Atspi.Text.get_selection(document, 0)
        check(
            "the selection reaches the screen reader",
            (selection.start_offset, selection.end_offset) == (0, len("US Letter, landscape ação")),
        )

        # Bold is in the Font group of the Format pane (step 2 of M+).
        press("ctrl+alt+i")
        bold = wait_for(
            "the Bold button of the Format pane",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.TOGGLE_BUTTON
                    and n.get_name() == "Bold"
                    and showing(n)
                ),
                None,
            ),
        )
        check("the Bold button is named", bold is not None)
        press("ctrl+b")
        wait_for("Bold to turn on", lambda: bold.get_state_set().contains(Atspi.StateType.PRESSED))
        check("Ctrl+B turns Bold on for the selection", True)
        press("ctrl+z")
        wait_for("Bold to turn off", lambda: not bold.get_state_set().contains(Atspi.StateType.PRESSED))
        check("Undo takes Bold back", True)
        press("ctrl+alt+i")

        press("End", "ctrl+z")
        wait_for(
            "the undone text in the accessibility tree",
            lambda: text_of(document) == "US Letter, landscape\n",
        )
        check("Undo takes the typing back", True)

        press("ctrl+shift+z")
        wait_for("Redo", lambda: text_of(document) == "US Letter, landscape ação\n")
        check("Redo brings it back", True)

        press("ctrl+s")

        def saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    return "ação" in package.read("content.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return False

        wait_for("the saved file", saved)
        check("Save writes the text to the file", True)
        with zipfile.ZipFile(letter) as package:
            check("the file has a preview", "Thumbnails/thumbnail.png" in package.namelist())
        frame = find(app, Atspi.Role.FRAME)
        check("the title no longer marks changes", frame and frame.get_name() == "Letter.odt")

        # The ruler shows in a word processing document, and Ctrl+R hides
        # it (INT-17).
        check("the ruler shows", showing(find_named(app, "Ruler")))
        press("ctrl+r")
        wait_for("the ruler to hide", lambda: not showing(find_named(app, "Ruler")))
        check("Ctrl+R hides the ruler", True)
        press("ctrl+r")
        wait_for("the ruler to show again", lambda: showing(find_named(app, "Ruler")))
        check("Ctrl+R shows it again", True)

        # Tab types a tab, which goes to the next stop (FMT-16).
        press("ctrl+End", "Tab")
        type_text("x")
        wait_for(
            "the tab in the accessibility tree",
            lambda: text_of(document) == "US Letter, landscape ação\tx\n",
        )
        check("Tab types a tab", True)

        # "1. " at the start of a paragraph starts a numbered list, whose
        # marker goes (LST-01), and Return on an empty item ends it.
        press("Return")
        type_text("1. one")
        press("Return")
        wait_for(
            "the item of the list in the accessibility tree",
            # Capitals are not fixed on a new install, as in Pages (TXT-07).
            lambda: text_of(document) == "US Letter, landscape ação\tx\none\n\n",
        )
        check("a marker starts a list", True)

        # The Inspector shows the list (LST-02) and the effects of the text
        # (FMT-10, FMT-21).
        press("ctrl+alt+i")
        wait_for("the list in the Inspector", lambda: showing(find_named(app, "Indent Level")))
        check("the Inspector shows the level of the item", True)
        # Alt+Shift+Right indents the item alone, as Option-Tab does in
        # Pages, since Alt+Tab switches windows in GNOME (LST-02).
        level = spin_of(app, "Indent Level")
        press("alt+shift+Right")
        wait_for("the item to go a level in", lambda: Atspi.Value.get_current_value(level) == 2)
        press("alt+shift+Left")
        wait_for("the item to come back", lambda: Atspi.Value.get_current_value(level) == 1)
        check("Alt+Shift+Right and Alt+Shift+Left change the level of the item (LST-02)", True)
        press("Return")
        wait_for("the list to end", lambda: not showing(find_named(app, "Indent Level")))
        check("Return on an empty item ends the list", True)
        # The Font group has the Typeface row, and its Advanced Options
        # hold character spacing, the baseline and capitalization (B-019).
        advanced = wait_for(
            "the Advanced Options of the Font group",
            lambda: inspector_node(app, "Advanced Options", has_action),
        )
        Atspi.Action.do_action(advanced, 0)
        # None in the Colors popover of a color that can be off turns it
        # off, as its switch does (B-026).
        toggle_switch_row(app, "Character Background", "the background of the characters")

        def background_switch():
            node = inspector_node(app, "Character Background", lambda n: n.get_role() in SWITCHES)
            return node if node is not None and checked(node) else None

        switch = wait_for("the background of the characters to turn on", background_switch)
        colors = wait_for(
            "the Colors popover of the background",
            lambda: inspector_node(app, "Character Background: Color", has_action),
        )
        open_menu_button(colors)
        none = wait_for("None in the Colors popover", lambda: popover_button(app, "None"))
        Atspi.Action.do_action(none, 0)
        wait_for("None to turn the background off", lambda: not checked(switch) or None)
        check("None takes the background of the characters away (B-026)", True)
        for name in (
            "Typeface",
            "Character Spacing",
            "Baseline Shift",
            "Capitalization",
            "Borders and Rules",
            "Hyphenate Words",
            "Hyphenate the Document",
            "Header Margin",
            "Space Between Notes",
            "Styles to Include",
        ):
            wait_for(f"{name} in the Inspector", lambda: inspector_node(app, name))
        wait_for("the Link popover of the More tab", lambda: link_node(app, "Make Links Inactive"))
        press("Escape")
        check("the Inspector shows spacing, hyphenation, borders and rules", True)
        check("the Inspector shows the headers and footers (EST-01)", True)
        check("the Inspector shows how notes show (EST-13)", True)
        check("the Inspector shows the table of contents and links (EST-23, EST-25)", True)
        press("ctrl+alt+i")

        # The Fonts popover has its effects and typography (INT-15,
        # FMT-08); it has no shortcut of its own (D-024).
        activate("fonts")
        wait_for("the Fonts popover", lambda: showing_named(app, "Ligatures"))
        check("win.fonts shows the Fonts popover (INT-15, IOP-06)", True)
        check("it has the shadow", showing_named(app, "Shadow") is not None)
        check(
            "it has the colors of the underline and the strikethrough (FMT-01, FMT-02)",
            showing_named(app, "Underline Color") is not None
            and showing_named(app, "Strikethrough Color") is not None,
        )
        check("it has the typography", True)
        press("Escape")

        # Ctrl+Shift+T shows the Styles page of the sidebar, with the style
        # of the paragraph chosen in its list (INT-18).
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Letter.odt", "windowfocus", "--sync"],
            check=True,
        )
        press("ctrl+shift+t")
        paragraphs = wait_for(
            "the Styles drawer",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.LIST
                    and n.get_name() == "Paragraph Styles"
                    and showing(n)
                ),
                None,
            ),
        )
        check("Ctrl+Shift+T shows the styles in the sidebar (INT-18)", True)
        chosen = [
            n
            for n in walk(paragraphs)
            if n.get_role() == Atspi.Role.LIST_ITEM
            and n.get_state_set().contains(Atspi.StateType.SELECTED)
        ]
        check("the style of the paragraph is chosen in the drawer", len(chosen) == 1)
        check("the character styles show", showing(find_named(app, "Character Styles")))
        press("ctrl+shift+t")

        # Smart quotes and the replacements of the table, as typed
        # (TXT-06, TXT-07).
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Letter.odt", "windowfocus", "--sync"],
            check=True,
        )
        press("ctrl+End")
        type_text('"hi" (c) ')
        wait_for(
            "the corrected text in the accessibility tree",
            lambda: text_of(document).endswith("\n\u201chi\u201d \u00a9 \n"),
        )
        check("quotes curl and (c) becomes a symbol (TXT-06, TXT-07)", True)

        # With text selected, Check Spelling looks only in it (TXT-10).
        type_text("qwzx vbnk plmo")
        wait_for("the misspelled words", lambda: "vbnk plmo" in text_of(document))
        press("ctrl+shift+Left", "ctrl+shift+Left")

        def selected():
            if Atspi.Text.get_n_selections(document) == 0:
                return None
            selection = Atspi.Text.get_selection(document, 0)
            return text_of(document)[selection.start_offset : selection.end_offset]

        wait_for("the last two words selected", lambda: selected() == "vbnk plmo")
        press("ctrl+semicolon")
        wait_for("the first mistake of the selection", lambda: selected() == "vbnk")
        press("ctrl+semicolon")
        wait_for("the next mistake of the selection", lambda: selected() == "plmo")
        press("ctrl+semicolon")
        time.sleep(0.5)
        check(
            "Check Spelling stays in the selected text (TXT-10)",
            selected() == "plmo",
        )
        press("End")

        # The Search Results page of the sidebar lists the matches as they
        # are typed (TXT-13).
        press("ctrl+alt+f")
        results = wait_for(
            "the Search sidebar",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.LIST
                    and n.get_name() == "Search Results"
                    and showing(n)
                ),
                None,
            ),
        )
        type_text("landscape")
        wait_for(
            "the match in the sidebar",
            lambda: sum(1 for n in walk(results) if n.get_role() == Atspi.Role.LIST_ITEM) == 1,
        )
        check("the Search sidebar lists the match", True)

        # Find & Replace replaces every match (TXT-12), from the bar that
        # Ctrl+F reveals.
        press("ctrl+f")
        bar = wait_for(
            "the Find & Replace bar",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_name() == "Find & Replace" and showing(n) and n.get_child_count() > 0
                ),
                None,
            ),
        )
        type_text("hi")
        Atspi.Action.do_action(
            next(n for n in walk(bar) if n.get_name() == "Show Replace"), 0
        )
        wait_for("the Replace field", lambda: showing(find_named(bar, "Replace All")))
        type_text("hello")
        replace_all = next(
            n
            for n in walk(bar)
            if n.get_role() == Atspi.Role.PUSH_BUTTON and n.get_name() == "Replace All"
        )
        Atspi.Action.do_action(replace_all, 0)
        wait_for(
            "the replaced text",
            lambda: "\u201chello\u201d" in text_of(document),
        )
        check("Replace All replaces the matches", True)
        press("Escape")

        # Ctrl+: shows Spelling and Grammar beside the pages (TXT-10).
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Letter.odt", "windowfocus", "--sync"],
            check=True,
        )
        press("ctrl+colon")
        wait_for("Spelling and Grammar", lambda: showing_named(app, "Spelling and Grammar"))
        check("Ctrl+: shows Spelling and Grammar", True)

        # With Check Grammar, the proofreader of Pages '09 that Desk keeps
        # finds a word written twice in the text selected (TXT-11).
        grammar = wait_for(
            "Check Grammar",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.CHECK_BOX
                    and n.get_name() == "Check Grammar"
                    and showing(n)
                ),
                None,
            ),
        )
        # A check button of GTK has no action through AT-SPI: it is clicked.
        if not grammar.get_state_set().contains(Atspi.StateType.CHECKED):
            click_node(grammar, "Letter.odt")
        wait_for(
            "Check Grammar on",
            lambda: grammar.get_state_set().contains(Atspi.StateType.CHECKED),
        )
        # The click gave the check button the keyboard: the document
        # takes it back with a click of its own.
        click_node(document, "Letter.odt")
        wait_for(
            "the document to have the keyboard",
            lambda: document.get_state_set().contains(Atspi.StateType.FOCUSED),
        )
        press("ctrl+End")
        before = text_of(document)
        press("Return")
        type_text("Go to to bed")
        wait_for("the repeated word", lambda: "Go to to bed" in text_of(document))
        press("shift+Home")
        wait_for("the new paragraph selected", lambda: selected() == "Go to to bed")
        find_next = wait_for(
            "the Find Next button",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.PUSH_BUTTON
                    and n.get_name() == "Find Next"
                    and showing(n)
                ),
                None,
            ),
        )
        Atspi.Action.do_action(find_next, 0)
        wait_for("the repeated word found", lambda: selected() == "to to")
        check(
            "the proofreader finds a repeated word (TXT-11)",
            showing_named(app, "Repeated word") is not None,
        )
        # The paragraph goes again, for the checks that follow.
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Letter.odt", "windowfocus", "--sync"],
            check=True,
        )
        press("End", "shift+Home", "BackSpace", "BackSpace")
        wait_for("the paragraph gone", lambda: text_of(document) == before)

        # Ctrl+. shows the emoji and symbols at the cursor (TXT-05).
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Letter.odt", "windowfocus", "--sync"],
            check=True,
        )
        press("ctrl+period")
        wait_for(
            "the special characters",
            lambda: any(
                n.get_name() == "Smileys & People" and showing(n) for n in walk(app)
            ),
        )
        check("Ctrl+. shows the special characters (TXT-05)", True)
        press("Escape")

        # Duplicate opens an untitled copy beside the original (DOC-14).
        press("ctrl+shift+d")
        wait_for(
            "the copy",
            lambda: any(
                n.get_role() == Atspi.Role.FRAME and "Letter copy" in (n.get_name() or "")
                for n in walk(app)
            ),
        )
        check("Duplicate opens a copy (DOC-14)", True)

        # Close All saves the documents that have a file, and asks to save
        # each new one with changes, here the copy (DOC-11, DOC-18).
        press("ctrl+alt+w")
        wait_for(
            "the question to save",
            lambda: any(
                n.get_name() == "Save Changes?" and showing(n) for n in walk(app)
            ),
        )
        check("Close All asks to save the changes (DOC-18)", True)
        press("Escape")
    finally:
        writer.terminate()
        writer.wait(timeout=10)
    selections_and_colors(work)
    header_on_the_page(work)
    objects_on_the_page(work)
    text_box_on_the_page(work)
    text_in_a_shape(work)
    object_in_the_text(work)
    shadow_of_a_shape(work)
    shadow_of_the_text(work)
    objects_in_the_outline(work)
    instant_alpha_of_a_picture(work)
    chosen_picture(work)
    chosen_movie(work)
    reduced_movie(work)
    points_of_a_star(work)
    drawn_with_the_pen(work)
    editable_shape(work)
    connection_line(work)
    position_of_a_turned_shape(work)
    media_browser(work)
    master_objects(work)
    page_layout(work)
    copies_of_a_shape(work)
    objects_between_apps(work)
    styled_text_between_apps(work)
    guide_from_the_ruler(work)
    screen_view(work)
    pages_as_sections(work)
    reflection_of_a_shape(work)
    advanced_gradient(work)
    mask_and_frame(work)
    mask_from_the_menu(work)
    gnome_window(work)
    two_level_inspector(work)
    sidebar_pages(work)
    document_controls(work)
    inventory_commands(work)
    header_and_menus(work)
    format_pane(work)
    object_styles(work)
    text_of_a_shape(work)
    small_screen(work)
    tables_and_charts(work)
    automatic_saving(work)
    print("All UI tests passed.")


def click(x, y, count=1, control=False):
    """Clicks a point of the screen, holding Ctrl when asked."""
    command = ["xdotool", "mousemove", "--sync", str(int(x)), str(int(y))]
    if control:
        command += ["keydown", "ctrl"]
    command += ["click", "--repeat", str(count), "--delay", "60", "1"]
    if control:
        command += ["keyup", "ctrl"]
    subprocess.run(command, check=True)
    time.sleep(0.4)


def styled(content, style, prop):
    """Whether an automatic text style of content.xml sets a property."""
    found = re.search(
        rf'<style:style style:name="{style}"[^>]*>(.*?)</style:style>', content, re.S
    )
    return found is not None and prop in found.group(1)


def span_style(content, text):
    found = re.search(rf'<text:span text:style-name="([^"]+)">{text}</text:span>', content)
    return found.group(1) if found else None


def selections_and_colors(work):
    """Ctrl and a click add a range to the selection, which formatting
    changes together (TXT-03), and the eyedropper takes a color from the
    screen through the portal (INT-16)."""
    letter = work / "Selections.odt"
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    portal = subprocess.Popen(
        [sys.executable, "-I", str(Path(__file__).parent / "helpers" / "fake_portal.py"), "1", "0", "0"],
        stdout=subprocess.PIPE,
        text=True,
    )
    writer = None
    try:
        check("the portal is ready", portal.stdout.readline().strip() == "ready")
        writer = subprocess.Popen([str(WRITER), str(letter)])
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, "Selections.odt"),
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Selections.odt", "windowfocus", "--sync"],
            check=True,
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Selections.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        width, height = int(window["WIDTH"]), int(window["HEIGHT"])
        origin = (int(window["X"]), int(window["Y"]))

        def selected():
            if Atspi.Text.get_n_selections(document) == 0:
                caret = caret_of(document)
                return caret, caret
            selection = Atspi.Text.get_selection(document, 0)
            return selection.start_offset, selection.end_offset

        # The words of the line, found by double clicks across the middle
        # of the window, which the page fills.
        line = origin[1] + height // 2
        start = len("US Letter, ")
        first = last = None
        for x in range(origin[0] + 10, origin[0] + width - 10, 8):
            click(x, line, count=2)
            got = selected()
            if got == (0, 2) and first is None:
                first = x
            if got[0] == start and got[1] > start:
                last = x
                break
        check("a double click selects a word", first is not None and last is not None)
        click(first, line, count=2)
        check("a double click selects the first word", selected() == (0, 2))
        click(last, line, count=2, control=True)
        check("Ctrl and a click add the next word", selected() == (start, start + len("landscape")))
        press("ctrl+b")

        # The eyedropper of the text color, in the Colors popover of the
        # Style tab of the Format pane, colors the selection (step 2 of
        # M+); win.colors opens the pane and the popover.
        click(first, line, count=2)
        activate("colors")
        picker = wait_for(
            "the Colors popover",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.PUSH_BUTTON
                    and n.get_name() == "Pick a Color from the Screen"
                    and showing(n)
                ),
                None,
            ),
        )
        check("win.colors opens the Colors popover of the text (INT-16)", showing_named(app, "Blue 3") is not None)
        wait_for("the colors of the document in it (B-019)", lambda: showing_named(app, "Document Color 1"))
        check("it shows the colors the document uses (B-019)", True)
        check(
            "the text color, which is never off, has no None (B-026)",
            popover_button(app, "None") is None,
        )
        Atspi.Action.do_action(picker, 0)
        time.sleep(1)
        press("ctrl+s")

        def saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    content = package.read("content.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return None
            return content if span_style(content, "landscape") else None

        content = wait_for("the saved file", saved)
        us = span_style(content, "US")
        check("both words are bold", us and styled(content, us, 'fo:font-weight="bold"'))
        check(
            "and the second too",
            styled(content, span_style(content, "landscape"), 'fo:font-weight="bold"'),
        )
        check("the word between is not", span_style(content, "Letter,") is None)
        # The first eyedropper of the Style tab is that of the text color.
        check("the eyedropper colors the selection", styled(content, us, '="#ff0000"'))
    finally:
        if writer is not None:
            writer.terminate()
            writer.wait(timeout=10)
        portal.terminate()
        portal.wait(timeout=10)


def header_on_the_page(work):
    """A click above the body puts the cursor in the header, where text is
    typed as in the body, and Esc takes it back (EST-12)."""
    letter = work / "Header.odt"
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, "Header.odt"),
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Header.odt", "windowfocus", "--sync"],
            check=True,
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Header.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        x = int(window["X"]) + int(window["WIDTH"]) // 2
        middle = int(window["Y"]) + int(window["HEIGHT"]) // 2
        # The line of the letter is in the middle of the window, which the
        # page fills; going up from it, a click comes to the header.
        in_header = False
        for y in range(middle, int(window["Y"]), -8):
            click(x, y)
            if text_of(document) == "\n":
                in_header = True
                break
        check("a click above the body puts the cursor in the header", in_header)
        type_text("Topo")
        wait_for("the text typed in the header", lambda: text_of(document) == "Topo\n")
        press("Escape")
        wait_for("the body again", lambda: text_of(document) == "US Letter, landscape\n")
        press("ctrl+s")

        def saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    styles = package.read("styles.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return None
            return styles if "Topo" in styles else None

        styles = wait_for("the saved file", saved)
        check(
            "the header holds the text typed (EST-12)",
            re.search(r"<style:header>.*Topo.*</style:header>", styles, re.S) is not None,
        )
        # The whole visit is one action of the body.
        press("ctrl+z")
        press("ctrl+s")

        def undone():
            try:
                with zipfile.ZipFile(letter) as package:
                    styles = package.read("styles.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return False
            return "Topo" not in styles

        wait_for("the file saved again", undone)
        check("Undo takes back the header as one action", text_of(document) == "US Letter, landscape\n")

        # F9 shows the thumbnails of the pages (INT-19).
        press("F9")
        wait_for(
            "the thumbnail of the first page",
            lambda: any(n.get_name() == "Page 1" and showing(n) for n in walk(app)),
        )
        check("F9 shows the page thumbnails (INT-19)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def points(length):
    """A length of ODF in points."""
    found = re.fullmatch(r"(-?[0-9.]+)(pt|in|cm|mm)", length)
    value, unit = float(found.group(1)), found.group(2)
    return value * {"pt": 1.0, "in": 72.0, "cm": 72 / 2.54, "mm": 72 / 25.4}[unit]


def objects_on_the_page(work):
    """A click selects a shape on the page, which the arrows move and
    Delete removes, with Undo, and typing leaves the text alone while it
    is selected (OBJ-03)."""
    letter = work / "Objects.odt"
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    with zipfile.ZipFile(letter) as package:
        files = {name: package.read(name) for name in package.namelist()}
    # A large rectangle in the middle of the page, under the middle of the
    # window.
    files["content.xml"] = with_filled_style(files["content.xml"]).replace(
        b"<office:text>",
        b'<office:text><draw:rect draw:style-name="Filled" text:anchor-type="page" '
        b'text:anchor-page-number="1" '
        b'svg:x="96pt" svg:y="120pt" svg:width="600pt" svg:height="400pt"/>',
        1,
    )
    with zipfile.ZipFile(letter, "w") as package:
        package.writestr(zipfile.ZipInfo("mimetype"), files.pop("mimetype"))
        for name, data in files.items():
            package.writestr(name, data, zipfile.ZIP_DEFLATED)
    writer = subprocess.Popen([str(WRITER), str(letter)])

    def saved_rect(present=True):
        try:
            with zipfile.ZipFile(letter) as package:
                content = package.read("content.xml").decode()
        except (zipfile.BadZipFile, KeyError, OSError):
            return None
        found = re.search(r"<draw:rect [^>]*>", content)
        if present:
            return found.group(0) if found else None
        return found is None

    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, "Objects.odt"),
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Objects.odt", "windowfocus", "--sync"],
            check=True,
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Objects.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        click(
            int(window["X"]) + int(window["WIDTH"]) // 2,
            int(window["Y"]) + int(window["HEIGHT"]) // 2,
        )
        type_text("zz")
        time.sleep(0.5)
        check(
            "typing leaves the text alone while a shape is selected (OBJ-21)",
            text_of(document) == "US Letter, landscape\n",
        )
        press("shift+Right", "shift+Down", "ctrl+s")
        rect = wait_for(
            "the moved shape in the saved file",
            lambda: (lambda r: r if r and 'svg:x="96pt"' not in r else None)(saved_rect()),
        )
        x = points(re.search(r'svg:x="([^"]+)"', rect).group(1))
        y = points(re.search(r'svg:y="([^"]+)"', rect).group(1))
        check("Shift and the arrows move the shape by ten points (OBJ-23)", (round(x), round(y)) == (106, 130))
        press("Delete", "ctrl+s")
        wait_for("the file without the shape", lambda: saved_rect(False))
        check("Delete removes the selected shape", True)
        press("ctrl+z", "ctrl+s")
        wait_for("the shape back in the file", saved_rect)
        check("Undo brings it back", True)
        # Alt and an arrow leave a copy a point away, as in Pages (OBJ-22).
        click(
            int(window["X"]) + int(window["WIDTH"]) // 2,
            int(window["Y"]) + int(window["HEIGHT"]) // 2,
        )
        press("alt+Right", "ctrl+s")
        wait_for(
            "the copy in the saved file",
            lambda: saved_content(letter, r"(?s)<draw:rect .*<draw:rect "),
        )
        check("Alt and an arrow copy the shape (OBJ-22)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def text_box_on_the_page(work):
    """A double click on a text box puts the cursor in its text, where
    typing goes, and Esc takes it back to the body (OBJ-13)."""
    letter = work / "Box.odt"
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    with zipfile.ZipFile(letter) as package:
        files = {name: package.read(name) for name in package.namelist()}
    files["content.xml"] = files["content.xml"].replace(
        b"<office:text>",
        b'<office:text><draw:frame text:anchor-type="page" text:anchor-page-number="1" '
        b'svg:x="96pt" svg:y="120pt" svg:width="600pt" svg:height="400pt">'
        b"<draw:text-box><text:p>Caixa</text:p></draw:text-box></draw:frame>",
        1,
    )
    with zipfile.ZipFile(letter, "w") as package:
        package.writestr(zipfile.ZipInfo("mimetype"), files.pop("mimetype"))
        for name, data in files.items():
            package.writestr(name, data, zipfile.ZIP_DEFLATED)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, "Box.odt"),
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Box.odt", "windowfocus", "--sync"],
            check=True,
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Box.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        click(
            int(window["X"]) + int(window["WIDTH"]) // 2,
            int(window["Y"]) + int(window["HEIGHT"]) // 2,
            count=2,
        )
        wait_for("the text of the box", lambda: text_of(document) == "Caixa\n")
        check("a double click on a text box edits its text (OBJ-13)", True)
        press("ctrl+End")
        type_text(" nova")
        wait_for("the typed text in the box", lambda: text_of(document) == "Caixa nova\n")
        press("Escape")
        wait_for("the body again", lambda: text_of(document) == "US Letter, landscape\n")
        press("ctrl+s")

        def saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    content = package.read("content.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return False
            return re.search(r"<draw:text-box[^>]*>.*Caixa nova.*</draw:text-box>", content, re.S)

        wait_for("the saved text box", saved)
        check("the text typed in the box is saved in it", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def text_in_a_shape(work):
    """A double click on a shape puts the cursor in it, and what is typed
    is kept as the text of the shape (OBJ-08)."""
    letter = work / "Shape.odt"
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    with zipfile.ZipFile(letter) as package:
        files = {name: package.read(name) for name in package.namelist()}
    files["content.xml"] = with_filled_style(files["content.xml"]).replace(
        b"<office:text>",
        b'<office:text><draw:rect draw:style-name="Filled" text:anchor-type="page" '
        b'text:anchor-page-number="1" '
        b'svg:x="96pt" svg:y="120pt" svg:width="600pt" svg:height="400pt"/>',
        1,
    )
    with zipfile.ZipFile(letter, "w") as package:
        package.writestr(zipfile.ZipInfo("mimetype"), files.pop("mimetype"))
        for name, data in files.items():
            package.writestr(name, data, zipfile.ZIP_DEFLATED)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, "Shape.odt"),
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Shape.odt", "windowfocus", "--sync"],
            check=True,
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Shape.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        click(
            int(window["X"]) + int(window["WIDTH"]) // 2,
            int(window["Y"]) + int(window["HEIGHT"]) // 2,
            count=2,
        )
        wait_for("the empty text of the shape", lambda: text_of(document) == "\n")
        check("a double click on a shape edits its text (OBJ-08)", True)
        type_text("Forma")
        wait_for("the typed text in the shape", lambda: text_of(document) == "Forma\n")
        press("Escape")
        wait_for("the body again", lambda: text_of(document) == "US Letter, landscape\n")
        press("ctrl+s")

        def saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    content = package.read("content.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return False
            return re.search(r"<draw:rect[^>]*>.*Forma.*</draw:rect>", content, re.S)

        wait_for("the saved shape", saved)
        check("the text typed in the shape is saved in it", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


SHAPE = (
    b'<draw:rect draw:style-name="Filled" text:anchor-type="page" text:anchor-page-number="1" '
    b'svg:x="96pt" svg:y="200pt" svg:width="300pt" svg:height="200pt"/>'
)

# The blue that shapes are inserted with. A shape with no fill is selected
# only on its outline, as in Pages (OBJ-21).
FILLED = (
    b'<style:style style:name="Filled" style:family="graphic">'
    b'<style:graphic-properties draw:fill="solid" draw:fill-color="#6c9fd8"/></style:style>'
)


def with_filled_style(content):
    """The content of a document with the style of filled shapes."""
    if b"<office:automatic-styles/>" in content:
        return content.replace(
            b"<office:automatic-styles/>",
            b"<office:automatic-styles>" + FILLED + b"</office:automatic-styles>",
            1,
        )
    return content.replace(b"<office:automatic-styles>", b"<office:automatic-styles>" + FILLED, 1)


def png(width, height, rgb):
    """A PNG of one color."""
    import struct
    import zlib

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    rows = b"".join(b"\0" + bytes(rgb) * width for _ in range(height))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows))
        + chunk(b"IEND", b"")
    )


def open_with_a_shape(work, name, shape=SHAPE, extra=None):
    """Opens a copy of the fixture with a shape over its text, shows the
    Inspector and selects the shape; returns the process, the app on the
    accessibility bus and the file. `extra` adds files to the package."""
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    with zipfile.ZipFile(letter) as package:
        files = {n: package.read(n) for n in package.namelist()}
    files["content.xml"] = with_filled_style(files["content.xml"]).replace(
        b"<office:text>", b"<office:text>" + shape, 1
    )
    files.update(extra or {})
    with zipfile.ZipFile(letter, "w") as package:
        package.writestr(zipfile.ZipInfo("mimetype"), files.pop("mimetype"))
        for n, data in files.items():
            package.writestr(n, data, zipfile.ZIP_DEFLATED)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, name),
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", name, "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        press("ctrl+alt+i")
        click(
            int(window["X"]) + int(window["WIDTH"]) // 3,
            int(window["Y"]) + int(window["HEIGHT"]) * 3 // 4,
        )
    except BaseException:
        writer.terminate()
        writer.wait(timeout=10)
        raise
    return writer, app, letter


def toggle_switch_row(app, title, what):
    """Turns the switch of the first showing row titled `title`."""
    def has_switch(node):
        return any(n.get_role() in SWITCHES for n in walk(node))

    switch = wait_for(what, lambda: inspector_node(app, title, has_switch))
    toggle = next(node for node in walk(switch) if node.get_role() in SWITCHES)
    Atspi.Action.do_action(toggle, 0)


def saved_content(letter, pattern):
    try:
        with zipfile.ZipFile(letter) as package:
            content = package.read("content.xml").decode()
    except (zipfile.BadZipFile, KeyError, OSError):
        return False
    return re.search(pattern, content)


def object_in_the_text(work):
    """The Inspector puts a selected shape in the line of the text, and it
    is saved there (OBJ-01)."""
    writer, app, letter = open_with_a_shape(work, "Inline.odt")
    try:
        toggle_switch_row(app, "Inline with Text", "the Wrap group of the Inspector")
        check("a selected shape shows whether it is in the text", True)
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved shape in the text",
            lambda: saved_content(letter, r"<text:p[^>]*>[^<]*<draw:rect[^>]*as-char"),
        )
        check("the shape put in the text is saved in the line (OBJ-01)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def shadow_of_a_shape(work):
    """The Graphic group of the Inspector gives a selected shape a shadow,
    which is saved with it (OBJ-11)."""
    writer, app, letter = open_with_a_shape(work, "Shadow.odt")
    try:
        toggle_switch_row(app, "Shadow", "the Graphic group of the Inspector")
        check("a selected shape shows whether it has a shadow", True)
        wait_for(
            "the rows of the shadow",
            lambda: (lambda n: n if showing(n) else None)(find_named(app, "Shadow Blur")),
        )
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved shadow",
            lambda: saved_content(letter, r'draw:shadow="visible"'),
        )
        check("the shadow of the shape is saved (OBJ-11)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def shadow_of_the_text(work):
    """The Inspector gives the selected text a shadow, as the Graphic
    inspector of Pages does when text is selected, and it is saved with
    it (FMT-03)."""
    writer, app, letter = open_with_a_shape(work, "TextShadow.odt")
    try:
        press("Escape")
        press("ctrl+a")
        def switch():
            return inspector_node(
                app,
                "Text Shadow",
                lambda n: n.get_role() != Atspi.Role.LABEL and Atspi.Action.get_n_actions(n) > 0,
            )

        Atspi.Action.do_action(wait_for("the Text Shadow switch of the Inspector", switch), 0)
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved shadow of the text",
            lambda: saved_content(letter, r'fo:text-shadow="[^"n]'),
        )
        check("the Inspector gives the text a shadow, which is saved (FMT-03)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def objects_in_the_outline(work):
    """The outline shows the objects of a paragraph in miniature when asked
    (TXT-14)."""
    inline = (
        b'<text:p>A shape <draw:rect text:anchor-type="as-char" '
        b'svg:width="40pt" svg:height="20pt"/> in the line</text:p>'
    )
    writer, app, letter = open_with_a_shape(work, "Outline.odt", inline)
    try:
        activate("outline")
        button = wait_for(
            "the button that shows objects in miniature",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_name() == "Show Objects in Miniature" and showing(n)
                ),
                None,
            ),
        )
        check("the outline shows its bar (TXT-14)", True)
        Atspi.Action.do_action(button, 0)
        wait_for(
            "the objects of the paragraph in miniature",
            lambda: next(
                (n for n in walk(app) if n.get_name() == "Objects" and showing(n)),
                None,
            ),
        )
        check("the outline shows the objects of a paragraph in miniature (TXT-14)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def instant_alpha_of_a_picture(work):
    """Instant Alpha takes out the color clicked in a selected picture,
    and the click is saved with it (OBJ-06)."""
    picture = (
        b'<draw:frame text:anchor-type="page" text:anchor-page-number="1" '
        b'svg:x="96pt" svg:y="200pt" svg:width="300pt" svg:height="200pt">'
        b'<draw:image xlink:href="Pictures/gray.png" xlink:type="simple"/></draw:frame>'
    )
    writer, app, letter = open_with_a_shape(
        work,
        "Alpha.odt",
        picture,
        {"Pictures/gray.png": png(16, 16, (128, 128, 128))},
    )
    try:
        button = wait_for(
            "the Image group of the Inspector",
            lambda: inspector_node(app, "Take Out a Color"),
        )
        check("a selected picture offers Instant Alpha", True)
        Atspi.Action.do_action(button, 0)
        time.sleep(0.5)
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Alpha.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        click(
            int(window["X"]) + int(window["WIDTH"]) // 3,
            int(window["Y"]) + int(window["HEIGHT"]) * 3 // 4,
        )
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved click of Instant Alpha",
            lambda: saved_content(letter, r'desk:instant-alpha="\d+ \d+ 20"'),
        )
        check("the color clicked in the picture is taken out and saved (OBJ-06)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


CHOOSE = "Choose an Image, Movie or Sound"


def choose_file(app, path):
    """Asks Insert, Choose… for a file and picks it in the dialog."""
    press("Escape", "ctrl+alt+p")
    wait_for(
        "the dialog that chooses an image",
        lambda: next(
            (n for n in walk(app) if n.get_name() == CHOOSE and showing(n)),
            None,
        ),
    )

    def location():
        # The entry of the dialog that takes a path, once it has the
        # keyboard.
        try:
            return next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.TEXT
                    and n.get_state_set().contains(Atspi.StateType.EDITABLE)
                    and n.get_state_set().contains(Atspi.StateType.FOCUSED)
                    and showing(n)
                ),
                None,
            )
        except GLib.GError:
            return None

    # The dialog takes the keyboard from the display, which has no window
    # manager to give it.
    subprocess.run(
        ["xdotool", "search", "--sync", "--name", CHOOSE, "windowfocus", "--sync"],
        check=True,
    )
    press("ctrl+l")
    entry = wait_for("the location entry of the dialog", location)
    Atspi.EditableText.set_text_contents(entry, str(path))
    wait_for("the path in the dialog", lambda: text_of(entry) == str(path))

    def open_button():
        # Open of the dialog, and not of the window; Return would search
        # the recent files when the dialog shows them.
        try:
            for n in walk(app):
                if n.get_role() == Atspi.Role.PUSH_BUTTON and n.get_name() == "Open":
                    up = n.get_parent()
                    while up is not None and up.get_name() != CHOOSE:
                        up = up.get_parent()
                    if up is not None:
                        return n
        except GLib.GError:
            pass
        return None

    def opened():
        # On a slow machine the dialog can still be checking the path it
        # was given, and then it lets a click on Open go; a person would
        # click again, and so does the test until the dialog closes.
        try:
            if not any(n.get_name() == CHOOSE and showing(n) for n in walk(app)):
                return True
        except GLib.GError:
            return False
        button = open_button()
        if button is not None:
            button.do_action(0)
        time.sleep(1)
        return False

    wait_for("the Open button of the dialog", open_button)
    wait_for("the dialog to open the file", opened)


def showing_named(app, name):
    """The node of a name that shows, if there is one."""
    try:
        return next((n for n in walk(app) if n.get_name() == name and showing(n)), None)
    except GLib.GError:
        # A part of a dialog that closed while it was walked.
        return None


# The roles of switches: AT-SPI 2.52 has its own.
SWITCHES = {Atspi.Role.CHECK_BOX, getattr(Atspi.Role, "SWITCH", Atspi.Role.CHECK_BOX)}

# The levels of the Inspector, and the tabs under them, by the names of
# their toggles (INT-13).
LEVELS = ("Format", "Document")
TABS = ("Style", "Layout", "More", "Image", "Text", "Media", "Arrange", "Document",
        "Section", "Info", "Table", "Cell", "Chart", "Axis", "Series")


def toggles_named(app, names, role=Atspi.Role.RADIO_BUTTON):
    """The showing toggles of the Inspector named one of `names`: the
    levels are radio buttons, and the tabs page tabs."""
    try:
        return [
            n
            for n in walk(app)
            if n.get_role() == role and n.get_name() in names and showing(n)
        ]
    except GLib.GError:
        return []


def tab_toggles(app):
    """The showing toggles of the tabs of the Inspector."""
    return toggles_named(app, TABS, Atspi.Role.PAGE_TAB)


def popover_button(app, name):
    """The showing push button of a name, such as one of a popover."""
    try:
        return next(
            (
                n
                for n in walk(app)
                if n.get_name() == name
                and n.get_role() == Atspi.Role.PUSH_BUTTON
                and showing(n)
            ),
            None,
        )
    except GLib.GError:
        return None


def checked(node):
    """Whether a switch or a toggle is on."""
    return node.get_state_set().contains(Atspi.StateType.CHECKED)


def has_action(node):
    """Whether a node can be clicked through the accessibility tree."""
    try:
        return Atspi.Action.get_n_actions(node) > 0
    except (GLib.GError, TypeError, NotImplementedError):
        return False


def inspector_node(app, name, accept=None):
    """The showing node of a name in the Inspector, going through its
    levels and their tabs until one shows it. `accept` narrows the
    nodes."""
    def look():
        try:
            return next(
                (
                    n
                    for n in walk(app)
                    if n.get_name() == name and showing(n) and (accept is None or accept(n))
                ),
                None,
            )
        except GLib.GError:
            return None

    found = look()
    if found is not None:
        return found
    for level in LEVELS:
        try:
            for toggle in toggles_named(app, (level,))[:1]:
                Atspi.Action.do_action(toggle, 0)
            time.sleep(0.3)
            found = look()
            if found is not None:
                return found
            for toggle in tab_toggles(app):
                Atspi.Action.do_action(toggle, 0)
                time.sleep(0.3)
                found = look()
                if found is not None:
                    return found
        except GLib.GError:
            # The Inspector made its tabs again for a new selection; the
            # next look finds the new toggles.
            return None
    return None


def tab_shown(app, tab, name):
    """Opens a tab of the Inspector by its name, as Show in Inspector
    does, and returns the showing node named `name` in it (INT-13)."""
    activate("inspector-tab", tab)
    time.sleep(0.3)
    return showing_named(app, name)


def link_node(app, name):
    """The showing node of a name in the Link popover, which the Link row
    of the More tab of text opens (INT-13)."""
    found = showing_named(app, name)
    if found is not None:
        return found
    button = inspector_node(app, "Link", lambda n: n.get_role() == Atspi.Role.TOGGLE_BUTTON)
    if button is not None:
        Atspi.Action.do_action(button, 0)
        time.sleep(0.5)
    return showing_named(app, name)


def opened_without_portals(work, name):
    """Opens a document with a shape, with the file dialog of GTK, which is
    on the accessibility bus with the app, and not the one of a portal."""
    os.environ["GDK_DEBUG"] = "no-portals"
    try:
        return open_with_a_shape(work, name)
    finally:
        del os.environ["GDK_DEBUG"]


def chosen_picture(work):
    """Insert, Choose… puts a photograph on the page, and it is saved as
    the JPEG it came as (OBJ-01)."""
    photo = work / "photo.jpg"
    photo.write_bytes((ROOT / "crates" / "desk-render" / "tests" / "two-colors.jpg").read_bytes())
    writer, app, letter = opened_without_portals(work, "Photo.odt")
    try:
        choose_file(app, photo)
        check("Insert, Choose… asks for an image", True)
        wait_for("the Image group of the Inspector", lambda: inspector_node(app, "Take Out a Color"))
        check("the chosen picture is on the page and selected", True)
        press("ctrl+s")
        wait_for(
            "the saved photograph",
            lambda: saved_content(letter, r'xlink:href="Pictures/object\d+\.jpg"'),
        )
        with zipfile.ZipFile(letter) as package:
            name = next(n for n in package.namelist() if n.endswith(".jpg"))
            check("the photograph is saved as it came", package.read(name) == photo.read_bytes())
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def chosen_movie(work):
    """Insert, Choose… puts a movie on the page with its first frame as its
    poster, and it is saved with it (OBJ-18, OBJ-19)."""
    movie = work / "movie.mp4"
    movie.write_bytes((ROOT / "tests" / "ui" / "fixtures" / "movie.mp4").read_bytes())
    writer, app, letter = opened_without_portals(work, "Movie.odt")
    try:
        choose_file(app, movie)
        wait_for("the Media tab of the Inspector", lambda: tab_shown(app, "media", "Poster Frame"))
        check("the chosen movie is on the page and selected (OBJ-18)", True)
        press("ctrl+s")
        wait_for(
            "the saved movie",
            lambda: saved_content(letter, r'<draw:plugin xlink:href="Media/object\d+\.mp4"'),
        )
        with zipfile.ZipFile(letter) as package:
            name = next(n for n in package.namelist() if n.endswith(".mp4"))
            check("the movie is saved as it came", package.read(name) == movie.read_bytes())
        check(
            "its first frame is saved as its poster (OBJ-19)",
            saved_content(letter, r'</draw:plugin><draw:image xlink:href="Pictures/object\d+\.png"'),
        )
        play = wait_for("the Play button of the Inspector", lambda: tab_shown(app, "media", "Play"))
        play.do_action(0)
        # The Inspector is titled Movie too; the window is a frame.
        wait_for(
            "the window that plays the movie",
            lambda: next(
                (n for n in toggles_named(app, ("Movie",), Atspi.Role.FRAME)),
                None,
            ),
        )
        check("the Inspector plays the movie (OBJ-19)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def reduced_movie(work):
    """Reduce Media takes out of a movie what comes after its stop, and the
    smaller movie is saved (OBJ-20)."""
    movie = work / "movie.mp4"
    movie.write_bytes((ROOT / "tests" / "ui" / "fixtures" / "movie.mp4").read_bytes())
    writer, app, letter = opened_without_portals(work, "Reduced.odt")
    try:
        choose_file(app, movie)
        row = wait_for("the Stop row of the Inspector", lambda: tab_shown(app, "media", "Stop"))
        spin = next(node for node in walk(row) if node.get_role() == Atspi.Role.SPIN_BUTTON)
        Atspi.Value.set_current_value(spin, 0.5)
        time.sleep(1)
        reduce = wait_for(
            "the Reduce button",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.PUSH_BUTTON
                    and n.get_name() == "Reduce Media"
                    and showing(n)
                ),
                None,
            ),
        )
        Atspi.Action.do_action(reduce, 0)
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved movie",
            lambda: saved_content(letter, r'<draw:plugin xlink:href="Media/object\d+\.mp4"'),
        )
        with zipfile.ZipFile(letter) as package:
            name = next(n for n in package.namelist() if n.endswith(".mp4"))
            saved = package.read(name)
        check(
            "the reduced movie is saved smaller (OBJ-20)",
            0 < len(saved) < len(movie.read_bytes()) and saved[4:8] == b"ftyp",
        )
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def media_browser(work):
    """The Media Browser, in the popover of Media, shows the photographs of
    the Pictures folder, and one activated there goes on the page (INT-14)."""
    pictures = work / "Pictures"
    pictures.mkdir(exist_ok=True)
    photo = pictures / "browsed.jpg"
    photo.write_bytes((ROOT / "crates" / "desk-render" / "tests" / "two-colors.jpg").read_bytes())
    dirs = Path(os.environ["XDG_CONFIG_HOME"]) / "user-dirs.dirs"
    dirs.write_text(f'XDG_PICTURES_DIR="{pictures}"\n')
    writer, app, letter = open_with_a_shape(work, "Browsed.odt")
    try:
        press("Escape")
        # The folders are read when the browser first shows.
        activate("media-browser")
        cell = wait_for("the photograph in the Media Browser", lambda: showing_named(app, "browsed.jpg"))
        check("the Media Browser shows the Pictures folder (INT-14)", True)
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Browsed.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        box = Atspi.Component.get_extents(cell, Atspi.CoordType.WINDOW)
        # A double click activates it, as in the browser of Pages.
        click(int(window["X"]) + box.x + box.width / 2, int(window["Y"]) + box.y + box.height / 2, 2)
        wait_for("the popover of Media to close", lambda: showing_named(app, "Choose File…") is None)
        check("the popover of Media closes as the photograph goes on the page (INT-14)", True)
        # The Inspector stays at the side, and shows the photograph.
        wait_for("the Image group of the Inspector", lambda: inspector_node(app, "Take Out a Color"))
        check("an activated photograph goes on the page (INT-14)", True)
    finally:
        dirs.unlink(missing_ok=True)
        writer.terminate()
        writer.wait(timeout=10)


def points_of_a_star(work):
    """The Shape group of the Inspector sets the points of a selected
    star, which are saved with it (OBJ-11)."""
    star = (
        b'<draw:custom-shape draw:style-name="Filled" text:anchor-type="page" '
        b'text:anchor-page-number="1" '
        b'svg:x="96pt" svg:y="200pt" svg:width="300pt" svg:height="200pt">'
        b'<draw:enhanced-geometry draw:type="star5"/></draw:custom-shape>'
    )
    writer, app, letter = open_with_a_shape(work, "Star.odt", star)
    try:
        row = wait_for(
            "the Shape group of the Inspector",
            lambda: inspector_node(
                app,
                "Star Points",
                lambda n: any(c.get_role() == Atspi.Role.SPIN_BUTTON for c in walk(n)),
            ),
        )
        check("a selected star shows its points", True)
        spin = next(node for node in walk(row) if node.get_role() == Atspi.Role.SPIN_BUTTON)
        Atspi.Value.set_current_value(spin, 7.0)
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved points of the star",
            lambda: saved_content(letter, r'desk:shape="star 7 3820"'),
        )
        check("the points of the star are saved (OBJ-11)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def activate(action, value=None):
    """Activates an action of the first window of Desk Writer, as its menu
    item does, through the actions that GTK puts on the session bus: the
    items of GTK's menus have no names for the accessibility tree. A value
    is the string that the item gives the action."""
    if value is None:
        parameter = "[]"
    elif isinstance(value, bool):
        parameter = f"[<{str(value).lower()}>]"
    else:
        parameter = f"[<'{value}'>]"
    subprocess.run(
        [
            "gdbus", "call", "--session", "--dest", "br.com.brahm.Desk.Writer",
            "--object-path", "/br/com/brahm/Desk/Writer/window/1",
            "--method", "org.gtk.Actions.Activate", action, parameter, "{}",
        ],
        check=True,
        capture_output=True,
    )


def drawn_with_the_pen(work):
    """The pen draws a shape point by point, and a click on its first
    point closes it (OBJ-09)."""
    writer, app, letter = open_with_a_shape(work, "Pen.odt")
    try:
        press("Escape")
        activate("draw-with-pen")
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Pen.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        x, y = int(window["X"]), int(window["Y"])
        w, h = int(window["WIDTH"]), int(window["HEIGHT"])
        corners = [(w // 4, h // 3), (w // 2, h // 4), (w // 2, h // 2)]
        for cx, cy in corners + corners[:1]:
            click(x + cx, y + cy)
            time.sleep(0.4)
        press("ctrl+s")
        wait_for(
            "the saved shape drawn with the pen",
            lambda: saved_content(letter, r'desk:shape="path closed"'),
        )
        check("the pen draws a shape that a click on its first point closes (OBJ-09)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def editable_shape(work):
    """Make Editable turns a selected rectangle into a shape of points,
    and Smooth Path turns its corners into curves (OBJ-10)."""
    writer, app, letter = open_with_a_shape(work, "Editable.odt")
    try:
        wait_for(
            "the Metrics group of the Inspector",
            lambda: inspector_node(app, "Width"),
        )
        activate("make-editable")
        time.sleep(0.5)
        press("ctrl+s")
        wait_for(
            "the saved editable shape",
            lambda: saved_content(letter, r'desk:shape="path closed"'),
        )
        check("Make Editable turns the rectangle into points (OBJ-10)", True)
        activate("smooth-path")
        time.sleep(0.5)
        press("ctrl+s")
        wait_for(
            "the saved smooth shape",
            lambda: saved_content(letter, r'draw:enhanced-path="M [-\d]+ [-\d]+ C '),
        )
        check("Smooth Path turns its corners into curves (OBJ-10)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def connection_line(work):
    """A connection line follows the shape it joins when the shape moves,
    and is saved joined to it (OBJ-31)."""
    joined = SHAPE.replace(b"<draw:rect ", b'<draw:rect draw:id="a" ', 1) + (
        b'<draw:rect draw:id="b" draw:style-name="Filled" text:anchor-type="page" '
        b'text:anchor-page-number="1" '
        b'svg:x="450pt" svg:y="250pt" svg:width="100pt" svg:height="100pt"/>'
        b'<draw:connector text:anchor-type="page" text:anchor-page-number="1" '
        b'draw:start-shape="a" draw:end-shape="b" '
        b'svg:x1="396pt" svg:y1="300pt" svg:x2="450pt" svg:y2="300pt"/>'
    )
    writer, app, letter = open_with_a_shape(work, "Connection.odt", shape=joined)
    try:
        wait_for(
            "the Metrics group of the Inspector",
            lambda: inspector_node(app, "Width"),
        )
        # Ten points left, and the line's start with it.
        press("shift+Left")
        time.sleep(0.5)
        press("ctrl+s")

        def start():
            match = saved_content(
                letter, r'<draw:connector[^>]*draw:start-shape="[^"]+"[^>]*svg:x1="([\d.]+)pt"'
            ) or saved_content(
                letter, r'<draw:connector[^>]*svg:x1="([\d.]+)pt"[^>]*draw:start-shape="[^"]+"'
            )
            return match and float(match.group(1)) < 390

        wait_for("the saved connection line, moved with its shape", start)
        check("a connection line follows the shape it joins (OBJ-31)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def spin_of(app, title):
    """The spin button of the showing row titled `title`."""
    def has_spin(node):
        return any(n.get_role() == Atspi.Role.SPIN_BUTTON for n in walk(node))

    row = wait_for(f"the {title} row of the Inspector", lambda: inspector_node(app, title, has_spin))
    return next(node for node in walk(row) if node.get_role() == Atspi.Role.SPIN_BUTTON)


def position_of_a_turned_shape(work):
    """X and Y of the Metrics place the box that holds a turned shape
    (OBJ-29)."""
    writer, app, letter = open_with_a_shape(work, "Turned.odt")
    try:
        x = spin_of(app, "X")
        # The shape is 300 by 200 points, at 96 points from the left, shown
        # in the units of the preferences.
        per_point = Atspi.Value.get_current_value(x) / 96
        Atspi.Value.set_current_value(spin_of(app, "Rotate"), 90)
        # A quarter turn about its middle: the box that holds it is 200
        # points wide, 50 points further right.
        wait_for(
            "X of the turned shape",
            lambda: abs(Atspi.Value.get_current_value(x) / per_point - 146) < 0.5 or None,
        )
        check("X of a turned shape is where the box that holds it starts (OBJ-29)", True)
        Atspi.Value.set_current_value(x, 100 * per_point)
        time.sleep(0.5)
        press("ctrl+s")

        def moved():
            # ODF places the turned corner of the frame, here the bottom
            # left, which a quarter turn takes to the left of the box.
            found = saved_content(
                letter, r'<draw:rect[^>]*translate \(([-\d.]+(?:pt|in|cm|mm))'
            )
            return found and abs(points(found.group(1)) - 100) < 0.5

        wait_for("the saved shape, turned and moved", moved)
        check("a turned shape moves so that its box starts at the X typed in (OBJ-29)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)

def inventory_commands(work):
    """The four commands that the inventory of Pages found beyond its
    manuals: Reset Text and Object Handles squares a turned shape with the
    page (VMI-04), Reapply Defaults to Selection gives it back the look of
    a new shape (VMI-02), Show Connection Lines hides and shows the lines
    between linked text boxes (VMI-03), and Ctrl+Shift+A takes the
    selection away (VMI-01)."""
    name = "Inventory.odt"
    writer, app, letter = open_with_a_shape(work, name)
    try:
        Atspi.Value.set_current_value(spin_of(app, "Rotate"), 30)
        wait_for(
            "the shape to turn",
            lambda: abs(Atspi.Value.get_current_value(spin_of(app, "Rotate")) - 30) < 0.5,
        )
        activate("reset-text-handles")
        wait_for(
            "the shape squared with the page",
            lambda: abs(Atspi.Value.get_current_value(spin_of(app, "Rotate"))) < 0.5,
        )
        check("Reset Text and Object Handles takes the turn out of a shape (VMI-04)", True)
        activate("reapply-defaults")
        time.sleep(0.5)
        press("ctrl+s")
        wait_for(
            "the shape saved with the look of a new one",
            lambda: saved_content(letter, r'svg:stroke-color="#000000"')
            and not saved_content(letter, r"rotate \("),
        )
        check(
            "Reapply Defaults to Selection gives the shape the line of a new one (VMI-02)",
            True,
        )
        check("and the shape is saved square with the page (VMI-04)", True)

        activate("connection-lines")
        wait_for("the lines to hide", lambda: action_state("connection-lines") == "false")
        activate("connection-lines")
        wait_for("the lines to show", lambda: action_state("connection-lines") == "true")
        check("Show Connection Lines hides and shows the lines of linked boxes (VMI-03)", True)

        # Back in the text, everything selected and then nothing.
        press("Escape")
        document = find(app, Atspi.Role.TEXT, name)
        press("ctrl+a")
        wait_for("the text selected", lambda: Atspi.Text.get_n_selections(document) > 0)
        press("ctrl+shift+a")
        wait_for("nothing selected", lambda: Atspi.Text.get_n_selections(document) == 0)
        check("Ctrl+Shift+A takes the selection away (VMI-01)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def click_node(node, name):
    """Clicks the middle of a node of the window titled `name`."""
    geometry = subprocess.run(
        ["xdotool", "search", "--name", name, "getwindowgeometry", "--shell"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
    box = Atspi.Component.get_extents(node, Atspi.CoordType.WINDOW)
    click(int(window["X"]) + box.x + box.width / 2, int(window["Y"]) + box.y + box.height / 2)


def reveal(node, name):
    """Scrolls the node of the window titled `name` up into sight, with
    the wheel over it."""
    geometry = subprocess.run(
        ["xdotool", "search", "--name", name, "getwindowgeometry", "--shell"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
    for _ in range(20):
        box = Atspi.Component.get_extents(node, Atspi.CoordType.WINDOW)
        if box.y + box.height <= int(window["HEIGHT"]) - 60:
            return
        x = int(window["X"]) + box.x + box.width // 2
        y = int(window["Y"]) + int(window["HEIGHT"]) // 2
        subprocess.run(["xdotool", "mousemove", "--sync", str(x), str(y), "click", "5"], check=True)
        time.sleep(0.3)


def choose_in_list(app, title, name, keys):
    """Opens the list of the first showing row titled `title`, in the
    window titled `name`, and picks an item with keys."""
    row = wait_for(
        f"the {title} row",
        lambda: inspector_node(app, title, lambda n: n.get_role() == Atspi.Role.COMBO_BOX),
    )
    reveal(row, name)
    click_node(row, name)
    time.sleep(1)
    press(*keys)
    time.sleep(1)


def saved_part(letter, part, pattern):
    """Whether a part of the saved package matches a pattern."""
    try:
        with zipfile.ZipFile(letter) as package:
            content = package.read(part).decode()
    except (zipfile.BadZipFile, KeyError, OSError):
        return False
    return re.search(pattern, content)


def master_objects(work):
    """Arrange, Move to Section Master puts a shape on the master of its
    section, saved in the master page; it is selected again only once
    master objects are made selectable, and goes back to the page with
    Move to Page (INT-05, EST-21)."""
    writer, app, letter = open_with_a_shape(work, "Master.odt")
    try:
        def width_row():
            return inspector_node(app, "Width")

        wait_for("the Metrics group of the Inspector", width_row)
        activate("move-to-master")
        wait_for("the shape no longer selected", lambda: width_row() is None)
        press("ctrl+s")
        wait_for(
            "the shape saved in the master page",
            lambda: saved_part(letter, "styles.xml", r'<style:master-page[^>]*>.*<draw:rect '),
        )
        check("a shape moved to the master is saved in the master page (INT-05)", True)
        check(
            "and no longer on the page",
            not saved_content(letter, r"<draw:rect "),
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Master.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        spot = (
            int(window["X"]) + int(window["WIDTH"]) // 3,
            int(window["Y"]) + int(window["HEIGHT"]) * 3 // 4,
        )
        click(*spot)
        time.sleep(1)
        check("a master object is not selected on the page (EST-21)", width_row() is None)
        activate("masters-selectable")
        click(*spot)
        wait_for("the master object selected", width_row)
        check("once master objects are selectable, a click selects one (EST-21)", True)
        activate("move-to-page")
        press("ctrl+s")
        wait_for(
            "the shape saved on the page again",
            lambda: saved_content(letter, r'<draw:rect [^>]*text:anchor-type="page"'),
        )
        check("Move to Page puts it back on the page (EST-21)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


PAGE_LAYOUT_SETTINGS = (
    b'<?xml version="1.0" encoding="UTF-8"?>'
    b'<office:document-settings '
    b'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
    b'xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0" office:version="1.3">'
    b'<office:settings><config:config-item-set config:name="DeskTextSettings">'
    b'<config:config-item config:name="PageLayoutPages" config:type="int">1</config:config-item>'
    b"</config:config-item-set></office:settings></office:document-settings>"
)


def page_layout(work):
    """A page layout document takes no typing in its body, and a page
    break adds a page to it, saved with the count of its pages (INT-03)."""
    writer, app, letter = open_with_a_shape(
        work, "Layout.odt", extra={"settings.xml": PAGE_LAYOUT_SETTINGS}
    )
    try:
        document = find(app, Atspi.Role.TEXT, "Layout.odt")
        before = text_of(document)
        check("a page layout document opens with no body text (INT-03)", before.strip() == "")
        wait_for(
            "the thumbnail of the page",
            lambda: any(n.get_name() == "Page 1" and showing(n) for n in walk(app)),
        )
        check("and with the thumbnails of its pages (INT-03)", True)
        press("Escape")
        type_text("Hello")
        press("Return")
        time.sleep(1)
        check("typing puts no text in its body (INT-03)", text_of(document) == before)
        activate("insert-page-break")
        press("ctrl+s")
        wait_for(
            "the document saved with two pages",
            lambda: saved_part(
                letter,
                "settings.xml",
                r'config:name="PageLayoutPages" config:type="int">2<',
            ),
        )
        check("a page break adds a page to it (INT-03)", True)
        check("and its body still has no text", not saved_content(letter, r"Hello"))
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def copies_of_a_shape(work):
    """Duplicate and Copy and Paste put copies of a selected shape on the
    page, each a little lower and to the right, and they are saved with it
    (OBJ-22)."""
    writer, app, letter = open_with_a_shape(work, "Copies.odt")
    try:
        press("ctrl+d")
        time.sleep(0.5)
        press("ctrl+c")
        press("ctrl+v")
        time.sleep(1)
        press("ctrl+s")
        def corners():
            content = saved_content(letter, r"<draw:rect")
            if not content:
                return None
            with zipfile.ZipFile(letter) as package:
                text = package.read("content.xml").decode()
            rects = re.findall(r"<draw:rect[^>]*>", text)
            if len(rects) != 3:
                return None
            return sorted(
                (
                    round(points(re.search(r'svg:x="([^"]+)"', r).group(1))),
                    round(points(re.search(r'svg:y="([^"]+)"', r).group(1))),
                )
                for r in rects
            )

        found = wait_for("the shape and its two copies saved", corners)
        check(
            "Duplicate and Paste put copies of the shape ten points lower and to the right (OBJ-22)",
            found == [(96, 200), (106, 210), (116, 220)],
        )
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def objects_between_apps(work):
    """A shape copied in one app of Desk pastes in another, a process of
    its own, through the format that the apps share, and keeps its origin
    (IOP-27, IOP-28, IOP-33)."""
    writer, app, letter = open_with_a_shape(work, "Source.odt")
    other = None
    try:
        press("ctrl+c")
        time.sleep(0.5)
        target = work / "Target.odt"
        subprocess.run([str(CONVERT), str(FIXTURE), str(target)], check=True)
        # A bus of its own, so that it is another process and not a
        # window of the first.
        other = subprocess.Popen(
            ["dbus-run-session", "--", str(WRITER), str(target)],
            start_new_session=True,
            # Off the accessibility bus, which the other tests read.
            env={**os.environ, "GTK_A11Y": "none", "NO_AT_BRIDGE": "1"},
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Target.odt", "windowfocus", "--sync"],
            check=True,
        )
        time.sleep(1)
        press("ctrl+v")
        time.sleep(1)
        press("ctrl+s")
        found = wait_for(
            "the pasted shape saved in the other app",
            lambda: saved_content(target, r'<draw:rect[^>]*desk:origin="[0-9a-f]+"[^>]*svg:x="96pt"'),
        )
        check("a shape copied in one app of Desk pastes in another (IOP-27, IOP-28)", bool(found))
    finally:
        if other is not None:
            os.killpg(other.pid, signal.SIGTERM)
            other.wait(timeout=10)
        writer.terminate()
        writer.wait(timeout=10)


def styled_text_between_apps(work):
    """Text copied with its paragraph style in one app of Desk pastes in
    another, a process of its own, with that style (IOP-17, IOP-25)."""
    source = work / "StyledSource.odt"
    subprocess.run([str(CONVERT), str(FIXTURE), str(source)], check=True)
    with zipfile.ZipFile(source) as package:
        files = {n: package.read(n) for n in package.namelist()}
    files["styles.xml"] = files["styles.xml"].replace(
        b"<office:styles>",
        b'<office:styles><style:style style:name="Clip_20_Style" style:display-name="Clip Style" '
        b'style:family="paragraph"><style:text-properties fo:font-weight="bold"/></style:style>',
        1,
    )
    files["content.xml"] = files["content.xml"].replace(
        b"<office:text>",
        b'<office:text><text:p text:style-name="Clip_20_Style">Styled words</text:p>',
        1,
    )
    with zipfile.ZipFile(source, "w") as package:
        package.writestr(zipfile.ZipInfo("mimetype"), files.pop("mimetype"))
        for n, data in files.items():
            package.writestr(n, data, zipfile.ZIP_DEFLATED)
    writer = subprocess.Popen([str(WRITER), str(source)])
    other = None
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for(
            "the document in the accessibility tree",
            lambda: find(app, Atspi.Role.TEXT, "StyledSource.odt"),
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "StyledSource.odt", "windowfocus", "--sync"],
            check=True,
        )
        # The first paragraph, with its end.
        press("ctrl+Home", "shift+Down", "ctrl+c")
        time.sleep(0.5)
        target = work / "StyledTarget.odt"
        subprocess.run([str(CONVERT), str(FIXTURE), str(target)], check=True)
        # A bus of its own, so that it is another process and not a
        # window of the first.
        other = subprocess.Popen(
            ["dbus-run-session", "--", str(WRITER), str(target)],
            start_new_session=True,
            env={**os.environ, "GTK_A11Y": "none", "NO_AT_BRIDGE": "1"},
        )
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "StyledTarget.odt", "windowfocus", "--sync"],
            check=True,
        )
        time.sleep(1)
        # In a paragraph of its own at the end.
        press("ctrl+End", "Return", "ctrl+v")
        time.sleep(1)
        press("ctrl+s")
        found = wait_for(
            "the pasted text saved with its style in the other app",
            lambda: saved_content(target, r'<text:p text:style-name="Clip_20_Style">Styled words'),
        )
        check(
            "text copied in one app of Desk pastes in another with its paragraph style (IOP-17, IOP-25)",
            bool(found),
        )
    finally:
        if other is not None:
            os.killpg(other.pid, signal.SIGTERM)
            other.wait(timeout=10)
        writer.terminate()
        writer.wait(timeout=10)

def guide_from_the_ruler(work):
    """A drag from the ruler down onto the page leaves a guide across it,
    saved with the document (OBJ-28); the layout view turns on and off
    (INT-09)."""
    writer, app, letter = open_with_a_shape(work, "Guides.odt")
    try:
        activate("show-layout")
        ruler = wait_for("the ruler", lambda: showing_named(app, "Ruler"))
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Guides.odt", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        box = Atspi.Component.get_extents(ruler, Atspi.CoordType.WINDOW)
        x = int(window["X"]) + box.x + box.width * 2 // 5
        y = int(window["Y"]) + box.y + box.height // 2
        subprocess.run(["xdotool", "mousemove", "--sync", str(x), str(y), "mousedown", "1"], check=True)
        for step in range(1, 11):
            subprocess.run(["xdotool", "mousemove", "--sync", str(x), str(y + step * 15)], check=True)
            time.sleep(0.05)
        subprocess.run(["xdotool", "mouseup", "1"], check=True)
        time.sleep(0.5)
        # Hide Layout hides the rulers too, as in Pages (INT-09).
        activate("show-layout")
        wait_for("the ruler hidden with the layout", lambda: showing_named(app, "Ruler") is None)
        press("ctrl+s")
        wait_for(
            "the guide saved in the settings",
            lambda: saved_part(letter, "settings.xml", r'config:name="Guides"[^>]*>h [0-9.]+(pt|in|cm|mm) 1<'),
        )
        check("a drag from the ruler leaves a guide on its page, saved with it (OBJ-28)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def screen_view(work):
    """Screen View shows the text in one flow as wide as the window,
    without pages, page breaks or rulers, and View ▸ Screen View again
    shows the pages (PXV-01). The table of contents of the sidebar
    follows the headings as they are typed (PXV-02, W041)."""
    name = "Screen.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    with zipfile.ZipFile(letter) as package:
        files = {n: package.read(n) for n in package.namelist()}
    files["content.xml"] = files["content.xml"].replace(
        b"<office:text>",
        b'<office:text><text:h text:style-name="Heading_20_1" text:outline-level="1">'
        b"Chapter</text:h>",
        1,
    )
    with zipfile.ZipFile(letter, "w") as package:
        package.writestr(zipfile.ZipInfo("mimetype"), files.pop("mimetype"))
        for n, data in files.items():
            package.writestr(n, data, zipfile.ZIP_DEFLATED)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        # The table of contents of the sidebar changes as a heading is
        # typed (W041).
        activate("contents")
        wait_for("the heading in the sidebar", lambda: showing_named(app, "Chapter"))
        press("ctrl+Home")
        type_text("First ")
        wait_for("the heading typed in the sidebar", lambda: showing_named(app, "First Chapter"))
        check("the table of contents of the sidebar follows the text as it is typed (W041)", True)
        activate("contents")
        press("ctrl+End")
        activate("insert-page-break")
        type_text("Second page")
        wait_for("the text on the second page", lambda: text_of(document).endswith("Second page\n"))

        def height():
            return Atspi.Component.get_extents(document, Atspi.CoordType.WINDOW).height

        # Two pages, one under the other, are taller than the window.
        wait_for("two pages", lambda: height() > 1500)
        wait_for("the ruler", lambda: showing_named(app, "Ruler"))
        press("ctrl+shift+r")
        wait_for("the text in one flow, without the ruler", lambda: showing_named(app, "Ruler") is None)
        wait_for("the flow as high as the window", lambda: height() < 1200)
        # The text is edited as on the pages.
        type_text(" flows")
        wait_for("the text typed in the flow", lambda: text_of(document).endswith("Second page flows\n"))
        activate("text-size", "bigger")
        activate("screen-view")
        wait_for("the pages again", lambda: height() > 1500)
        wait_for("the ruler again", lambda: showing_named(app, "Ruler"))
        check("Screen View flows the text without pages, and the pages come back (PXV-01)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def pages_as_sections(work):
    """In a page layout document each page is a section of its own: the
    thumbnails select a page, and Duplicate copies it with its objects
    after it (EST-16, EST-04)."""
    writer, app, letter = open_with_a_shape(
        work, "Sections.odt", extra={"settings.xml": PAGE_LAYOUT_SETTINGS}
    )
    try:
        thumbnail = wait_for("the thumbnail of the page", lambda: showing_named(app, "Page 1"))
        click_node(thumbnail, "Sections.odt")
        press("ctrl+d")
        wait_for(
            "the thumbnail of the copy",
            lambda: showing_named(app, "Page 2"),
        )
        press("ctrl+s")
        wait_for(
            "the document saved with two pages",
            lambda: saved_part(
                letter,
                "settings.xml",
                r'config:name="PageLayoutPages" config:type="int">2<',
            ),
        )
        check(
            "Duplicate in the thumbnails copies a page with its shape (EST-16)",
            len(re.findall(r"<draw:rect", zipfile.ZipFile(letter).read("content.xml").decode())) == 2,
        )
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def reflection_of_a_shape(work):
    """The Graphic group of the Inspector reflects a selected shape below
    it, at half its opacity, and the reflection is saved (OBJ-38)."""
    writer, app, letter = open_with_a_shape(work, "Reflection.odt")
    try:
        toggle_switch_row(app, "Reflection", "the Graphic group of the Inspector")
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved reflection",
            lambda: saved_content(letter, r'desk:reflection="50%"'),
        )
        check("the reflection of the shape is saved (OBJ-38)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)



def advanced_gradient(work):
    """The Graphic group of the Inspector fills a selected shape with an
    advanced gradient, adds a color halfway along it, and the colors are
    saved (OBJ-41)."""
    writer, app, letter = open_with_a_shape(work, "Gradient.odt")
    try:
        # The list of fills opens, and its last one is the advanced
        # gradient.
        choose_in_list(app, "Fill", "Gradient.odt", ["End", "Return"])
        add = wait_for("the Add Color button", lambda: showing_named(app, "Add Color"))
        Atspi.Action.do_action(add, 0)
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved gradient of three colors",
            lambda: saved_part(
                letter,
                "styles.xml",
                r'<draw:gradient [^>]*><desk:gradient-stop svg:offset="0"[^>]*/>'
                r'<desk:gradient-stop svg:offset="0.5"[^>]*/><desk:gradient-stop svg:offset="1"',
            ),
        )
        check("a color added to an advanced gradient is saved (OBJ-41)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)



def mask_and_frame(work):
    """The Image group of the Inspector masks a selected picture with an
    oval, the Graphic group puts a picture frame around it, and both are
    saved (OBJ-04, OBJ-36)."""
    picture = (
        b'<draw:frame text:anchor-type="page" text:anchor-page-number="1" '
        b'svg:x="96pt" svg:y="200pt" svg:width="300pt" svg:height="200pt">'
        b'<draw:image xlink:href="Pictures/gray.png" xlink:type="simple"/></draw:frame>'
    )
    writer, app, letter = open_with_a_shape(
        work,
        "Masked.odt",
        picture,
        {"Pictures/gray.png": png(16, 16, (128, 128, 128))},
    )
    try:
        toggle_switch_row(app, "Mask", "the Image group of the Inspector")
        time.sleep(1)
        # The third shape of the list is the oval.
        choose_in_list(app, "Mask Shape", "Masked.odt", ["Home", "Down", "Down", "Return"])
        # The picture frame is the last of the lines.
        choose_in_list(app, "Stroke", "Masked.odt", ["End", "Return"])
        press("ctrl+s")
        wait_for(
            "the saved mask and frame",
            lambda: saved_content(letter, r'desk:mask-shape="ellipse"')
            and saved_content(letter, r'desk:picture-frame="photo"'),
        )
        check("a picture masked with an oval and framed is saved (OBJ-04, OBJ-36)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)

def mask_from_the_menu(work):
    """Mask with Shape of the Arrange menu masks the selected picture with
    a star, and Unmask takes the mask off again (OBJ-04)."""
    picture = (
        b'<draw:frame text:anchor-type="page" text:anchor-page-number="1" '
        b'svg:x="96pt" svg:y="200pt" svg:width="200pt" svg:height="200pt">'
        b'<draw:image xlink:href="Pictures/gray.png" xlink:type="simple"/></draw:frame>'
    )
    writer, app, letter = open_with_a_shape(
        work,
        "Starred.odt",
        picture,
        {"Pictures/gray.png": png(16, 16, (128, 128, 128))},
    )
    try:
        activate("mask-with-shape", "star")
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved star mask",
            lambda: saved_content(letter, r'desk:mask-shape="star \d+ \d+"'),
        )
        check("Mask with Shape of the menu masks a picture with a star (OBJ-04)", True)
        activate("unmask")
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved picture without a mask",
            lambda: saved_content(letter, r"<draw:image")
            and not saved_content(letter, r"desk:mask"),
        )
        check("Unmask takes the mask off the picture (OBJ-04)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)



def extents(node):
    """Where a node is in its window."""
    return Atspi.Component.get_extents(node, Atspi.CoordType.WINDOW)


def scroll_pane_of(node):
    """The scrolled pane that shows a node: the part of the window that
    the pages take."""
    while node is not None and node.get_role() != Atspi.Role.SCROLL_PANE:
        node = node.get_parent()
    return node


def right_of(node):
    """The right edge of a node in its window."""
    box = extents(node)
    return box.x + box.width


def gnome_window(work):
    """The window as the decisions of 13/11 adapt it to GNOME: a header bar
    without customization and the main menu (INT-11), the shortcuts
    dialog of Libadwaita (GNM-11), Ctrl+H for Replace (TXT-12), the
    character styles in the Format pane (STY-02), the language of the text
    in the Inspector (TXT-10), Favorites in the Fonts popover (INT-15),
    and a narrow window that keeps what matters most (GNM-01); and the
    skeleton of interface B: the navigation sidebar as high as the window
    (GNM-12), and the sidebar and then the Inspector covering the pages in
    a narrow window (GNM-01)."""
    name = "Gnome.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        wait_for("the main menu", lambda: showing_named(app, "Main Menu"))
        check(
            "the header bar has the main menu and nothing to customize it (INT-11)",
            showing_named(app, "Customize Toolbar") is None,
        )

        press("ctrl+question")
        wait_for(
            "the shortcuts dialog",
            lambda: showing_named(app, "Apply the style of the key"),
        )
        check("Ctrl+? shows the shortcuts of Libadwaita (GNM-11)", True)
        press("Escape")

        press("ctrl+h")
        wait_for("the replacement field", lambda: showing_named(app, "Show Replace"))
        wait_for("Replace All", lambda: showing_named(app, "Replace All"))
        check("Ctrl+H shows Find & Replace with the replacement (TXT-12)", True)
        press("Escape")

        press("ctrl+alt+i")
        check(
            "the Inspector sets the language of the text (TXT-10)",
            wait_for("the language", lambda: inspector_node(app, "Language")) is not None,
        )
        press("ctrl+alt+i")

        activate("fonts")
        wait_for("the Fonts popover", lambda: showing_named(app, "Favorites"))
        check("the Fonts popover has Favorites and its star (INT-15)", showing_named(app, "Favorite") is not None)
        press("Escape")
        # The popover opens on the Family row of the Format pane, which
        # stays open after it (step 2 of M+).
        check("win.fonts opens the Format pane", inspector_node(app, "Font Family") is not None)
        press("ctrl+alt+i")
        wait_for("the Inspector to close", lambda: inspector_node(app, "Font Family") is None)

        document = find(app, Atspi.Role.TEXT, name)
        start = extents(document).x
        press("F9")
        add = wait_for("the Add Page button of the sidebar", lambda: showing_named(app, "Add Page"))
        check(
            "F9 shows the navigation sidebar with its content button on Pages (GNM-12)",
            showing_named(app, "Pages") is not None,
        )
        check(
            "the sidebar is as high as the window, beside the header bar (GNM-12)",
            abs(extents(add).y - extents(showing_named(app, "Main Menu")).y) < 12,
        )
        wait_for("the sidebar to push the pages", lambda: extents(document).x >= start + 150)
        check("the sidebar pushes the pages in a wide window (GNM-12)", True)
        press("F9")
        wait_for("the sidebar to hide", lambda: showing_named(app, "Add Page") is None)

        subprocess.run(
            ["xdotool", "search", "--name", name, "windowsize", "640", "600"],
            check=True,
        )
        wait_for("the main menu in the narrow window", lambda: showing_named(app, "Main Menu"))
        check("a narrow window keeps the main controls (GNM-01)", True)
        zoom, _ = wait_for("the zoom", lambda: named_like(app, r"\d+%"))
        count, _ = wait_for("the count", lambda: named_like(app, r"[\d,]+ words?"))
        check(
            "in a narrow window the count sits over the zoom (GNM-13)",
            extents(count).y + extents(count).height <= extents(zoom).y
            and abs(right_of(count) - right_of(showing_named(app, "Zoom In"))) < 40,
        )

        start = extents(document).x
        press("F9")
        wait_for("the sidebar in the narrow window", lambda: showing_named(app, "Add Page"))
        check("below 720 px the sidebar covers the pages (GNM-01)", extents(document).x == start)
        press("F9")
        wait_for("the sidebar to hide", lambda: showing_named(app, "Add Page") is None)

        pages = scroll_pane_of(document)
        press("ctrl+alt+i")
        language = wait_for("the Inspector", lambda: inspector_node(app, "Language"))
        check(
            "at 640 px, half of a 1280 × 720 screen, the Inspector sits beside the pages (GNM-01)",
            right_of(pages) <= extents(language).x,
        )
        subprocess.run(
            ["xdotool", "search", "--name", name, "windowsize", "500", "600"],
            check=True,
        )
        # Below 520 px the Inspector closes, and comes back over the pages.
        wait_for("the Inspector to close below 520 px", lambda: inspector_node(app, "Language") is None)
        press("ctrl+alt+i")
        language = wait_for("the Inspector over the pages", lambda: inspector_node(app, "Language"))
        check(
            "below 520 px the Inspector covers the pages (GNM-01)",
            right_of(pages) > extents(language).x,
        )
        press("ctrl+alt+i")
    finally:
        writer.terminate()
        writer.wait(timeout=10)


# The buttons of the header bar of interface B, by their names, and the
# ones a window narrower than 720 px puts in the Insert menu (INT-11).
HEADER = ("Sidebar", "Insert", "Find", "Inspector", "Open Documents", "Main Menu")
OBJECTS = ("Table", "Chart", "Text Box", "Shape", "Media", "Comment")


def header_and_menus(work):
    """The header bar of interface B: the Sidebar button, Insert and the
    six objects at the start, Find, the Inspector and the main menu at the
    end (INT-11); the popover of Media with the Media Browser (INT-14);
    Search Commands, which finds every command by name (GNM-10); and no
    warning on a screen of 1280 × 720 or more (GNM-14)."""
    name = "Header.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        wait_for("the Sidebar button", lambda: showing_named(app, "Sidebar"))
        missing = [b for b in HEADER + OBJECTS if showing_named(app, b) is None]
        check(
            "the header bar has Sidebar, Insert, the six objects, Find, the Inspector and the menu (INT-11)",
            not missing,
        )
        sidebar = showing_named(app, "Sidebar")
        objects = [extents(showing_named(app, b)).x for b in ("Insert",) + OBJECTS]
        check(
            "the objects follow Insert at the start of the header bar (INT-11)",
            extents(sidebar).x < objects[0] and objects == sorted(objects),
        )
        check(
            "a screen of 1280 × 720 or more has no warning (GNM-14)",
            warning_of_small_screen(app) is None,
        )

        # Find shows the find bar, and is down while it shows.
        Atspi.Action.do_action(showing_named(app, "Find"), 0)
        wait_for("the find bar", lambda: showing_named(app, "Show Replace"))
        check("Find in the header bar shows the find bar (INT-11)", True)
        press("Escape")
        wait_for("the find bar to hide", lambda: showing_named(app, "Show Replace") is None)

        # The button inside the menu button of Media has the action.
        media = next(
            n
            for n in walk(app)
            if n.get_name() == "Media" and n.get_role() == Atspi.Role.TOGGLE_BUTTON and showing(n)
        )
        Atspi.Action.do_action(media, 0)
        wait_for("the popover of Media", lambda: showing_named(app, "Choose File…"))
        check(
            "Media opens a popover with the Media Browser and Choose File (INT-14)",
            showing_named(app, "Media Browser") is not None,
        )
        press("Escape")
        wait_for("the popover to close", lambda: showing_named(app, "Choose File…") is None)

        # Ctrl+/ searches the commands; Enter runs the first found.
        press("ctrl+slash")
        wait_for("the command search", lambda: showing_named(app, "Search Commands"))
        type_text("save as")
        wait_for("Save As in the search", lambda: showing_named(app, "Save As…"))
        check("Search Commands finds the commands of the main menu (GNM-10)", True)
        press("ctrl+a")
        type_text("page break")
        wait_for("Page Break in the search", lambda: showing_named(app, "Page Break"))
        check(
            "Search Commands finds what is in submenus, and says where (GNM-10)",
            showing_named(app, "Save As…") is None,
        )
        press("Return")
        wait_for("the command search to close", lambda: showing_named(app, "Search Commands") is None)
        # The count over the document counts the pages, now two.
        activate("count", "pages")
        wait_for("the page break", lambda: showing_named(app, "2 pages"))
        check("a command found runs with Enter (GNM-10)", True)
        activate("count", "words")

        # Documents open as tabs of the window (W013, D-023, D-024): the
        # tab bar shows from the second tab, Ctrl+Page Up goes back,
        # Ctrl+W closes the tab and the window stays.
        check("one document shows no tab bar", not document_tabs(app))
        press("ctrl+t")
        wait_for("the second tab", lambda: len(document_tabs(app)) == 2)
        check("Ctrl+T opens a new document in a tab (D-024)", window_named("Untitled"))
        press("ctrl+Page_Up")
        wait_for("the first tab", lambda: window_named(name))
        check("Ctrl+Page Up shows the tab before, and the window takes its title (D-023)", True)
        press("ctrl+Page_Down")
        wait_for("the new tab", lambda: window_named("Untitled"))
        # A new document has no file to save itself to, so it asks to be
        # saved when its tab closes (DOC-11, DOC-18).
        press("ctrl+w")
        wait_for("the question to save", lambda: showing_named(app, "Save Changes?"))
        Atspi.Action.do_action(showing_named(app, "Discard"), 0)
        wait_for("the tab to close", lambda: not document_tabs(app))
        check("Ctrl+W closes the tab and leaves the window (D-023)", window_named(name))
        Atspi.Action.do_action(showing_named(app, "Open Documents"), 0)
        wait_for("the tab overview", lambda: showing_named(app, "New Tab"))
        check("Open Documents shows the open documents (GNM-01)", True)
        press("Escape")
        wait_for("the tab overview to close", lambda: showing_named(app, "New Tab") is None)

        subprocess.run(
            ["xdotool", "search", "--name", name, "windowsize", "700", "600"],
            check=True,
        )
        wait_for("the objects to leave the header bar", lambda: showing_named(app, "Table") is None)
        time.sleep(1)
        missing = [b for b in HEADER if showing_named(app, b) is None]
        check(
            "below 720 px the header bar keeps Insert and leaves out the objects (INT-11)",
            not missing,
        )
    finally:
        writer.terminate()
        writer.wait(timeout=10)


# What the Format pane of text shows in its Style tab, and what the Arrange
# tab of an object shows, in place of the format bar (step 2 of M+).
TEXT_PANE = ("Paragraph Style", "Font Family", "Size", "Bold", "Italic", "Underline",
             "Strikethrough", "Text Color", "Character Style", "Align Left")
ARRANGE_PANE = ("Bring to Front", "Send to Back", "Arrange")


def action_state(name):
    """The state of an action of the first window of Desk Writer, as GTK
    describes it on the session bus, as text: "true", "'styles'"…"""
    out = subprocess.run(
        [
            "gdbus", "call", "--session", "--dest", "br.com.brahm.Desk.Writer",
            "--object-path", "/br/com/brahm/Desk/Writer/window/1",
            "--method", "org.gtk.Actions.Describe", name,
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    # (true, '', [<true>],)
    inside = out[out.rfind("[") + 1 : out.rfind("]")]
    return inside.strip().removeprefix("<").removesuffix(">")


# The pages of the navigation sidebar and the actions that show them.
SIDEBAR_PAGES = (("pages", "thumbnails"), ("styles", "styles-drawer"),
                 ("comments", "comments"), ("search", "search"))


def shown_pages():
    """The pages of the sidebar whose actions say they show."""
    return [page for page, action in SIDEBAR_PAGES if action_state(action) == "true"]


def sidebar_pages(work):
    """The navigation sidebar of interface B shows the page thumbnails,
    the styles that were the Styles drawer, the comments and the search
    results, one at a time; the View menu and their shortcuts choose them,
    and F9 hides and shows the sidebar with the page it had (INT-18,
    TXT-13, GNM-12). A comment is written there, and shows in a bubble at
    its text (W149)."""
    name = "Sidebar.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        press("ctrl+shift+t")
        wait_for("the styles in the sidebar", lambda: shown_pages() == ["styles"])
        wait_for("the paragraph styles", lambda: showing_named(app, "Paragraph Styles"))
        check("Ctrl+Shift+T shows the Styles page of the sidebar, not a drawer (INT-18)", True)
        activate("comments")
        wait_for("the comments in the sidebar", lambda: shown_pages() == ["comments"])
        check(
            "the Comments page takes the place of the styles",
            showing_named(app, "No Comments") is not None
            and showing_named(app, "Paragraph Styles") is None,
        )
        # Ctrl+Shift+K comments on the selection, in a card of the page
        # that takes the keyboard; Ctrl+Return is done with it, and it is
        # saved as an annotation on its text (W149, REV-08).
        press("ctrl+a", "ctrl+shift+k")
        wait_for("the card of the new comment", lambda: showing_named(app, "New Comment"))
        type_text("Why landscape?")
        press("ctrl+Return")
        wait_for(
            "the card of the comment",
            lambda: named_like(app, r"Comment by .+") and showing_named(app, "Why landscape?"),
        )
        check("Ctrl+Shift+K writes a comment on the selection in the sidebar (W149)", True)
        press("ctrl+s")
        wait_for(
            "the saved comment",
            lambda: saved_content(
                letter,
                r"<office:annotation office:name=\"c1\">.*Why landscape\?.*</office:annotation>US Letter"
                r".*<office:annotation-end office:name=\"c1\"/>",
            ),
        )
        check("and it is saved on its text (REV-08)", True)
        press("ctrl+alt+f")
        wait_for("the search results in the sidebar", lambda: shown_pages() == ["search"])
        check(
            "Ctrl+Alt+F shows the Search Results page of the sidebar (TXT-13)",
            showing_named(app, "Search Results") is not None,
        )
        activate("sidebar")
        wait_for("the sidebar to hide", lambda: action_state("sidebar") == "false")
        check("the Sidebar button hides the sidebar (GNM-12)", shown_pages() == [])
        activate("sidebar")
        wait_for("the sidebar to show", lambda: action_state("sidebar") == "true")
        check("and shows it again with the page it had (GNM-12)", shown_pages() == ["search"])
        activate("thumbnails")
        wait_for("the thumbnails", lambda: shown_pages() == ["pages"])
        wait_for(
            "the thumbnail of the first page",
            lambda: any(n.get_name() == "Page 1" and showing(n) for n in walk(app)),
        )
        check("Page Thumbnails turns the sidebar back to the pages (INT-19)", True)
        # Without the Comments page, Next Comment shows the comment in a
        # bubble at its text, and Escape takes it away (W149).
        press("ctrl+Home", "ctrl+alt+k")
        wait_for(
            "the bubble of the comment",
            lambda: showing_named(app, "Previous Comment") and showing_named(app, "Why landscape?"),
        )
        check("Next Comment shows the comment in a bubble at its text (W149)", True)
        press("Escape")
        wait_for("the bubble to go", lambda: showing_named(app, "Why landscape?") is None)
        check("and Escape takes the bubble away", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


LONG_DOCUMENT = ROOT / "corpus" / "fixtures" / "pages" / "long-document.pages"


def named_like(app, pattern):
    """The first node that shows with a name matching a pattern, and the
    match."""
    try:
        for node in walk(app):
            found = re.fullmatch(pattern, node.get_name() or "")
            if found and showing(node):
                return node, found
    except GLib.GError:
        pass
    return None


def open_menu_button(node):
    """Opens a menu button: the toggle that it holds takes the action."""
    for part in walk(node):
        try:
            if Atspi.Action.get_n_actions(part) > 0:
                part.do_action(0)
                return
        except (GLib.GError, NotImplementedError, TypeError):
            continue
    fail(f"{node.get_name()} has nothing to press")


def go_to_page_field(app):
    """Opens Go to Page in the View menu of the main menu, by the keys of
    its mnemonics, and gives its field."""
    press("F10")
    wait_for("the main menu", lambda: showing_named(app, "Main Menu"))
    time.sleep(0.5)
    press("v")
    return wait_for(
        "Go to Page",
        lambda: next(
            (
                n
                for n in walk(app)
                if n.get_role() == Atspi.Role.TEXT and n.get_name() == "Go to Page" and showing(n)
            ),
            None,
        ),
    )


def document_controls(work):
    """The controls over the document of window M+, in place of the bottom
    bar (step 3): the zoom between Zoom Out and Zoom In over the end
    corner, the count over the foot of the document, of the document or
    of the selection when text is selected, whose menu chooses what it
    counts and shows the statistics; Go to Page and Navigate By in the
    View menu (GNM-13, INT-07, INT-19, DOC-20)."""
    name = "Long.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(LONG_DOCUMENT), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        zoom, _ = wait_for("the zoom", lambda: named_like(app, r"\d+%"))
        count, found = wait_for("the count", lambda: named_like(app, r"([\d,]+) words"))
        words = int(found.group(1).replace(",", ""))
        pages = scroll_pane_of(document)
        check(
            "the zoom and the count sit over the document, the zoom at the end and the count in the middle (GNM-13)",
            words > 1000
            and showing_named(app, "Zoom Out") is not None
            and showing_named(app, "Zoom In") is not None
            and right_of(count) < extents(zoom).x
            and extents(pages).x < extents(count).x
            and right_of(zoom) <= right_of(pages)
            and extents(zoom).y + extents(zoom).height <= extents(pages).y + extents(pages).height,
        )
        check("there is no bottom bar (GNM-13)", named_like(app, r"Page \d+ of \d+") is None)
        activate("zoom", "1.5")
        wait_for("the zoom to change", lambda: showing_named(app, "150%"))
        check("the zoom over the document shows the zoom chosen (INT-07)", True)
        Atspi.Action.do_action(showing_named(app, "Zoom In"), 0)
        wait_for("the zoom to grow", lambda: named_like(app, r"(1[6-9]\d|[2-9]\d\d)%"))
        check("Zoom In of the pill makes the pages bigger (INT-07)", True)

        field = go_to_page_field(app)
        check("Go to Page is in the View menu (INT-19)", text_of(field) == "1")
        # Its label takes the focus to it by its mnemonic, G.
        press("g")
        wait_for("the focus in Go to Page", lambda: field.get_state_set().contains(Atspi.StateType.FOCUSED))
        press("ctrl+a")
        type_text("6")
        press("Return")
        wait_for("the menu to close", lambda: showing_named(app, "Go to Page") is None)
        field = go_to_page_field(app)
        check("Go to Page shows the page typed (INT-19)", text_of(field) == "6")
        press("Escape")
        press("Escape")
        activate("navigate", True)
        time.sleep(0.5)
        field = go_to_page_field(app)
        check("Next Page goes to the next page (INT-19)", text_of(field) == "7")
        press("Escape")
        press("Escape")
        activate("navigate-by", "section")
        check("Navigate By turns the steps to another kind of place (INT-19)", action_state("navigate-by") == "'section'")
        activate("navigate-by", "page")

        # The main menu gave the focus back to its button: a click puts it
        # in the text again.
        area = extents(scroll_pane_of(document))
        click(area.x + area.width // 2, area.y + area.height // 3)
        # The first word selected: its count alone, with no total, as
        # Pages 15.4 shows it.
        press("ctrl+Home", "ctrl+shift+Right")
        wait_for("the count of the selection", lambda: named_like(app, r"1 word"))
        check("with text selected the count is that of the selection (DOC-20)", True)
        press("ctrl+Home")
        wait_for("the count of the document", lambda: named_like(app, rf"{words:,} words"))
        activate("count", "characters")
        wait_for("the characters", lambda: named_like(app, r"[\d,]+ characters"))
        check("the menu of the count chooses what it counts (DOC-20)", True)
        count, _ = named_like(app, r"[\d,]+ characters")
        # The items of GTK's menus have no names: Show Statistics is the
        # action that the item gives.
        open_menu_button(count)
        press("Escape")
        activate("statistics")
        wait_for("the statistics", lambda: showing_named(app, "Statistics"))
        check("Show Statistics of the count opens the statistics (DOC-20)", True)
        press("Escape")
        activate("count", "words")
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def search_commands(app, query):
    """Opens Search Commands and types `query`."""
    press("ctrl+slash")
    wait_for("the command search", lambda: showing_named(app, "Search Commands"))
    type_text(query)


def format_pane(work):
    """The Format pane of window M+ in place of the format bar (step 2,
    INT-12, INT-13): for text, the paragraph style over the tabs, and in the
    Style tab the font, its size, bold, italic, underline and
    strikethrough, the color, the character style (STY-02) and the
    alignment; for an object, the Arrange tab with its order and the menu
    of aligning, distributing and flipping. Search Commands still finds
    what the menus of the bar had."""
    name = "Pane.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        check("there is no format bar (INT-12)", showing_named(app, "More Formatting") is None)
        press("ctrl+alt+i")
        wait_for("the Format pane of text", lambda: showing_named(app, "Paragraph Style"))
        missing = [b for b in TEXT_PANE if showing_named(app, b) is None]
        check(
            "the Format pane of text has the style, the font, its size and style, the color, the character style and the alignment (INT-12)",
            not missing,
        )
        search_commands(app, "character style none")
        wait_for("the character styles in Search Commands", lambda: showing_named(app, "None"))
        check("Search Commands finds the character styles (STY-02)", True)
        press("Escape")
        wait_for("the command search to close", lambda: showing_named(app, "Search Commands") is None)

        activate("line-spacing", "200")
        activate("list-style", "bullets")
        time.sleep(1)
        press("ctrl+s")
        wait_for("the saved line spacing", lambda: saved_content(letter, r'fo:line-height="200%"'))
        check("the line spacing is saved (INT-12)", True)
        wait_for("the saved list", lambda: saved_content(letter, r"<text:list "))
        check("the list is saved (INT-12)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)

    writer, app, letter = open_with_a_shape(work, "PaneShape.odt")
    try:
        if showing_named(app, "Document") is None:
            press("ctrl+alt+i")
        activate("inspector-tab", "arrange")
        wait_for("the Arrange tab", lambda: showing_named(app, "Bring to Front"))
        missing = [b for b in ARRANGE_PANE if showing_named(app, b) is None]
        check(
            "a selected object has its order and the Arrange menu in the Arrange tab (INT-12)",
            not missing,
        )
        activate("object-reflection")
        time.sleep(1)
        press("ctrl+s")
        wait_for("the saved reflection", lambda: saved_content(letter, r'desk:reflection="50%"'))
        check("Reflection reflects the shape (INT-12)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def context_title(app, title):
    """The title of the Inspector over the tabs of Format, if it shows."""
    try:
        return next(
            (
                n
                for n in walk(app)
                if n.get_role() == Atspi.Role.LABEL and n.get_name() == title and showing(n)
            ),
            None,
        )
    except GLib.GError:
        return None


def document_tabs(app):
    """The titles of the document tabs that show in the tab bar; an empty
    list while one document is open, since the bar hides then."""
    return [
        n.get_name() or ""
        for n in walk(app)
        if n.get_role() == Atspi.Role.PAGE_TAB and showing(n) and n.get_name() not in TABS
    ]


def window_named(title):
    """Whether a window has a title that contains title, as xdotool sees it."""
    return subprocess.run(
        ["xdotool", "search", "--name", title], capture_output=True
    ).returncode == 0


def tab_names(app):
    """The names of the tabs of the Inspector that show."""
    return sorted(n.get_name() for n in tab_toggles(app))


def two_level_inspector(work):
    """The Inspector of interface B: Format and Document at the top; under
    Format a title that says what is selected and its tabs, Style, Layout
    and More for text, Style, Text and Arrange for a shape; under Document
    the tabs Document, Section and Info; and a tab by its name from Show
    in Inspector (INT-13)."""
    name = "Levels.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        activate("show-inspector")
        wait_for("the levels of the Inspector", lambda: len(toggles_named(app, LEVELS)) == 2)
        check("the Inspector starts with Format and Document (INT-13)", True)
        wait_for("the title of text", lambda: context_title(app, "Text"))
        check(
            "Format has a title for text and the tabs Style, Layout and More (INT-13)",
            tab_names(app) == ["Layout", "More", "Style"],
        )
        activate("inspector-tab", "layout")
        wait_for("the Layout tab", lambda: showing_named(app, "Default Tabs"))
        check("Show in Inspector opens a tab by its name (INT-13)", True)
        activate("inspector-tab", "info")
        wait_for("the Info tab", lambda: showing_named(app, "Words"))
        check(
            "Document has the tabs Document, Section and Info, with the counts (INT-13)",
            tab_names(app) == ["Document", "Info", "Section"]
            and context_title(app, "Text") is None,
        )
        activate("show-inspector")
        wait_for("Format again", lambda: context_title(app, "Text"))
    finally:
        writer.terminate()
        writer.wait(timeout=10)

    writer, app, letter = open_with_a_shape(work, "LevelsShape.odt")
    try:
        activate("show-inspector")
        wait_for("the title of a shape", lambda: context_title(app, "Shape"))
        check(
            "a selected shape has the title Shape and the tabs Style, Text and Arrange (INT-13)",
            tab_names(app) == ["Arrange", "Style", "Text"],
        )
        activate("inspector-tab", "arrange")
        wait_for("the Arrange tab", lambda: showing_named(app, "Width"))
        check("Arrange has the size and the wrap of the object (INT-13)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


# A shape with a word in it, for the Text tab of a shape (OBJ-13).
SHAPE_WITH_TEXT = (
    b'<draw:rect draw:style-name="Filled" text:anchor-type="page" text:anchor-page-number="1" '
    b'svg:x="96pt" svg:y="200pt" svg:width="300pt" svg:height="200pt">'
    b"<text:p>Shape</text:p></draw:rect>"
)


def object_styles(work):
    """The object styles of iWork '09: the Style tab of a shape starts with
    its styles, and choosing one gives the shape its look and keeps it
    linked to the style in the file (MOD-04)."""
    writer, app, letter = open_with_a_shape(work, "ObjectStyles.odt")
    try:
        group = wait_for("the object styles", lambda: inspector_node(app, "Object Styles"))
        check("the Style tab of a shape has the object styles (MOD-04)", group is not None)
        blue = wait_for(
            "the Blue style",
            lambda: inspector_node(app, "Blue", lambda n: n.get_role() == Atspi.Role.TOGGLE_BUTTON),
        )
        Atspi.Action.do_action(blue, 0)
        time.sleep(1)
        press("ctrl+s")
        wait_for(
            "the saved style of the shape",
            lambda: saved_content(letter, r'style:parent-style-name="[^"]*Blue"'),
        )
        check("a chosen style is the parent of the look of the shape (MOD-04)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


def text_of_a_shape(work):
    """The Text tab of a selected shape formats all its text, as the Text
    inspector of Pages does: the font, its style and the alignment
    (OBJ-13)."""
    writer, app, letter = open_with_a_shape(work, "ShapeText.odt", SHAPE_WITH_TEXT)
    try:
        activate("inspector-tab", "text")
        wait_for("the font of the shape", lambda: inspector_node(app, "Font Family"))
        check("the Text tab of a shape has its font (OBJ-13)", True)
        activate("bold")
        activate("align", "right")
        time.sleep(1)
        press("ctrl+s")
        wait_for("the saved bold text", lambda: saved_content(letter, r'fo:font-weight="bold"'))
        check("Bold with a shape selected makes its text bold (OBJ-13)", True)
        wait_for("the saved alignment", lambda: saved_content(letter, r'fo:text-align="end"'))
        check("the alignment with a shape selected aligns its text (OBJ-13)", True)
    finally:
        writer.terminate()
        writer.wait(timeout=10)


# The warning of a screen smaller than Desk asks for (GNM-14).
SMALL_SCREEN = "The screen is smaller than 1280 × 720, so parts of the window might not fit"


def warning_of_small_screen(app):
    """The title of the warning of a small screen, if it shows: a banner
    that is not revealed still says it shows, with no height."""
    title = showing_named(app, SMALL_SCREEN)
    try:
        return title if title is not None and extents(title).height > 0 else None
    except GLib.GError:
        return None


def small_screen(work):
    """On a screen of 1024 × 600 the first window says that the screen is
    smaller than Desk asks for, Close takes the warning away, and a new
    window does not show it again (GNM-14)."""
    display = ":97"
    screen = subprocess.Popen(
        ["Xvfb", display, "-screen", "0", "1024x600x24"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    name = "Small.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    env = dict(os.environ, DISPLAY=display)
    time.sleep(1)
    writer = subprocess.Popen([str(WRITER), str(letter)], env=env)
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        title = wait_for("the warning of a small screen", lambda: warning_of_small_screen(app))
        check("a screen of 1024 × 600 shows the warning over the first window (GNM-14)", True)
        # Close of the banner is on the line of its title; the window has
        # a Close of its own in the header bar.
        line = extents(title)
        close = next(
            (
                n
                for n in walk(app)
                if n.get_role() == Atspi.Role.PUSH_BUTTON
                and n.get_name() == "Close"
                and showing(n)
                and extents(n).y <= line.y + line.height / 2 <= extents(n).y + extents(n).height
            ),
            None,
        )
        check("the warning has a Close button (GNM-14)", close is not None)
        Atspi.Action.do_action(close, 0)
        wait_for("the warning to go", lambda: warning_of_small_screen(app) is None)
        check("Close takes the warning away (GNM-14)", True)
        # New Window opens a second window of the same app.
        made = subprocess.run(
            [
                "gdbus", "call", "--session", "--dest", "br.com.brahm.Desk.Writer",
                "--object-path", "/br/com/brahm/Desk/Writer",
                "--method", "org.gtk.Actions.Activate", "new-window", "[]", "{}",
            ],
            capture_output=True,
            text=True,
        )
        check("New Window opens a second window", made.returncode == 0)
        wait_for("the second window", lambda: app.get_child_count() >= 2)
        time.sleep(1)
        check(
            "the warning does not come back in a new window (GNM-14)",
            warning_of_small_screen(app) is None,
        )
    finally:
        writer.terminate()
        writer.wait(timeout=10)
        screen.terminate()
        screen.wait(timeout=10)



def tables_and_charts(work):
    """A table from the Insert menu takes typing in its cells, Tab goes
    to the next one and a formula adds them; the Inspector shows the
    Table, Cell and Text tabs, and the screen reader hears the cell. A
    chart from the menu shows the Chart, Axis and Series tabs, and both
    are saved in the file (TAB-01, TAB-02, TAB-22, GRF-01, GRF-03,
    GNM-04). The Table and Chart tabs show the table and chart styles
    (PXV-14, W080), a locked table is not edited (PXV-16), and cells
    dragged onto the page make a new table (TAB-26, PXV-17), with a custom
    format (PXV-18)."""
    name = "Tables.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    # A command that takes a screenshot by a name, for the pull request.
    shots = os.environ.get("DESK_UI_SHOTS")
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        document = wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        activate("show-inspector")
        activate("insert-table")
        wait_for("the Table tab", lambda: context_title(app, "Table"))
        check(
            "a table shows the tabs Table, Cell, Text and Arrange (TAB-02)",
            tab_names(app) == ["Arrange", "Cell", "Table", "Text"],
        )
        type_text("12")
        press("Tab")
        type_text("30")
        press("Tab")
        type_text("=B2+C2")
        press("Return")
        wait_for(
            "the cell for the screen reader",
            lambda: "Table 1" in (document.get_description() or ""),
        )
        check("the screen reader hears the table and the cell (GNM-04)", True)
        if shots:
            subprocess.run([*shots.split(), "table"], check=False)
        press("ctrl+s")
        wait_for(
            "the sum in the saved file",
            lambda: saved_content(letter, r'<table:table-cell [^>]*table:formula="[^"]*B2[^"]*C2[^"]*"')
            and saved_content(letter, r'<table:table-cell [^>]*office:value="42"'),
        )
        check("typing in the cells and a formula reach the file (TAB-01, TAB-22)", True)
        # Selected cells dragged out of the table onto the page make a new
        # table of their own, and leave theirs, as Pages 15.4 moves them
        # (TAB-26, PXV-17).
        type_text("7")
        field = wait_for(
            "the field over the cell",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() in (Atspi.Role.TEXT, Atspi.Role.ENTRY)
                    and showing(n)
                    and text_of(n) == "7"
                ),
                None,
            ),
        )
        box = extents(field)
        press("Return", "Up")
        geometry = subprocess.run(
            ["xdotool", "search", "--name", name, "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        x = int(window["X"]) + box.x + box.width // 2
        y = int(window["Y"]) + box.y + box.height // 2
        subprocess.run(["xdotool", "mousemove", "--sync", str(x), str(y), "mousedown", "1"], check=True)
        for step in range(1, 11):
            subprocess.run(["xdotool", "mousemove", "--sync", str(x), str(y - step * 16)], check=True)
            time.sleep(0.05)
        subprocess.run(["xdotool", "mouseup", "1"], check=True)
        time.sleep(0.5)
        press("ctrl+s")
        wait_for(
            "the new table of the dragged cell in the saved file",
            lambda: saved_content(letter, r'(?s)table:name="Table 2".*office:value="7"')
            and len(re.findall(r'office:value="7"', saved_part(letter, "content.xml", "(?s).*").group(0))) == 1,
        )
        check("cells dragged onto the page make a new table and leave theirs (TAB-26, PXV-17)", True)
        # Create Custom Format, at the end of the Data Format menu, keeps a
        # format under a name in the settings of the file and gives it to
        # the cells (PXV-18).
        choose_in_list(app, "Data Format", name, ["End", "Return"])
        ok = wait_for(
            "the Create Custom Format sheet",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.BUTTON and n.get_name() == "OK" and showing(n)
                ),
                None,
            ),
        )
        Atspi.Action.do_action(ok, 0)
        press("ctrl+s")
        wait_for(
            "the custom format in the saved file",
            lambda: saved_part(letter, "settings.xml", r'config:name="CustomFormats"[^>]*>[^<]*Custom Format 1'),
        )
        check("Create Custom Format keeps a format by name in the file (PXV-18)", True)
        # The Table tab shows the table styles, and Create Style keeps the
        # look of the table as a new one, saved as an ODF table template
        # (PXV-14).
        wait_for("the table styles of the Table tab", lambda: inspector_node(app, "Blue"))
        create = wait_for("the Create Style button", lambda: showing_named(app, "Create Style"))
        Atspi.Action.do_action(create, 0)
        press("ctrl+s")

        def template_saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    styles = package.read("styles.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return False
            return 'table:name="Table Style 1"' in styles

        wait_for("the new table style in the saved file", template_saved)
        check("the Table tab shows the table styles and Create Style keeps one (PXV-14)", True)
        # Lock leaves the table selected as an object, with its tabs off
        # until Unlock, and the file keeps it locked (PXV-16).
        activate("lock")

        def tabs_off():
            # The group of the button is turned off, which AT-SPI shows on
            # the group, not on the button inside it.
            node = create
            for _ in range(8):
                if node is None:
                    return False
                if not node.get_state_set().contains(Atspi.StateType.SENSITIVE):
                    return True
                node = node.get_parent()
            return False

        wait_for("the Table tab to turn off for a locked table", tabs_off)
        press("ctrl+s")
        wait_for(
            "the locked table in the saved file",
            lambda: saved_content(letter, r'style:protect="position size"'),
        )
        activate("unlock")
        wait_for("the Table tab to come back", lambda: not tabs_off())
        check("a locked table is not edited and the file keeps the lock (PXV-16)", True)
        activate("insert-chart-kind", "pie")
        wait_for("the Chart tab", lambda: context_title(app, "Chart"))
        check(
            "a chart shows the tabs Chart, Axis, Series and Arrange (GRF-03)",
            tab_names(app) == ["Arrange", "Axis", "Chart", "Series"],
        )
        if shots:
            subprocess.run([*shots.split(), "chart"], check=False)
        press("ctrl+s")

        def chart_saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    # A chart is in the folder named after its object.
                    return any(
                        name.startswith("Object ") and name.endswith("/content.xml")
                        for name in package.namelist()
                    )
            except (zipfile.BadZipFile, OSError):
                return False

        wait_for("the chart in the saved file", chart_saved)
        check("the chart is saved as a chart of ODF (GRF-01)", True)
        # The Chart tab shows the chart styles, and Create Style keeps the
        # look of the chart after them, in the settings of the file (W080).
        wait_for("the chart styles of the Chart tab", lambda: showing_named(app, "Chart Style 6"))
        create = wait_for("the Create Style button", lambda: showing_named(app, "Create Style"))
        Atspi.Action.do_action(create, 0)
        press("ctrl+s")

        def chart_style_saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    return "ChartStyles" in package.read("settings.xml").decode()
            except (zipfile.BadZipFile, KeyError, OSError):
                return False

        wait_for("the new chart style in the saved file", chart_style_saved)
        check("the Chart tab shows the chart styles and Create Style keeps one (W080)", True)
        # Insert Equation writes LaTeX with a preview of it; with the chart
        # selected, the equation goes on the page, and is saved as MathML
        # with its LaTeX (W104, PXV-22).
        activate("insert-equation")
        wait_for("the equation editor", lambda: find(app, Atspi.Role.TEXT, "Equation"))
        type_text("x^2 + 1")

        def in_editor(node):
            while node is not None:
                if node.get_name() == "Insert Equation" and node.get_role() != Atspi.Role.LABEL:
                    return True
                node = node.get_parent()
            return False

        insert = wait_for(
            "the Insert button of the editor",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.BUTTON
                    and n.get_name() == "Insert"
                    and showing(n)
                    and n.get_state_set().contains(Atspi.StateType.SENSITIVE)
                    and in_editor(n)
                ),
                None,
            ),
        )
        if shots:
            subprocess.run([*shots.split(), "equation-editor"], check=False)
        Atspi.Action.do_action(insert, 0)
        wait_for("the Equation tab", lambda: context_title(app, "Equation"))
        check(
            "an equation shows the tabs Equation and Arrange (W104)",
            tab_names(app) == ["Arrange", "Equation"],
        )
        press("ctrl+s")

        def equation_saved():
            try:
                with zipfile.ZipFile(letter) as package:
                    return any(
                        name.startswith("Equation ") and name.endswith("/content.xml")
                        for name in package.namelist()
                    )
            except (zipfile.BadZipFile, OSError):
                return False

        wait_for("the equation in the saved file", equation_saved)
        check(
            "Insert Equation writes LaTeX, saved as MathML with it (W104)",
            saved_content(letter, r'desk:latex="x\^2 \+ 1"'),
        )
    finally:
        writer.terminate()
        writer.wait(timeout=10)



def versions_of(work):
    """The versions that Desk keeps, by why they were kept: the end of
    their file names, as "opened" (DOC-11)."""
    root = Path(os.environ["XDG_DATA_HOME"]) / "desk" / "versions"
    return sorted(p.stem.split("-", 2)[2] for p in root.glob("*/*") if p.is_file())


def automatic_saving(work):
    """A document with a file saves itself as it is edited, keeps the
    file as it was opened as a version, keeps one more with Ctrl+S, goes
    back to an older one from Browse Versions, and closes without asking
    (DOC-11, D-001, step 5 of window M+)."""
    root = Path(os.environ["XDG_DATA_HOME"]) / "desk" / "versions"
    shutil.rmtree(root, ignore_errors=True)
    name = "Saving.odt"
    letter = work / name
    subprocess.run([str(CONVERT), str(FIXTURE), str(letter)], check=True)
    writer = subprocess.Popen([str(WRITER), str(letter)])
    try:
        app = wait_for("Desk Writer on the accessibility bus", lambda: app_named("desk-writer"))
        wait_for("the document", lambda: find(app, Atspi.Role.TEXT, name))
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", name, "windowfocus", "--sync"],
            check=True,
        )
        press("ctrl+End")
        type_text(" autosaved")
        wait_for("the document to save itself", lambda: saved_content(letter, "autosaved"))
        check("a document with a file saves itself as it is edited, without Ctrl+S (DOC-11)", True)
        check("and its title shows no unsaved changes (DOC-11)", not window_named("•"))
        check("the file as it was opened is kept as a version (DOC-11)", versions_of(work) == ["opened"])
        press("ctrl+s")
        wait_for("a version kept", lambda: versions_of(work) == ["opened", "saved"])
        check("Ctrl+S keeps a version, as Cmd-S of iWork (DOC-11)", True)

        activate("versions")
        wait_for("the versions", lambda: showing_named(app, "When the document was opened"))
        check("Browse Versions lists the versions (DOC-11)", showing_named(app, "Kept with Save"))
        restore = sorted(
            (
                n
                for n in walk(app)
                if n.get_role() == Atspi.Role.BUTTON and n.get_name() == "Restore" and showing(n)
            ),
            key=lambda n: extents(n).y,
        )
        # The oldest is the last: the file as it was opened.
        Atspi.Action.do_action(restore[-1], 0)
        wait_for("the document as it was opened", lambda: not saved_content(letter, "autosaved"))
        check("Restore puts an older version in place of the document (DOC-11)", True)
        check(
            "and keeps what it held as a version first (DOC-11)",
            versions_of(work) == ["auto", "opened", "saved"],
        )

        press("ctrl+End")
        type_text(" again")
        press("ctrl+w")
        wait_for("the window to close", lambda: writer.poll() is not None or not window_named(name))
        check("the tab of a document that saves itself closes without asking (DOC-11)", True)
        check("and what was typed last is in the file", saved_content(letter, "again"))
    finally:
        writer.terminate()
        writer.wait(timeout=10)

if __name__ == "__main__":
    main()
