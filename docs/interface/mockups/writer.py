# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desk Writer in the three versions."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import canvas
import parts
import shell
from kit import (Scene, action_row, classes, combo_row, dropdown, expand, group, hbox, icon_button,
                 inline_switcher, label, menu_button, menu_model, page, spacer, spin_row,
                 split_button, switch_row, tag, text_button, toggle_group, vbox, view_stack)

APP = 'writer'
SIDE_MODES = ['Pages', 'Styles', 'Comments', 'Search results']
ADD_PAGE = menu_model(['Blank'], [{'title': 'Template pages'}, 'Cover', 'Table of contents', 'Chapter', 'Text'],
                      ['Section break'])


def doc_view(selection='text', rulers=True, overlays=(), comments=False):
    cv = tag(canvas.writer_canvas(selection=selection, comments=comments), 'doc.page')
    inner = cv
    if overlays:
        ov = Gtk.Overlay(child=cv)
        for w in overlays:
            ov.add_overlay(w)
        inner = ov
    if rulers:
        return vbox(tag(canvas.ruler('h'), 'doc.ruler'), inner)
    return inner


# --------------------------------------------------------------- Inspector


def format_pages(context):
    """The pages of Format for what is selected, as (name, title, child)."""
    if context == 'text':
        return [('style', 'Style', None, parts.text_style_page()),
                ('layout', 'Layout', None, parts.text_layout_page()),
                ('more', 'More', None, parts.text_more_page())], 'style'
    if context == 'image':
        return [('style', 'Style', None, parts.object_style_page(image=True)),
                ('image', 'Image', None, parts.image_page()),
                ('arrange', 'Arrange', None, parts.arrange_page())], 'arrange'
    if context == 'shape':
        return [('style', 'Style', None, parts.object_style_page()),
                ('text', 'Text', None, parts.text_style_page('insp.text')),
                ('arrange', 'Arrange', None, parts.arrange_page())], 'style'
    raise ValueError(context)


def inspector_b(context='text', level=shell.FORMAT):
    levels = [shell.FORMAT, shell.DOCUMENT]
    if level == 'Document':
        return shell.inspector_b(levels, level, pages=[
            ('doc', 'Document', None, parts.document_page()),
            ('section', 'Section', None, parts.section_page()),
            ('info', 'Info', None, parts.info_page())], visible='doc')
    pages, visible = format_pages(context)
    if context == 'text':
        return shell.inspector_b(levels, level, 'Text', parts.style_header(), pages, visible)
    return shell.inspector_b(levels, level, {'image': 'Image', 'shape': 'Shape'}[context], None, pages, visible)


def inspector_c(context='text', level=shell.FORMAT):
    pages, visible = format_pages(context)
    head = parts.style_header() if context == 'text' else None
    return shell.inspector_c(head, pages, visible)


A_ICONS = [('doc', 'x-office-document', 'Document'), ('layout', 'desk-columns', 'Layout'),
           ('wrap', 'desk-wrap', 'Wrap'), ('text', 'desk-character-style', 'Text'),
           ('graphic', 'desk-fill', 'Graphic'), ('metrics', 'desk-ruler', 'Metrics'),
           ('link', 'desk-link', 'Link')]


def inspector_a(context='text'):
    """The Text pane of the iWork '09 Inspector, in the compact rows Desk has today."""
    cg, cr = shell.compact_group, shell.compact_row
    groups = [
        cg('Color and alignment',
           shell.plain_row(hbox(parts.swatch('#1e1e1e'), parts.align_group(None, 'flat'), spacing=10)),
           cid='insp.a.color'),
        cg('Spacing', cr('Character', 0, '%', 0), cr('Line', 1.2, 'lines'),
           cr('Before paragraph', 0, 'pt', 0), cr('After paragraph', 6, 'pt', 0),
           cid='insp.a.spacing'),
        cg('Inset margin', cr('Margin', 0, 'pt', 0), cid='insp.a.margin'),
        cg('Indents', cr('First line', 0, 'cm', 2), cr('Left', 0, 'cm', 2), cr('Right', 0, 'cm', 2),
           cid='insp.a.indents'),
        cg('Hyphenation', shell.switch_line('Hyphenate words', True), cid='insp.a.hyphenation'),
    ]
    return shell.inspector_a(A_ICONS, 3, 'Text', ['Text', 'List', 'Tabs', 'More'], groups)


# ---------------------------------------------------------------- versions


