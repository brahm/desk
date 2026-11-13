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


def press(*keys):
    subprocess.run(["xdotool", "key", "--delay", "80", *keys], check=True)
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

        press("ctrl+b")
        bold = find(app, Atspi.Role.TOGGLE_BUTTON, "Bold")
        check("the Bold button is named", bold is not None)
        wait_for("Bold to turn on", lambda: bold.get_state_set().contains(Atspi.StateType.PRESSED))
        check("Ctrl+B turns Bold on for the selection", True)
        press("ctrl+z")
        wait_for("Bold to turn off", lambda: not bold.get_state_set().contains(Atspi.StateType.PRESSED))
        check("Undo takes Bold back", True)

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
        press("Return")
        wait_for("the list to end", lambda: not showing(find_named(app, "Indent Level")))
        check("Return on an empty item ends the list", True)
        wait_for(
            "the effects in the Inspector",
            lambda: showing(find_named(app, "Character Spacing"))
            and showing(find_named(app, "Borders and Rules"))
            and showing(find_named(app, "Hyphenate Words"))
            and showing(find_named(app, "Hyphenate the Document"))
            and showing(find_named(app, "Header Margin"))
            and showing(find_named(app, "Space Between Notes"))
            and showing(find_named(app, "Styles to Include"))
            and showing(find_named(app, "Make Links Inactive")),
        )
        check("the Inspector shows spacing, hyphenation, borders and rules", True)
        check("the Inspector shows the headers and footers (EST-01)", True)
        check("the Inspector shows how notes show (EST-13)", True)
        check("the Inspector shows the table of contents and links (EST-23, EST-25)", True)
        press("ctrl+alt+i")

        # Ctrl+T shows the Fonts window, with its effects and typography
        # (INT-15, FMT-08).
        press("ctrl+t")
        fonts = wait_for(
            "the Fonts window",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.FRAME and n.get_name() == "Fonts"
                ),
                None,
            ),
        )
        check("Ctrl+T shows the Fonts window (INT-15, IOP-06)", showing(fonts))
        check("it has the shadow", find_named(fonts, "Shadow") is not None)
        check(
            "it has the colors of the underline and the strikethrough (FMT-01, FMT-02)",
            find_named(fonts, "Underline Color") is not None
            and find_named(fonts, "Strikethrough Color") is not None,
        )
        check("it has the typography", find_named(fonts, "Ligatures") is not None)

        # Ctrl+Shift+T shows the Styles drawer, with the style of the
        # paragraph chosen in its list (INT-18).
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
        check("Ctrl+Shift+T shows the Styles drawer (INT-18)", True)
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

        # The Search sidebar lists the matches as they are typed (TXT-13).
        press("ctrl+shift+f")
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

        # Find & Replace replaces every match (TXT-12).
        press("ctrl+f")
        dialog = wait_for(
            "the Find & Replace window",
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_role() == Atspi.Role.FRAME and n.get_name() == "Find & Replace"
                ),
                None,
            ),
        )
        type_text("hi")
        press("Tab")
        type_text("hello")
        replace_all = next(
            n
            for n in walk(dialog)
            if n.get_role() == Atspi.Role.PUSH_BUTTON and n.get_name() == "Replace All"
        )
        Atspi.Action.do_action(replace_all, 0)
        wait_for(
            "the replaced text",
            lambda: "\u201chello\u201d" in text_of(document),
        )
        check("Replace All replaces the matches", True)

        # Ctrl+: shows the Spelling and Grammar window (TXT-10).
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Letter.odt", "windowfocus", "--sync"],
            check=True,
        )
        press("ctrl+colon")
        wait_for(
            "the Spelling and Grammar window",
            lambda: any(
                n.get_role() == Atspi.Role.FRAME and n.get_name() == "Spelling and Grammar"
                for n in walk(app)
            ),
        )
        check("Ctrl+: shows the Spelling and Grammar window", True)

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

        # Close All asks to save each document with changes (DOC-18).
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
    pages_as_sections(work)
    reflection_of_a_shape(work)
    advanced_gradient(work)
    mask_and_frame(work)
    mask_from_the_menu(work)
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

        # The eyedropper of the format bar colors the selection.
        click(first, line, count=2)
        picker = next(
            n
            for n in walk(app)
            if n.get_role() == Atspi.Role.PUSH_BUTTON
            and n.get_name() == "Pick a Color from the Screen"
            and showing(n)
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
        # The first eyedropper of the bar is that of the color behind the
        # text, or of the text.
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
    def row():
        for node in walk(app):
            if node.get_name() == title and showing(node):
                return node
        return None

    switch = wait_for(what, row)
    toggle = next(node for node in walk(switch) if node.get_role() == Atspi.Role.CHECK_BOX)
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
            for node in walk(app):
                if (
                    node.get_name() == "Text Shadow"
                    and node.get_role() != Atspi.Role.LABEL
                    and showing(node)
                    and Atspi.Action.get_n_actions(node) > 0
                ):
                    return node
            return None

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
            lambda: next(
                (
                    n
                    for n in walk(app)
                    if n.get_name() == "Take Out a Color" and showing(n)
                ),
                None,
            ),
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
    press("Escape", "ctrl+shift+v")
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

    wait_for("the Open button of the dialog", open_button).do_action(0)
    time.sleep(1)


def showing_named(app, name):
    """The node of a name that shows, if there is one."""
    try:
        return next((n for n in walk(app) if n.get_name() == name and showing(n)), None)
    except GLib.GError:
        # A part of a dialog that closed while it was walked.
        return None


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
        wait_for("the Image group of the Inspector", lambda: showing_named(app, "Take Out a Color"))
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
        wait_for("the Movie and Sound group of the Inspector", lambda: showing_named(app, "Poster Frame"))
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
        play = wait_for("the Play button of the Inspector", lambda: showing_named(app, "Play"))
        play.do_action(0)
        wait_for("the window that plays the movie", lambda: showing_named(app, "Movie"))
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
        row = wait_for("the Stop row of the Inspector", lambda: showing_named(app, "Stop"))
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
    """The Media Browser shows the photographs of the Pictures folder, and
    one activated there goes on the page (INT-14)."""
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
        subprocess.run(
            ["xdotool", "search", "--sync", "--name", "Media Browser", "windowfocus", "--sync"],
            check=True,
        )
        geometry = subprocess.run(
            ["xdotool", "search", "--name", "Media Browser", "getwindowgeometry", "--shell"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        window = dict(line.split("=", 1) for line in geometry.split() if "=" in line)
        box = Atspi.Component.get_extents(cell, Atspi.CoordType.WINDOW)
        # A double click activates it, as in the browser of Pages.
        click(int(window["X"]) + box.x + box.width / 2, int(window["Y"]) + box.y + box.height / 2, 2)
        wait_for("the Image group of the Inspector", lambda: showing_named(app, "Take Out a Color"))
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
        def points_row():
            for node in walk(app):
                if node.get_name() == "Star Points" and showing(node):
                    return node
            return None

        row = wait_for("the Shape group of the Inspector", points_row)
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
    parameter = "[]" if value is None else f"[<'{value}'>]"
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
            lambda: (lambda n: n if n and showing(n) else None)(find_named(app, "Width")),
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
            lambda: (lambda n: n if n and showing(n) else None)(find_named(app, "Width")),
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
    row = wait_for(f"the {title} row of the Inspector", lambda: showing_named(app, title))
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
        lambda: next(
            (
                n
                for n in walk(app)
                if n.get_name() == title
                and n.get_role() == Atspi.Role.COMBO_BOX
                and showing(n)
            ),
            None,
        ),
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
            return (lambda n: n if n and showing(n) else None)(find_named(app, "Width"))

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


if __name__ == "__main__":
    main()
