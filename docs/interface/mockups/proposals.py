# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The two windows proposed on 2026-11-16, when the target of Desk became
the current iWork (Pages, Numbers and Keynote 15.4).

N  New iWork  the window of the current iWork, drawn with the widgets of
              Libadwaita wherever one exists: a toolbar of buttons with a
              caption under the icon, document tabs, the sheets of Tables
              as tabs under the toolbar, a navigation pane under the
              toolbar, Format, Animate and Document as toolbar buttons that
              choose what the pane on the right shows, and nothing under the
              document but the word count.
M  Mix        the same model (contextual Format pane, tabs, no format bar),
              arranged where the GNOME HIG puts each thing: a sidebar of
              full height, a header bar of icon buttons with the window
              title, one Insert button, one Inspector button and the
              Format/Document choice inside the pane, a primary menu, and
              the sheets in a view switcher at the bottom.

Widgets that Libadwaita does not have are built here and tagged with an id
that starts with `proprio.`: they are the ones to review when a new
Libadwaita comes out.
"""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gdk, Gtk  # noqa: E402

import parts
import shell
import slides
import tables
import writer
from kit import (Scene, classes, expand, hbox, icon_button, label, menu_button, menu_model, spacer,
                 tag, toggle_group, vbox)

CSS = """
.tool-item { padding: 3px 6px; min-width: 52px; }
.tool-caption { font-size: 0.78em; font-weight: 400; }
.tool-zoom { font-weight: 700; font-size: 0.95em; min-height: 16px; }
.pane-under { background-color: var(--sidebar-bg-color); }
.sheet-switch { margin: 3px 0; }
"""
_css_loaded = False


def _css():
    """Adds the styles of this module once; kit.init has run by then."""
    global _css_loaded
    if _css_loaded:
        return
    prov = Gtk.CssProvider()
    prov.load_from_string(CSS)
    Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), prov,
                                              Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION + 1)
    _css_loaded = True


# ------------------------------------------------------------ own widgets


def tool_item(caption, cid, icon=None, text=None, menu=None, popover=None, toggle=False, active=False):
    """A toolbar item of iWork: an icon (or a value, as in Zoom) over a
    caption. Libadwaita has no button of this shape (AdwButtonContent puts
    the label beside the icon), so it is a GtkButton with a custom child."""
    if icon:
        top = Gtk.Image(icon_name=icon + '-symbolic', pixel_size=16)
    else:
        top = classes(Gtk.Label(label=text), 'tool-zoom', 'numeric')
    top.set_size_request(-1, 20)        # one height, so that the captions line up
    top.set_valign(Gtk.Align.CENTER)
    child = vbox(top, classes(Gtk.Label(label=caption), 'tool-caption'), spacing=2)
    if menu is not None or popover is not None:
        b = Gtk.MenuButton(child=child)
        if popover is not None:
            b.set_popover(popover)
        else:
            b.set_menu_model(menu)
    elif toggle:
        b = Gtk.ToggleButton(child=child, active=active)
    else:
        b = Gtk.Button(child=child)
    classes(b, 'flat', 'tool-item')
    b.set_valign(Gtk.Align.CENTER)
    return tag(b, 'own.' + cid)


def insert_items(app, first):
    """Insert (or Play), Table, Chart, Text, Shape, Media and Comment."""
    items = [first]
    names = {'text': 'Text'}
    for suf, icon, tip, kind in parts.INSERT:
        pop = parts.gallery_popover(kind) if kind else None
        items.append(tool_item(names.get(suf, tip), 'bar.' + suf, icon=icon, popover=pop))
    return hbox(*items, spacing=2)


def toolbar_n(app, start, centre, end):
    """The toolbar of the current iWork as a header bar: view and zoom at
    the start, the insert items in the middle, the panes at the end. The
    primary menu is what GNOME adds: there is no menu bar to hold it."""
    hb = Adw.HeaderBar()
    hb.set_title_widget(centre)
    for w in start:
        hb.pack_start(w)
    hb.pack_end(menu_button(icon='open-menu', tip='Main menu', cid='head.menu',
                            menu=parts.main_menu(app, lean=True)))
    for w in reversed(end):
        hb.pack_end(w)
    return tag(hb, 'head.bar')


def document_tabs(app, others, child):
    """The open documents as tabs (AdwTabView and AdwTabBar)."""
    view = Adw.TabView()
    names = [parts.APPS[app]['doc']] + list(others)
    first = None
    for n in names:
        pg = view.append(child if first is None else Gtk.Box())
        pg.set_title(n)
        first = first or pg
    view.set_selected_page(first)
    bar = Adw.TabBar(view=view, autohide=False)
    return view, tag(bar, 'tabs.documents')


def pill(text, cid, **kw):
    return shell.osd_pill(label(text, 'numeric', margin_start=10, margin_end=10, margin_top=5,
                                margin_bottom=5), cid=cid, **kw)


def zoom_pill(text):
    return shell.osd_pill(icon_button('zoom-out', 'Zoom out'),
                          label(text, 'numeric', margin_start=4, margin_end=4),
                          icon_button('zoom-in', 'Zoom in'), cid='osd.zoom',
                          halign=Gtk.Align.END, valign=Gtk.Align.END, margin_end=14, margin_bottom=12)


def under_pane(child, width):
    """A navigation pane under the toolbar, as in the current iWork."""
    child.set_size_request(width, -1)
    return classes(vbox(child), 'pane-under')


WIDE_W = 372   # four tabs, or three long first-level names, do not fit in 332 px


def wide(build, width=WIDE_W):
    """Builds an Inspector wider than 332 px and gives it back with its width."""
    old, shell.INSPECTOR_W = shell.INSPECTOR_W, width
    try:
        return build(), width
    finally:
        shell.INSPECTOR_W = old


def objects_sidebar():
    """The tables and charts of the current sheet: what the Sheets pane of
    Numbers '09 listed, now that the sheets themselves are tabs."""
    sb = Adw.Sidebar()
    sec = Adw.SidebarSection(title='In “Expenses”')
    for n, ic in tables.OBJECTS:
        sec.append(Adw.SidebarItem(title=n, icon_name=ic + '-symbolic'))
    sb.append(sec)
    sb.set_selected(0)
    return tag(sb, 'side.objects')