def version_a(win, context='text'):
    start = [text_button('Open', 'Open a document', 'head.open'),
             menu_button(text='View', tip='View menu', cid='head.view-menu',
                         menu=parts.view_menu(APP)),
             menu_button(text='Insert', tip='Insert menu', cid='head.insert-menu',
                         menu=parts.insert_menu(APP, objects=True))]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('image-x-generic', 'Media', 'head.media', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    fbar = tag(parts.format_bar_text('fmt', full=True), 'fmt.bar')
    thumbs = parts.thumbs_list('page', 5, 2, cid='side.thumbnails')
    thumbs.set_size_request(118, -1)
    inner = shell.split(inspector_a(context), doc_view(context), shell.INSPECTOR_A_W, Gtk.PackType.END)
    body = shell.split(thumbs, inner, 118)
    page_entry = Gtk.Entry(text='6', width_chars=3, max_width_chars=3, valign=Gtk.Align.CENTER)
    bottom = shell.bottom_bar(
        parts.flat_menu('125%', 'Zoom', 'bottom.zoom', menu=parts.zoom_menu()),
        icon_button('go-up', 'Previous page', 'bottom.previous'),
        icon_button('go-down', 'Next page', 'bottom.next'),
        parts.flat_menu('Page', 'Browse by', 'bottom.navigate-by',
                        menu=menu_model(['Page', 'Section', 'Page break', 'Comment', 'Footnote',
                                         'Link', 'Paragraph style'])),
        tag(page_entry, 'bottom.go-to'),
        expand(),
        tag(label('26,171 words · 41 pages', 'dimmed', 'tiny', 'numeric', margin_end=8), 'bottom.count'))
    win.set_content(shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom]))


