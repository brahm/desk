# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Drives Desk Writer and checks it through the accessibility tree, as a
screen reader sees it: it opens a document, reads its text and cursor,
types, undoes and saves.

Run it with ci/ui-test.sh, which starts a display and the accessibility
bus. It reads with PyGObject and the AT-SPI typelib (gir1.2-atspi-2.0)
and types with xdotool.
"""

import re
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

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
            lambda: text_of(document) == "US Letter, landscape ação\tx\nOne\n\n",
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


if __name__ == "__main__":
    main()
