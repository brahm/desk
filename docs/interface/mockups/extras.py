# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""More views of version B and the pieces the three applications share:
other selections, open menus and popovers, dialogs, narrow windows, dark."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, GLib, Gtk  # noqa: E402

import canvas
import kit
import parts
import shell
import slides
import tables
import writer
from kit import (Scene, action_row, button_row, classes, combo_row, expand, group, hbox, icon_button, label,
                 menu_button, menu_model, spacer, spin_row, split_button, switch_row, tag, text_button,
                 toggle_group, vbox)


def popup(cid):
    """Open the menu or the popover of the button tagged `cid`."""
    return lambda win: kit.find(cid).popup()


# ------------------------------------------------------------- narrow Writer


def writer_narrow(win):
    """Version B in half of the smallest screen Desk supports (1280 × 720),
    below the 720 px breakpoint: the sidebar becomes an overlay and the
    insert buttons go into the Insert menu. The Inspector stays beside the
    page down to 520 px, and over it below that."""
    side = shell.nav_sidebar('writer', parts.thumbs_list('page', 5, 2), writer.SIDE_MODES, 'Add page',
                             writer.ADD_PAGE)
    start = [icon_button('sidebar-show', 'Sidebar', 'head.sidebar', toggle=True),
             parts.insert_menu_button('writer')]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu('writer')),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('edit-find', 'Find', 'head.find', toggle=True)]
    hb = Adw.HeaderBar()
    title = Adw.WindowTitle(title='Annual report 2026', subtitle='Documentos · Editado')
    hb.set_title_widget(tag(title, 'head.title'))
    for w in start:
        hb.pack_start(w)
    for w in end:
        hb.pack_end(w)
    tag(hb, 'head.bar')
    fbar = tag(parts.format_bar_text('fmt', slim=True, compact='medium'), 'fmt.bar')
    doc = writer.doc_view('text', rulers=False)
    body = shell.split(writer.inspector_b('text'), doc, shell.INSPECTOR_W, Gtk.PackType.END)
    bottom = shell.bottom_bar(parts.flat_menu('100%', 'Zoom', 'bottom.zoom', menu=parts.zoom_menu()), expand(),
                              parts.flat_menu('Page 6 of 41', 'Go to and browse by', 'bottom.page',
                                              arrow=False, menu=menu_model(['Go to page…'])))
    content = shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W, collapsed=True, show=False))


# ------------------------------------------------------------ light table


def slides_light_table(win):
    start = [split_button(icon='sidebar-show', tip='Sidebar', drop_tip='View',
                          cid='head.view', menu=parts.view_menu('slides')),
             spacer(), parts.insert_menu_button('slides')]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu('slides')),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('edit-find', 'Find', 'head.find', toggle=True),
           spacer(),
           split_button(icon='media-playback-start', tip='Play', drop_tip='Slideshow options',
                        cid='head.play', menu=slides.PLAY_MENU)]
    hb = shell.header_bar('slides', start, end)
    table = Gtk.ScrolledWindow(child=canvas.light_table(12, 3, 196), hscrollbar_policy=Gtk.PolicyType.NEVER,
                               vexpand=True, hexpand=True)
    classes(table, 'canvas')
    tag(table, 'doc.light-table')
    body = shell.split(slides.inspector_b('slide', shell.FORMAT), table, shell.INSPECTOR_W, Gtk.PackType.END)
    size = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
    size.set_value(55), size.set_size_request(140, -1), size.set_draw_value(False)
    size.set_tooltip_text('Thumbnail size')
    bottom = shell.bottom_bar(tag(size, 'bottom.thumbnail-size'), expand(),
                              tag(label('12 slides · 1 selected', 'dimmed', 'tiny', 'numeric', margin_end=8),
                                  'bottom.count'))
    win.set_content(shell.toolbar_view(body, top=[hb], bottom=[bottom]))


# --------------------------------------------------------- template chooser


