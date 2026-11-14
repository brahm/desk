# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The window of each version: where the bars and the panes go.

A  Classic   the iWork '09 arrangement in Adwaita: two bars above the
             document, panes under them, the Inspector with one icon per
             pane, a bar below. What Desk Writer is today.
B  Balanced  a navigation sidebar of full height, the insert buttons in the
             header bar, a slim contextual format bar, the Inspector as a
             utility pane with Format and Document, a slim bar below.
C  Lean      the arrangement of today's iWork: no format bar and no bar
             below; Format, Animate and Document are header buttons that
             open the pane; one Insert button.
"""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import parts
from kit import (classes, expand, hbox, icon_button, label, menu_button, menu_model, spacer,
                 split_button, tag, text_button, toggle_group, vbox)

SIDEBAR_W = 196
INSPECTOR_W = 332      # versions B and C: rows of Libadwaita
INSPECTOR_A_W = 290    # version A: the compact rows of Desk
# The first level of the Inspector of version B. They are views, so nouns.
FORMAT, ANIMATION, DATA, DOCUMENT = 'Formato', 'Animação', 'Dados', 'Documento'


def window_title(app, edited=True):
    doc = parts.APPS[app]['doc']
    return tag(Adw.WindowTitle(title=doc, subtitle='Documentos' + (' · Editado' if edited else '')),
               'cab.titulo')


def split(start, content, width, position=Gtk.PackType.START, collapsed=False, show=True):
    sv = Adw.OverlaySplitView(sidebar=start, content=content, sidebar_position=position,
                              min_sidebar_width=width, max_sidebar_width=width,
                              collapsed=collapsed, show_sidebar=show)
    return sv


def toolbar_view(content, top=(), bottom=(), style=None):
    tv = Adw.ToolbarView(content=content)
    for bar in top:
        if bar is not None:
            tv.add_top_bar(bar)
    for bar in bottom:
        if bar is not None:
            tv.add_bottom_bar(bar)
    if style is not None:
        tv.set_top_bar_style(style)
        tv.set_bottom_bar_style(style)
    return tv


def nav_sidebar(app, content, modes, add_tip, add_menu=None, title=None, add_popover=None):
    """The navigation sidebar of versions B and C, as high as the window."""
    hb = Adw.HeaderBar()
    mode = menu_button(text=title or modes[0], tip='Conteúdo da barra lateral', cid='lat.modo',
                       menu=menu_model(modes))
    classes(mode, 'flat')
    hb.set_title_widget(mode)
    add = menu_button(icon='list-add', tip=add_tip, cid='lat.adicionar', popover=add_popover,
                      menu=add_menu or menu_model(['Em branco']))
    hb.pack_end(add)
    return tag(toolbar_view(content, top=[hb]), 'lat.barra')


def inspector_pane(*children, cid='insp.painel', width=None):
    box = vbox(*children)
    box.set_size_request(width or INSPECTOR_W, -1)
    return tag(box, cid)


def level1(names, active=0, cid='insp.nivel1'):
    g = toggle_group([(n, n, None, None) for n in names], active, cid, 'round', homogeneous=len(names) == 2)
    classes(g, 'inspector-switch')
    return g


def context_title(text, cid='insp.contexto'):
    lab = label(text, 'heading', margin_start=14, margin_top=12, margin_bottom=2)
    return tag(lab, cid)


def bottom_bar(*children, cid='inf.barra'):
    bar = hbox(*children, spacing=4)
    classes(bar, 'toolbar', 'bottombar')
    return tag(bar, cid)


def osd_pill(*children, cid=None, **kw):
    box = hbox(*children, spacing=2, **kw)
    classes(box, 'osd', 'toolbar', 'osd-pill')
    return tag(box, cid) if cid else box


def header_bar(app, start=(), end=(), title=True):
    hb = Adw.HeaderBar()
    if title:
        hb.set_title_widget(window_title(app))
    for w in start:
        hb.pack_start(w)
    for w in end:
        hb.pack_end(w)
    return tag(hb, 'cab.barra')


# ---------------------------------------------------------------- Inspector

from kit import inline_switcher, view_stack  # noqa: E402


def _tabs(pages, visible, *css, top=8):
    stack = view_stack(pages, visible)
    if len(pages) == 1:
        return [stack]             # one page needs no switcher
    sw = inline_switcher(stack, 'insp.abas', *css, margin_start=12, margin_end=12, margin_top=top,
                         homogeneous=len(pages) <= 3 and max(len(p[1]) for p in pages) <= 8)
    return [sw, stack]


def inspector_b(levels, level, context=None, head=None, pages=(), visible=None):
    """Version B: Format, (Animate or Reorganize) and Document on top, then the
    pages for what is selected, with text labels."""
    kids = [level1(levels, levels.index(level))]
    if context:
        kids.append(context_title(context))
    if head is not None:
        head.set_margin_start(12), head.set_margin_end(12), head.set_margin_top(8)
        kids.append(head)
    kids += _tabs(pages, visible, 'flat')
    return inspector_pane(*kids)


def inspector_c(head=None, pages=(), visible=None):
    """Version C: the header buttons choose Format, Animate or Document; the
    pane starts at the pages for what is selected."""
    kids = []
    if head is not None:
        head.set_margin_start(12), head.set_margin_end(12), head.set_margin_top(12)
        kids.append(head)
    kids += _tabs(pages, visible, *(('flat',) if len(pages) > 3 else ()), top=10)
    return inspector_pane(*kids)


def compact_row(title, value, unit='', digits=1):
    spin = Gtk.SpinButton.new_with_range(0, 999, 0.1)
    spin.set_digits(digits)
    spin.set_value(value)
    spin.set_width_chars(5)
    spin.set_valign(Gtk.Align.CENTER)
    return plain_row(hbox(label(title, hexpand=True, wrap=True), spin,
                          label(unit, 'dimmed', 'tiny', width_chars=3), spacing=8), 3)


def plain_row(child, pad=5):
    row = Gtk.ListBoxRow(activatable=False)
    child.set_margin_start(10), child.set_margin_end(8), child.set_margin_top(pad), child.set_margin_bottom(pad)
    row.set_child(child)
    return row


def switch_line(title, active=False):
    return plain_row(hbox(label(title, hexpand=True, wrap=True), Gtk.Switch(active=active, valign=Gtk.Align.CENTER)))


def combo_line(title, items, selected=0):
    dd = Gtk.DropDown.new_from_strings(items)
    dd.set_selected(selected)
    dd.set_valign(Gtk.Align.CENTER)
    return plain_row(hbox(label(title, hexpand=True), dd, spacing=8), 3)


def compact_group(title, *rows, cid=None, suffix=None):
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(lb, 'boxed-list')
    for r in rows:
        lb.append(r)
    box = vbox(hbox(label(title, 'heading', hexpand=True), suffix, margin_bottom=6), lb)
    return tag(box, cid) if cid else box


def inspector_a(icons, active, title, subtabs, groups, sub_active=0):
    """Version A: one icon per pane of the iWork '09 Inspector, in its order;
    the pane does not change by itself when the selection changes."""
    tabs = toggle_group([(n, None, i, t) for n, i, t in icons], active, 'insp.icones', 'flat', 'icon-tabs',
                        halign=Gtk.Align.CENTER, margin_top=8)
    kids = [tabs, context_title(title)]
    if subtabs:
        kids.append(toggle_group([(s, s, None, None) for s in subtabs], sub_active, 'insp.subabas',
                                 margin_start=12, margin_end=12, margin_top=8,
                                 homogeneous=len(subtabs) <= 3))
    body = vbox(*groups, spacing=14)
    classes(body, 'inspector-page')
    sw = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    sw.set_child(body)
    kids.append(sw)
    return inspector_pane(*kids, width=INSPECTOR_A_W)