OTHER_DOCS = {'writer': ['March minutes', 'Sales proposal'],
              'tables': ['Cash flow'],
              'slides': ['Sales meeting']}


# -------------------------------------------------------------- version N


def writer_n(win, context='text', level=shell.FORMAT):
    _css()
    app = 'writer'
    start = [tool_item('View', 'bar.view', icon='sidebar-show', menu=parts.view_menu(app)),
             tool_item('Zoom', 'bar.zoom', text='125%', menu=parts.zoom_menu()),
             tool_item('Add page', 'bar.add', icon='list-add', menu=writer.ADD_PAGE)]
    centre = insert_items(app, tool_item('Insert', 'bar.insert', icon='desk-insert',
                                         menu=parts.insert_menu(app)))
    end = [tool_item('Format', 'bar.format', icon='desk-format', toggle=True,
                     active=level == shell.FORMAT),
           tool_item('Document', 'bar.document', icon='x-office-document', toggle=True,
                     active=level == shell.DOCUMENT)]
    words = pill('26,171 words', 'osd.count', halign=Gtk.Align.CENTER, valign=Gtk.Align.END,
                 margin_bottom=12)
    doc = writer.doc_view(context, rulers=False, overlays=(words,))
    inner = shell.split(writer.inspector_c(context, level), doc, shell.INSPECTOR_W, Gtk.PackType.END)
    body = shell.split(under_pane(parts.thumbs_list('page', 5, 2), 150), inner, 150)
    view, tabs = document_tabs(app, OTHER_DOCS[app], body)
    win.set_content(shell.toolbar_view(view, top=[toolbar_n(app, start, centre, end), tabs]))


def tables_n(win):
    _css()
    app = 'tables'
    start = [tool_item('View', 'bar.view', icon='sidebar-show', menu=parts.view_menu(app)),
             tool_item('Zoom', 'bar.zoom', text='100%', menu=parts.zoom_menu()),
             tool_item('Category', 'bar.category', icon='desk-category', menu=menu_model(['—'])),
             tool_item('Pivot table', 'bar.pivot', icon='desk-sort')]
    centre = insert_items(app, tool_item('Insert', 'bar.insert', icon='desk-function',
                                         menu=tables.FUNCTIONS))
    end = [tool_item('Format', 'bar.format', icon='desk-format', toggle=True, active=True),
           tool_item('Arrange', 'bar.arrange', icon='desk-filter', toggle=True)]
    sheets = Adw.TabView()
    chosen = None
    for n in tables.SHEETS:
        pg = sheets.append(tables.sheet_view() if n == 'Expenses' else Gtk.Box())
        pg.set_title(n)
        if n == 'Expenses':
            chosen = pg
    sheets.set_selected_page(chosen)
    strip = Adw.TabBar(view=sheets, autohide=False, expand_tabs=False)
    strip.set_start_action_widget(icon_button('list-add', 'Add sheet', 'tabs.add'))
    tag(strip, 'tabs.sheets')
    cell = hbox(tag(classes(label('=SUM(B2:E2)', 'formula-entry', hexpand=True), 'numeric'), 'bottom.formula'),
                tables.calc_chips(), spacing=8)
    classes(cell, 'toolbar', 'bottombar')
    tag(cell, 'bottom.cell')
    pages, vis = tables.format_pages()
    insp, width = wide(lambda: shell.inspector_c(None, pages, vis), 404)
    inner = shell.split(insp, shell.toolbar_view(sheets, top=[strip], bottom=[cell]), width,
                        Gtk.PackType.END)
    view, tabs = document_tabs(app, OTHER_DOCS[app], inner)
    win.set_content(shell.toolbar_view(view, top=[toolbar_n(app, start, centre, end), tabs]))