def template_chooser(win):
    """The chooser inside the window: what a new window shows before there is
    a document. Categories on the left, the templates as a grid."""
    hb = Adw.HeaderBar()
    hb.set_title_widget(Adw.WindowTitle(title='New Document', subtitle='Desk Writer'))
    open_ = Gtk.Button(child=Adw.ButtonContent(icon_name='document-open-symbolic', label='Open…'),
                       tooltip_text='Open a document')
    hb.pack_start(tag(open_, 'templates.open'))
    hb.pack_end(menu_button(icon='open-menu', tip='Main menu', cid='head.menu',
                            menu=parts.main_menu('writer')))
    hb.pack_end(tag(icon_button('edit-find', 'Search templates', toggle=True), 'templates.search'))
    sb = Adw.Sidebar()
    s1 = Adw.SidebarSection()
    for n, ic in (('All', 'view-grid'), ('Recent', 'document-open-recent'), ('My Templates', 'user-bookmarks')):
        s1.append(Adw.SidebarItem(title=n, icon_name=ic + '-symbolic'))
    s2 = Adw.SidebarSection(title='Word Processing')
    for n in ('Blank', 'Letters', 'Envelopes', 'Forms', 'Résumés', 'Reports', 'Outline'):
        s2.append(Adw.SidebarItem(title=n))
    s3 = Adw.SidebarSection(title='Page Layout')
    for n in ('Blank', 'Newsletters', 'Brochures', 'Posters', 'Cards and Invitations', 'Business Cards'):
        s3.append(Adw.SidebarItem(title=n))
    for s in (s1, s2, s3):
        sb.append(s)
    sb.set_selected(0)
    side = shell.toolbar_view(tag(sb, 'templates.categories'), top=[Adw.HeaderBar(show_title=False)])
    flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=5, min_children_per_line=3,
                       column_spacing=10, row_spacing=12, margin_top=18, margin_bottom=18, margin_start=24,
                       margin_end=24, valign=Gtk.Align.START, homogeneous=True)
    names = ['Blank', 'Blank (landscape)', 'Classic letter', 'Modern letter', 'School report',
             'Project report', 'Résumé', 'Proposal', 'Newsletter', 'Brochure']
    kinds = ['text', 'text', 'text', 'text', 'cover', 'image', 'text', 'table', 'image', 'cover']
    for i, (n, k) in enumerate(zip(names, kinds)):
        th = canvas.page_thumb(i + 1, selected=False, kind=k if i > 1 else 'blank')
        th.set_halign(Gtk.Align.CENTER)
        card = vbox(th, label(n, xalign=0.5, wrap=True, justify=Gtk.Justification.CENTER, max_width_chars=16),
                    spacing=6)
        classes(card, 'template-card')
        if i == 2:
            classes(card, 'selected')
        flow.append(card)
    grid = Gtk.ScrolledWindow(child=flow, hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    classes(grid, 'view')
    tag(grid, 'templates.grid')
    create = classes(Gtk.Button(label='Create'), 'suggested-action', 'pill')
    create.set_size_request(120, -1)
    bar = hbox(tag(Gtk.CheckButton(label='Show when Desk Writer opens', active=True), 'templates.show-on-open'),
               expand(), tag(create, 'templates.create'), spacing=8)
    classes(bar, 'toolbar')
    content = shell.toolbar_view(grid, top=[hb], bottom=[bar])
    win.set_content(tag(shell.split(side, content, 220), 'templates.window'))


# ------------------------------------------------------------------ dialogs


def close_dialog(win):
    d = Adw.AlertDialog(heading='Save the changes?',
                        body='The document “Annual report 2026” has changes that were not saved. '
                             'If you do not save, they will be lost.')
    d.add_response('cancel', 'Cancel')
    d.add_response('discard', 'Discard')
    d.add_response('save', 'Save')
    d.set_response_appearance('discard', Adw.ResponseAppearance.DESTRUCTIVE)
    d.set_response_appearance('save', Adw.ResponseAppearance.SUGGESTED)
    d.set_default_response('save')
    d.set_close_response('cancel')
    tag(d, 'dialog.close')
    d.present(win)


def preferences_dialog(win):
    d = Adw.PreferencesDialog()
    general = Adw.PreferencesPage(title='General', icon_name='emblem-system-symbolic')
    general.add(group('New documents',
                      combo_row('When creating a document', ['Show the template chooser', 'Use the chosen template']),
                      action_row('Template', subtitle='Blank', arrow=True, prop=True), cid='pref.new'))
    general.add(group('Editing', switch_row('Show size and position when moving objects', True),
                      switch_row('Show the word count', True),
                      switch_row('Automatic lists', True), cid='pref.editing'))
    general.add(group('Saving', switch_row('Keep a recovery draft', True,
                                               'Every minute, while there are changes'),
                      switch_row('Include a preview in the document', True), cid='pref.saving'))
    general.add(group('Author', Adw.EntryRow(title='Name in comments and changes', text='Ana Souza'),
                      cid='pref.author'))
    rulers = Adw.PreferencesPage(title='Rulers', icon_name='desk-ruler-symbolic')
    rulers.add(group('Units', combo_row('Ruler units', ['Centimeters', 'Millimeters', 'Inches',
                                                                    'Points', 'Picas']),
                     switch_row('Origin at the center of the ruler'), switch_row('Units as percentage')))
    rulers.add(group('Alignment guides', switch_row('Guides at the center of the object', True),
                     switch_row('Guides at the edges of the object', True)))
    auto = Adw.PreferencesPage(title='Auto-Correction', icon_name='tools-check-spelling-symbolic')
    auto.add(group('Text', switch_row('Smart quotes', True), switch_row('Capitalize sentences'),
                   switch_row('Detect links and addresses', True)))
    for p in (general, rulers, auto):
        d.add(p)
    tag(d, 'dialog.preferences')
    d.present(win)


def shortcuts_dialog(win):
    d = Adw.ShortcutsDialog()
    data = [
        ('Document', [('New Document', '<Control>n'), ('Open', '<Control>o'), ('Save', '<Control>s'),
                       ('Save As', '<Control><Shift>s'), ('Print', '<Control>p'),
                       ('Close the window', '<Control>w')]),
        ('Panes', [('Sidebar', 'F9'), ('Inspector', '<Control><Alt>i'), ('Ruler', '<Control>r'),
                     ('Format bar', '<Control><Shift>r'), ('Fullscreen', 'F11'),
                     ('Search commands', '<Control>slash')]),
        ('Text', [('Bold', '<Control>b'), ('Italic', '<Control>i'), ('Underline', '<Control>u'),
                   ('Paragraph styles 1 to 8', '<Control>1'), ('Fonts', '<Control>t'),
                   ('Colors', '<Control><Shift>c'), ('Paste and match style', '<Control><Shift>v')]),
        ('Objects', [('Duplicate', '<Control>d'), ('Lock', '<Control>l'), ('Group', '<Control><Alt>g'),
                     ('Bring to front', '<Control><Shift>bracketright'), ('Send to back', '<Control><Shift>bracketleft')]),
    ]
    for title, items in data:
        sec = Adw.ShortcutsSection(title=title)
        for name, acc in items:
            sec.add(Adw.ShortcutsItem(title=name, accelerator=acc))
        d.add(sec)
    tag(d, 'dialog.shortcuts')
    d.present(win)


def command_dialog(win):
    """Search the commands: every item the iWork menu bar had, by name."""
    d = Adw.Dialog(title='Search commands', content_width=460, content_height=420)
    entry = Gtk.SearchEntry(text='break', placeholder_text='Search Commands…', hexpand=True)
    hb = Adw.HeaderBar()
    hb.set_title_widget(tag(entry, 'commands.field'))
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(lb, 'navigation-sidebar')
    rows = [('Insert page break', 'Insert', '<Control>Return'),
            ('Insert column break', 'Insert', None),
            ('Insert section break', 'Insert', None),
            ('Insert layout break', 'Insert', None),
            ('Show invisibles (breaks and spaces)', 'View', '<Control><Shift>i'),
            ('Paragraph starts on a new page', 'Format › Text', None),
            ('Remove page breaks from the selection', 'Edit', None)]
    for i, (name, where, acc) in enumerate(rows):
        r = Adw.ActionRow(title=name, subtitle=where)
        if acc:
            r.add_suffix(Adw.ShortcutLabel(accelerator=acc, valign=Gtk.Align.CENTER))
        lb.append(r)
    lb.select_row(lb.get_row_at_index(0))
    sw = Gtk.ScrolledWindow(child=tag(lb, 'commands.list'), hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    d.set_child(shell.toolbar_view(sw, top=[hb]))
    tag(d, 'commands.dialog')
    d.present(win)


def wb(**kw):
    return lambda w: writer.version_b(w, **kw)


SCENES = [
    Scene('writer-b-image', 'Writer B · image selected', wb(context='image'), 'writer', 'B',
          note='The format bar and the Inspector change with the selection.'),
    Scene('writer-b-shape', 'Writer B · shape selected', wb(context='shape'), 'writer', 'B'),
    Scene('writer-b-document', 'Writer B · Document', wb(level=shell.DOCUMENT), 'writer', 'B'),
    Scene('writer-b-find', 'Writer B · find and notices', wb(find=True, banner=True), 'writer', 'B'),
    Scene('writer-b-shapes', 'Writer B · Shape popover', wb(), 'writer', 'B', after=popup('head.insert.shape')),
    Scene('writer-b-menu', 'Writer B · main menu', wb(), 'writer', 'B', after=popup('head.menu')),
    Scene('writer-b-view', 'Writer B · View menu', wb(), 'writer', 'B',
          after=popup('head.view')),
    Scene('writer-b-insert', 'Writer B · Insert menu', wb(), 'writer', 'B', after=popup('head.insert')),
    Scene('writer-b-fonts', 'Writer B · Fonts popover', wb(), 'writer', 'B', after=popup('fmt.font')),
    Scene('writer-b-colors', 'Writer B · Colors popover', wb(), 'writer', 'B', after=popup('fmt.text-color')),
    Scene('writer-c-insert', 'Writer C · Insert', lambda w: writer.version_c(w), 'writer', 'C',
          after=popup('head.insert-all')),
    Scene('tables-b-data', 'Tables B · Data', lambda w: tables.version_b(w, level=shell.DATA), 'tables', 'B'),
    Scene('tables-b-chart', 'Tables B · chart selected', lambda w: tables.version_b(w, selection='chart'),
          'tables', 'B'),
    Scene('tables-b-formula', 'Tables B · editing a formula',
          lambda w: tables.version_b(w, selection='cell', editing=True), 'tables', 'B'),
    Scene('tables-b-document', 'Tables B · Document', lambda w: tables.version_b(w, level=shell.DOCUMENT),
          'tables', 'B'),
    Scene('slides-b-slide', 'Slides B · nothing selected', lambda w: slides.version_b(w, context='slide',
                                                                                     selection='slide'),
          'slides', 'B', note='With nothing selected, the format bar and the Inspector show what belongs to the slide.'),
    Scene('slides-b-transition', 'Slides B · transition',
          lambda w: slides.version_b(w, context='slide', level=shell.ANIMATION, selection='slide'), 'slides', 'B'),
    Scene('slides-b-document', 'Slides B · Document', lambda w: slides.version_b(w, level=shell.DOCUMENT),
          'slides', 'B'),
    Scene('slides-b-animation', 'Slides B · Animation', lambda w: slides.version_b(w, level=shell.ANIMATION),
          'slides', 'B'),
    Scene('slides-b-light-table', 'Slides B · light table', slides_light_table, 'slides', 'B'),
    Scene('common-templates', 'Template chooser in the window', template_chooser, 'common', 'B'),
    Scene('common-close', 'Close with changes', wb(), 'common', 'B', after=close_dialog),
    Scene('common-preferences', 'Preferences', wb(), 'common', 'B', after=preferences_dialog),
    Scene('common-shortcuts', 'Keyboard Shortcuts', wb(), 'common', 'B', after=shortcuts_dialog),
    Scene('common-commands', 'Search commands', wb(), 'common', 'B', after=command_dialog),
    Scene('common-half', 'Half screen (640 × 720)', writer_narrow, 'common', 'B', size=(640, 720),
          note='Half of the minimum screen, side by side with another window.'),
    Scene('common-notice', 'Screen smaller than the minimum (1024 × 600)', wb(low_res=True), 'common', 'B', size=(1024, 600),
          note='The banner that warns, on opening, that the screen has less than 1280 × 720.'),
    Scene('common-dark', 'Dark style', wb(), 'common', 'B', dark=True),
]