def version_b(win, context='text', level=shell.FORMAT, find=False, banner=False, low_res=False):
    side = shell.nav_sidebar(APP, parts.thumbs_list('page', 5, 2), SIDE_MODES, 'Add page', ADD_PAGE)
    start = [split_button(icon='sidebar-show', tip='Sidebar', drop_tip='View',
                          cid='head.view', menu=parts.view_menu(APP)),
             spacer(),
             parts.insert_menu_button(APP)]
    start += parts.insert_buttons()
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('edit-find', 'Find', 'head.find', toggle=True, active=find)]
    hb = shell.header_bar(APP, start, end)
    if context in ('image', 'shape'):
        fbar = tag(parts.format_bar_object('fmt', context, slim=True, writer=True), 'fmt.bar')
    else:
        fbar = tag(parts.format_bar_text('fmt', slim=True), 'fmt.bar')
    tops = [hb, fbar]
    if find:
        tops.append(find_bar())
    if low_res:
        tops.append(tag(Adw.Banner(title='Screen smaller than 1280 × 720 ‒ parts of the window may not fit',
                                   button_label='Close', revealed=True), 'notice.resolution'))
    if banner:
        tops.append(tag(Adw.Banner(title='This document was opened with 3 warnings', button_label='Show warnings',
                                   revealed=True), 'notice.banner'))
    body = shell.split(inspector_b(context, level), doc_view(context), shell.INSPECTOR_W, Gtk.PackType.END)
    bottom = shell.bottom_bar(
        parts.flat_menu('125%', 'Zoom', 'bottom.zoom', menu=parts.zoom_menu()),
        expand(),
        icon_button('go-up', 'Previous page', 'bottom.previous'),
        parts.flat_menu('Page 6 of 41', 'Go to and browse by', 'bottom.page', arrow=False,
                        menu=menu_model(['Go to page…'],
                                        [{'title': 'Browse by'}, 'Page', 'Section', 'Comment',
                                         'Footnote', 'Link', 'Paragraph style'])),
        icon_button('go-down', 'Next page', 'bottom.next'),
        expand(),
        parts.flat_menu('26,171 words', 'Count', 'bottom.count', arrow=False,
                        menu=menu_model(['Words', 'Characters', 'Characters without spaces', 'Paragraphs',
                                         'Pages'], ['Show statistics'])))
    content = shell.toolbar_view(body, top=tops, bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def find_bar():
    entry = Gtk.SearchEntry(placeholder_text='Find in document…', hexpand=True, text='cost')
    entry.set_max_width_chars(28)
    row1 = hbox(tag(entry, 'find.field'),
                tag(label('2 of 7', 'dimmed', 'numeric'), 'find.counter'),
                icon_button('go-up', 'Previous', 'find.previous'),
                icon_button('go-down', 'Next', 'find.next'),
                menu_button(icon='emblem-system', tip='Find options', cid='find.options',
                            menu=menu_model(['Match case', 'Whole words'],
                                            [{'title': 'Advanced'}, 'Paragraph style…', 'In selection only',
                                             'Insert tab', 'Insert paragraph break'],
                                            ['Show all results in the sidebar'])),
                spacing=6)
    rep = Gtk.Entry(placeholder_text='Replace with…', hexpand=True)
    rep.set_max_width_chars(28)
    row2 = hbox(tag(rep, 'find.replace-field'),
                text_button('Replace', None, 'find.replace'),
                text_button('Replace All', None, 'find.replace-all'), spacing=6)
    box = vbox(row1, row2, spacing=6, halign=Gtk.Align.CENTER, margin_top=6, margin_bottom=6)
    box.set_size_request(560, -1)
    wrap = Gtk.Box(halign=Gtk.Align.FILL)
    wrap.append(expand()), wrap.append(box), wrap.append(expand())
    classes(wrap, 'toolbar')
    return tag(wrap, 'find.bar')


def version_c(win, context='text', level=shell.FORMAT):
    side = shell.nav_sidebar(APP, parts.thumbs_list('page', 5, 2), SIDE_MODES, 'Add page', ADD_PAGE)
    start = [icon_button('sidebar-show', 'Sidebar', 'head.sidebar', toggle=True, active=True),
             menu_button(icon='desk-insert', tip='Insert', cid='head.insert-all',
                         popover=insert_all_popover())]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu',
                       menu=parts.main_menu(APP, lean=True)),
           spacer(),
           icon_button('x-office-document', 'Document', 'head.document', toggle=True,
                       active=(level == 'Document')),
           icon_button('desk-format', 'Format', 'head.format', toggle=True, active=(level == shell.FORMAT)),
           spacer(),
           icon_button('edit-find', 'Find', 'head.find', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    zoom = shell.osd_pill(icon_button('zoom-out', 'Zoom out'), label('125%', 'numeric', margin_start=4, margin_end=4),
                          icon_button('zoom-in', 'Zoom in'), cid='osd.zoom',
                          halign=Gtk.Align.END, valign=Gtk.Align.END, margin_end=14, margin_bottom=12)
    words = shell.osd_pill(label('26,171 words', 'numeric', margin_start=10, margin_end=10, margin_top=5,
                                 margin_bottom=5), cid='osd.count',
                           halign=Gtk.Align.CENTER, valign=Gtk.Align.END, margin_bottom=12)
    body = shell.split(inspector_c(context, level), doc_view(context, rulers=False, overlays=(zoom, words)),
                       shell.INSPECTOR_W, Gtk.PackType.END)
    content = shell.toolbar_view(body, top=[hb])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def insert_all_popover(app=APP):
    """One Insert button: every kind of object behind a view switcher."""
    pop = Gtk.Popover()
    grid = Gtk.FlowBox(max_children_per_line=3, min_children_per_line=3, selection_mode=Gtk.SelectionMode.NONE,
                       row_spacing=8, column_spacing=8)
    for col in ['#3d5373', '#1c71d8', '#26a269', '#e66100', '#813d9c', '#5e5c64']:
        grid.append(parts.color_box(f'linear-gradient(180deg, {col} 0 26%, white 26% 50%, '
                                    f'color-mix(in srgb, {col} 12%, white) 50% 74%, white 74%)',
                                    'style-chip', w=74, h=52))
    stack = view_stack([('t', 'Table', 'desk-table', grid),
                        ('g', 'Chart', 'desk-chart', Gtk.Box()),
                        ('f', 'Shape', 'desk-shape', Gtk.Box()),
                        ('m', 'Media', 'desk-media', Gtk.Box())], 't')
    stack.set_vexpand(False)
    sw = inline_switcher(stack, 'ins.tabs', mode=Adw.InlineViewSwitcherDisplayMode.ICONS, homogeneous=True)
    rest = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(rest, 'boxed-list')
    for t, ic in (('Text box', 'desk-textbox'), ('Comment', 'desk-comment'),
                  ('Break, field or note…', 'desk-page-break')):
        rest.append(action_row(t, prefix=Gtk.Image(icon_name=ic + '-symbolic'), arrow=(ic == 'desk-page-break')))
    box = vbox(sw, stack, tag(rest, 'ins.list'), spacing=10, margin_top=12, margin_bottom=12, margin_start=12,
               margin_end=12)
    pop.set_child(box)
    return tag(pop, 'ins.popover')


SCENES = [
    Scene('writer-a', 'Desk Writer · A Classic', lambda w: version_a(w), APP, 'A'),
    Scene('writer-b', 'Desk Writer · B Equilibrada', lambda w: version_b(w), APP, 'B'),
    Scene('writer-c', 'Desk Writer · C Enxuta', lambda w: version_c(w), APP, 'C'),
]