def slides_n(win, context='image', level=shell.FORMAT):
    _css()
    app = 'slides'
    start = [tool_item('View', 'bar.view', icon='sidebar-show', menu=parts.view_menu(app)),
             tool_item('Zoom', 'bar.zoom', text='62%', menu=parts.zoom_menu()),
             tool_item('Add slide', 'bar.add', icon='list-add',
                       popover=slides.masters_popover())]
    centre = insert_items(app, tool_item('Play', 'bar.play', icon='media-playback-start'))
    end = [tool_item('Format', 'bar.format', icon='desk-format', toggle=True,
                     active=level == shell.FORMAT),
           tool_item('Animate', 'bar.animate', icon='desk-animate', toggle=True,
                     active=level == shell.ANIMATION),
           tool_item('Document', 'bar.document', icon='x-office-document', toggle=True,
                     active=level == shell.DOCUMENT)]
    inner = shell.split(slides.inspector_c(context, level), slides.slide_view(context, notes=False),
                        shell.INSPECTOR_W, Gtk.PackType.END)
    body = shell.split(under_pane(slides.navigator(), 170), inner, 170)
    view, tabs = document_tabs(app, OTHER_DOCS[app], body)
    win.set_content(shell.toolbar_view(view, top=[toolbar_n(app, start, centre, end), tabs]))


# -------------------------------------------------------------- version M


def header_m(app, extra_start=(), objects=False):
    """The header bar of the mix: what the HIG asks of one. Icon buttons
    for the window as a whole, the title in the middle, the primary menu
    at the end. Saving is automatic, so the subtitle is the folder alone."""
    hb = Adw.HeaderBar()
    hb.set_title_widget(tag(Adw.WindowTitle(title=parts.APPS[app]['doc'], subtitle='Documents'),
                            'head.title'))
    hb.pack_start(icon_button('sidebar-show', 'Sidebar', 'head.sidebar', toggle=True, active=True))
    if objects:
        # Variant M+: the six kinds of object stay one click away, as icon
        # buttons with a tooltip, and Insert keeps the rest (breaks, fields).
        hb.pack_start(parts.insert_menu_button(app))
        for b in parts.insert_buttons():
            hb.pack_start(b)
    else:
        hb.pack_start(menu_button(icon='desk-insert', text='Insert', tip='Insert', cid='head.insert',
                                  arrow=False, popover=writer.insert_all_popover(app)))
    for w in extra_start:
        hb.pack_start(w)
    hb.pack_end(menu_button(icon='open-menu', tip='Main menu', cid='head.menu',
                            menu=parts.main_menu(app, lean=True)))
    hb.pack_end(icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True))
    hb.pack_end(icon_button('edit-find', 'Find', 'head.find', toggle=True))
    hb.pack_end(icon_button('view-grid', 'Open documents', 'head.tabs'))
    return tag(hb, 'head.bar')


def writer_m(win, context='text', level=shell.FORMAT, objects=False):
    _css()
    app = 'writer'
    side = shell.nav_sidebar(app, parts.thumbs_list('page', 5, 2), writer.SIDE_MODES, 'Add page',
                             writer.ADD_PAGE)
    words = pill('26,171 words', 'osd.count', halign=Gtk.Align.CENTER, valign=Gtk.Align.END,
                 margin_bottom=12)
    doc = writer.doc_view(context, rulers=False, overlays=(zoom_pill('125%'), words))
    if context == 'text':
        insp, width = writer.inspector_b(context, level), shell.INSPECTOR_W
    else:       # the three second-level names of an object need the wider pane
        insp, width = wide(lambda: writer.inspector_b(context, level))
    body = shell.split(insp, doc, width, Gtk.PackType.END)
    view, tabs = document_tabs(app, OTHER_DOCS[app], body)
    content = shell.toolbar_view(view, top=[header_m(app, objects=objects), tabs])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def tables_m(win, level=shell.FORMAT, objects=False):
    _css()
    app = 'tables'
    switch = toggle_group([(n, n, None, None) for n in tables.SHEETS], 1, 'bottom.sheets', 'round',
                          'sheet-switch')
    bottom = hbox(icon_button('list-add', 'Add sheet', 'bottom.add'), switch, expand(),
                  tables.calc_chips(), spacing=6)
    classes(bottom, 'toolbar', 'bottombar')
    tag(bottom, 'bottom.bar')
    insp, width = wide(lambda: tables.inspector_b(level), 404)
    body = shell.split(insp, tables.sheet_view(overlays=(zoom_pill('100%'),)), width, Gtk.PackType.END)
    view, tabs = document_tabs(app, OTHER_DOCS[app], body)
    content = shell.toolbar_view(view, top=[header_m(app, objects=objects), tabs, tables.formula_bar()], bottom=[bottom])
    side = shell.nav_sidebar(app, objects_sidebar(), ['Tables and charts', 'Table styles', 'Comments',
                                                      'Search results', 'Formula list'],
                             'Add table or chart', menu_model(['Table', 'Chart']))
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def slides_m(win, context='image', level=shell.FORMAT, objects=False):
    _css()
    app = 'slides'
    side = shell.nav_sidebar(app, slides.navigator(), slides.SIDE_MODES, 'Add slide',
                             add_popover=slides.masters_popover())
    play = icon_button('media-playback-start', 'Play', 'head.play')
    classes(play, 'suggested-action')
    insp, width = wide(lambda: slides.inspector_b(context, level))
    body = shell.split(insp, slides.slide_view(context, notes=True, overlays=(zoom_pill('62%'),)), width,
                       Gtk.PackType.END)
    view, tabs = document_tabs(app, OTHER_DOCS[app], body)
    content = shell.toolbar_view(view, top=[header_m(app, [spacer(), play], objects=objects), tabs])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


SCENES = [
    Scene('writer-n', 'Desk Writer · N current iWork', lambda w: writer_n(w), 'writer', 'N'),
    Scene('writer-n-image', 'Writer N · image selected', lambda w: writer_n(w, context='image'),
          'writer', 'N'),
    Scene('writer-n-dark', 'Writer N · dark style', lambda w: writer_n(w), 'writer', 'N', dark=True),
    Scene('tables-n', 'Desk Tables · N current iWork', lambda w: tables_n(w), 'tables', 'N'),
    Scene('slides-n', 'Desk Slides · N current iWork', lambda w: slides_n(w), 'slides', 'N'),
    Scene('slides-n-animate', 'Slides N · Animate', lambda w: slides_n(w, level=shell.ANIMATION), 'slides',
          'N'),
    Scene('writer-m', 'Desk Writer · M mix with the HIG', lambda w: writer_m(w), 'writer', 'M'),
    Scene('writer-m-image', 'Writer M · image selected', lambda w: writer_m(w, context='image'),
          'writer', 'M'),
    Scene('writer-m-dark', 'Writer M · dark style', lambda w: writer_m(w), 'writer', 'M', dark=True),
    Scene('tables-m', 'Desk Tables · M mix with the HIG', lambda w: tables_m(w), 'tables', 'M'),
    Scene('slides-m', 'Desk Slides · M mix with the HIG', lambda w: slides_m(w), 'slides', 'M'),
    # M+, the window chosen on 2026-11-16 (D-008). Its own version, so that
    # inventory.py can list the components of this window alone.
    Scene('writer-m-plus', 'Writer M+ · objects in the header', lambda w: writer_m(w, objects=True), 'writer', 'M+'),
    Scene('writer-m-plus-image', 'Writer M+ · image selected',
          lambda w: writer_m(w, context='image', objects=True), 'writer', 'M+'),
    Scene('writer-m-plus-document', 'Writer M+ · Document',
          lambda w: writer_m(w, level=shell.DOCUMENT, objects=True), 'writer', 'M+'),
    Scene('writer-m-plus-dark', 'Writer M+ · dark style', lambda w: writer_m(w, objects=True), 'writer', 'M+',
          dark=True),
    Scene('tables-m-plus', 'Tables M+ · objects in the header', lambda w: tables_m(w, objects=True), 'tables', 'M+'),
    Scene('tables-m-plus-data', 'Tables M+ · Data', lambda w: tables_m(w, level=shell.DATA, objects=True),
          'tables', 'M+'),
    Scene('slides-m-plus', 'Slides M+ · objects in the header', lambda w: slides_m(w, objects=True), 'slides', 'M+'),
    Scene('slides-m-plus-animate', 'Slides M+ · Animation',
          lambda w: slides_m(w, level=shell.ANIMATION, objects=True), 'slides', 'M+'),
    Scene('slides-m-animate', 'Slides M · Animation', lambda w: slides_m(w, level=shell.ANIMATION), 'slides',
          'M'),
]
