# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pieces the three applications share: menus, popovers, Inspector pages."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import canvas
from kit import (action_row, button_row, classes, color_box, combo_row, dropdown, expander_row,
                 expand, group, hbox, icon_button, inline_switcher, label, menu_button, menu_model,
                 page, scale_row, spacer, spin_row, split_button, swatch, switch_row, tag,
                 text_button, toggle_group, vbox, view_stack, vsep)

APPS = {
    'writer': {'name': 'Desk Writer', 'doc': 'Annual report 2026', 'ext': 'odt'},
    'tables': {'name': 'Desk Tables', 'doc': 'Budget 2027', 'ext': 'ods'},
    'slides': {'name': 'Desk Slides', 'doc': 'Launch plan', 'ext': 'odp'},
}

# ------------------------------------------------------------------- menus


def main_menu(app, lean=False):
    """The primary menu: the file and the application, twelve items at most.
    What iWork kept in the menu bar and is not here is found by Search Commands."""
    name = APPS[app]['name']
    export = {'writer': ['PDF…', 'Word…', 'RTF…', 'Plain text…', 'EPUB…'],
              'tables': ['PDF…', 'Excel…', 'CSV…'],
              'slides': ['PDF…', 'PowerPoint…', 'Images…', 'Movie…', 'HTML…']}[app]
    recent = ('Open Recent', [['Annual report 2026', 'March minutes', 'Proposal'], ['Clear list']])
    files = ['Save As…', ('Export', [export, ['Send by email…']]), 'Print…']
    if lean:
        view = {'writer': ['Ruler', 'Invisibles', 'Layout', 'Comments', 'Word count'],
                'tables': ['Rulers', 'Formula bar', 'Comments', 'Formula list'],
                'slides': ['Rulers', 'Presenter notes', 'Comments', 'Light table', 'Outline']}[app]
        sections = [['New Document', 'Open…', recent], files,
                    [('View', [view, ['Fullscreen']]), 'Search Commands…']]
    else:
        sections = [['New Document', 'Open…', recent], ['Save'] + files, ['Search Commands…']]
    sections.append(['Preferences', 'Keyboard Shortcuts', 'Help', f'About {name}'])
    return menu_model(*sections)


def view_menu(app):
    """The View menu of iWork, as the secondary menu of the window."""
    if app == 'writer':
        return menu_model(
            [{'title': 'Sidebar'}, 'Page thumbnails', 'Styles', 'Comments', 'Search results'],
            ['Ruler', 'Format bar', 'Layout', 'Invisibles', 'Track changes'],
            ['Outline mode', 'Fullscreen'],
            [('Zoom', [['Zoom in', 'Zoom out', 'Actual size'], ['Fit width', 'Fit page'],
                       ['One page', 'Two pages']])])
    if app == 'tables':
        return menu_model(
            [{'title': 'Sidebar'}, 'Sheets', 'Table styles', 'Comments', 'Search results'],
            ['Rulers', 'Format bar', 'Formula bar', 'Formula list'],
            ['Print preview', 'Fullscreen'],
            [('Zoom', [['Zoom in', 'Zoom out', 'Actual size'], ['Fit in window']])])
    return menu_model(
        [{'title': 'Mode'}, 'Navigator', 'Outline', 'Slide only', 'Light table'],
        ['Slide layouts', 'Presenter notes', 'Rulers', 'Format bar', 'Comments'],
        ['Fullscreen'],
        [('Zoom', [['Zoom in', 'Zoom out', 'Actual size'], ['Fit in window']])])


def insert_menu(app, objects=False):
    """What iWork keeps in the Insert menu besides the object buttons; with
    `objects`, the objects too (version A, and version B in a narrow window)."""
    obj = ['Table', 'Chart', 'Text box', 'Shape', 'Image, movie or sound…', 'Comment']
    if app == 'writer':
        rest = [[('Break', [['Page', 'Column', 'Section', 'Layout']]),
                 ('Field', [['Page number', 'Page count', 'Date and time', 'File name',
                             'Merge field']])],
                ['Footnote', 'Table of contents', 'Bookmark', 'Link…'],
                ['Special characters…', 'Choose…']]
    elif app == 'tables':
        rest = [['Sheet', ('Rows and columns', [['Row above', 'Row below'], ['Column before', 'Column after']])],
                ['Function', 'Date and time', 'File name', 'Page number'],
                ['Link…', 'Special characters…', 'Choose…']]
    else:
        rest = [['Slide', 'Slide number', 'Date and time'],
                ['Smart build', 'Link…', 'Web view'],
                ['Special characters…', 'Choose…']]
    return menu_model(*(([obj] if objects else []) + rest))


def zoom_menu():
    return menu_model(['Fit width', 'Fit page'],
                      ['50%', '75%', '100%', '125%', '150%', '200%', '400%'],
                      ['One page', 'Two pages'])


def gallery_popover(kind):
    """The popover of an insert button: a small gallery, as in iWork."""
    pop = Gtk.Popover()
    box = vbox(spacing=10, margin_top=12, margin_bottom=12, margin_start=12, margin_end=12)
    if kind == 'shape':
        box.append(classes(Gtk.SearchEntry(placeholder_text='Search shapes…'), 'gallery-search'))
        cats = toggle_group([('b', 'Basic', None, None), ('g', 'Geometry', None, None),
                             ('s', 'Arrows', None, None), ('o', 'Objects', None, None)], 0, None, 'flat')
        box.append(cats)
        flow = Gtk.FlowBox(max_children_per_line=6, min_children_per_line=6,
                           selection_mode=Gtk.SelectionMode.NONE, row_spacing=4, column_spacing=4)
        for icon in ['desk-fill', 'desk-stroke', 'media-record', 'starred', 'go-next', 'go-previous',
                     'desk-comment', 'desk-shape', 'media-playback-start', 'pan-up', 'desk-animate',
                     'list-add', 'desk-bookmark', 'user-bookmarks', 'view-pin', 'emote-love',
                     'weather-clear', 'mail-attachment']:
            b = Gtk.Button(icon_name=icon + '-symbolic')
            classes(b, 'flat')
            b.get_first_child().set_pixel_size(24) if b.get_first_child() else None
            flow.append(b)
        box.append(flow)
        box.append(button_row_plain('Draw with the pen'))
    elif kind in ('table', 'chart'):
        flow = Gtk.FlowBox(max_children_per_line=3, min_children_per_line=3,
                           selection_mode=Gtk.SelectionMode.NONE, row_spacing=8, column_spacing=8)
        if kind == 'chart':
            sw = toggle_group([('2d', '2D', None, None), ('3d', '3D', None, None),
                               ('i', 'Interactive', None, None)], 0, None)
            box.append(sw)
        cols = ['#3d5373', '#1c71d8', '#26a269', '#e66100', '#813d9c', '#5e5c64']
        for i, col in enumerate(cols):
            flow.append(color_box(f'linear-gradient(180deg, {col} 0 26%, white 26% 50%, '
                                  f'color-mix(in srgb, {col} 12%, white) 50% 74%, white 74%)'
                                  if kind == 'table' else
                                  f'linear-gradient(90deg, white 0 12%, {col} 12% 30%, white 30% 41%, '
                                  f'color-mix(in srgb, {col} 55%, white) 41% 59%, white 59% 70%, {col} 70% 88%, white 88%)',
                                  'style-chip', w=74, h=52))
        box.append(flow)
    elif kind == 'media':
        sw = toggle_group([('f', 'Photos', None, None), ('m', 'Music', None, None),
                           ('v', 'Movies', None, None)], 0, None)
        box.append(sw)
        flow = Gtk.FlowBox(max_children_per_line=4, min_children_per_line=4,
                           selection_mode=Gtk.SelectionMode.NONE, row_spacing=6, column_spacing=6)
        for col in ['#b5d6f2', '#f6d32d', '#8ff0a4', '#f66151', '#dc8add', '#cdab8f', '#99c1f1', '#57e389']:
            flow.append(color_box(f'linear-gradient(160deg, {col}, color-mix(in srgb, {col} 60%, black))',
                                  'style-chip', w=58, h=44))
        box.append(flow)
        box.append(button_row_plain('Choose file…'))
    pop.set_child(box)
    return pop


def button_row_plain(text):
    b = Gtk.Button(label=text)
    classes(b, 'flat')
    b.get_child().set_xalign(0)
    return b


INSERT = [  # cid suffix, icon, tooltip, popover kind
    ('table', 'desk-table', 'Table', 'table'),
    ('chart', 'desk-chart', 'Chart', 'chart'),
    ('text', 'desk-textbox', 'Text box', None),
    ('shape', 'desk-shape', 'Shape', 'shape'),
    ('media', 'desk-media', 'Media', 'media'),
    ('comment', 'desk-comment', 'Comment', None),
]


def insert_menu_button(app, cid='head.insert'):
    """Insert, with an icon and a label: what the Insert menu of iWork keeps
    besides the objects, and the objects too when the window is narrow."""
    return menu_button(icon='desk-insert', text='Insert', tip='Insert menu', cid=cid, arrow=False,
                       menu=insert_menu(app))


def insert_buttons(prefix='head.insert'):
    """Table, Chart, Text, Shape, Media and Comment, one button each."""
    out = []
    for suf, icon, tip, kind in INSERT:
        if kind:
            out.append(menu_button(icon=icon, tip=tip, cid=f'{prefix}.{suf}', popover=gallery_popover(kind)))
        else:
            out.append(icon_button(icon, tip, f'{prefix}.{suf}'))
    return out


# -------------------------------------------------------------- format bars


PALETTE = [  # the GNOME palette, five shades of nine colours
    ['#99c1f1', '#8ff0a4', '#f9f06b', '#ffbe6f', '#f66151', '#dc8add', '#cdab8f', '#ffffff', '#77767b'],
    ['#62a0ea', '#57e389', '#f8e45c', '#ffa348', '#ed333b', '#c061cb', '#b5835a', '#f6f5f4', '#5e5c64'],
    ['#3584e4', '#33d17a', '#f6d32d', '#ff7800', '#e01b24', '#9141ac', '#986a44', '#deddda', '#3d3846'],
    ['#1c71d8', '#2ec27e', '#f5c211', '#e66100', '#c01c28', '#813d9c', '#865e3c', '#c0bfbc', '#241f31'],
    ['#1a5fb4', '#26a269', '#e5a50a', '#c64600', '#a51d2d', '#613583', '#63452c', '#9a9996', '#000000'],
]


def colors_popover(cid='colors.popover', title='Text color'):
    """Colours as a popover: the palette, the colours of the document, the
    eyedropper and a custom colour. It takes the place of the Colors window."""
    pop = Gtk.Popover()
    grid = Gtk.Grid(row_spacing=3, column_spacing=3)
    for r, row in enumerate(PALETTE):
        for c, col in enumerate(row):
            grid.attach(color_box(col, 'palette-cell'), c, r, 1, 1)
    recent = hbox(spacing=3)
    for col in ['#3d5373', '#e66100', '#1e1e1e', '#26a269']:
        recent.append(color_box(col, 'palette-cell'))
    actions = hbox(tag(icon_button('color-select', 'Eyedropper'), 'colors.eyedropper'),
                   tag(text_button('Custom…', None, None, 'flat'), 'colors.custom'), expand(),
                   tag(text_button('None', None, None, 'flat'), 'colors.none'), spacing=4)
    box = vbox(label(title, 'heading'), tag(grid, 'colors.palette'), label('From the document', 'dimmed', 'tiny'),
               tag(recent, 'colors.document'), actions, spacing=8, margin_top=12, margin_bottom=10,
               margin_start=12, margin_end=12)
    pop.set_child(box)
    return tag(pop, cid)


def fonts_popover(cid='fonts.popover'):
    """Fonts as a popover: search, the fonts of the document, then all of them,
    each written in itself. It takes the place of the Fonts window."""
    pop = Gtk.Popover()
    entry = Gtk.SearchEntry(placeholder_text='Search fonts…')
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(lb, 'navigation-sidebar')
    fams = ['TeX Gyre Termes', 'Adwaita Sans', 'Adwaita Mono', 'Carlito', 'Caladea', 'DejaVu Sans',
            'Liberation Serif', 'TeX Gyre Heros', 'TeX Gyre Pagella']
    for i, fam in enumerate(fams):
        lab = Gtk.Label(xalign=0)
        lab.set_markup(f'<span font_family="{fam}">{fam}</span>')
        classes(lab, 'font-sample')
        row = Gtk.ListBoxRow(child=hbox(lab, expand(), Gtk.Image(icon_name='object-select-symbolic') if i == 0
                                        else None))
        lb.append(row)
        if i == 1:
            lb.set_header_func(None)
    lb.select_row(lb.get_row_at_index(0))
    sw = Gtk.ScrolledWindow(child=lb, hscrollbar_policy=Gtk.PolicyType.NEVER, min_content_height=300,
                            min_content_width=250)
    foot = button_row_plain('Typeface, spacing and ligatures in the Inspector')
    box = vbox(tag(entry, 'fonts.find'), label('In the document and recent', 'dimmed', 'tiny', margin_start=8),
               tag(sw, 'fonts.list'), Gtk.Separator(), tag(foot, 'fonts.more'), spacing=6, margin_top=10,
               margin_bottom=6, margin_start=6, margin_end=6)
    pop.set_child(box)
    return tag(pop, cid)



def align_group(cid=None, *css):
    return toggle_group([('l', None, 'format-justify-left', 'Align left'),
                         ('c', None, 'format-justify-center', 'Center'),
                         ('r', None, 'format-justify-right', 'Align right'),
                         ('j', None, 'format-justify-fill', 'Justify')], 3, cid, *css)


def biu(prefix, strike=False):
    out = [icon_button('format-text-bold', 'Bold', f'{prefix}.bold', toggle=True),
           icon_button('format-text-italic', 'Italic', f'{prefix}.italic', toggle=True),
           icon_button('format-text-underline', 'Underline', f'{prefix}.underline', toggle=True)]
    if strike:
        out.append(icon_button('format-text-strikethrough', 'Strikethrough', f'{prefix}.strikethrough', toggle=True))
    return out


def size_spin(value=11, cid=None):
    s = Gtk.SpinButton.new_with_range(1, 400, 1)
    s.set_value(value)
    s.set_width_chars(3)
    s.set_tooltip_text('Font size')
    s.set_valign(Gtk.Align.CENTER)
    return tag(s, cid) if cid else s


def format_bar_text(prefix, slim=False, full=False, compact=False):
    """The format bar when text is selected.

    full: the text mode of the iWork '09 bar, in its order: styles drawer,
    paragraph and character style; font, typeface, size, the two colours;
    B I U; alignment; line spacing and columns; list style. slim: what is used
    all the time, the rest under More.
    """
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim' if slim else 'formatbar')
    if full:
        bar.append(icon_button('desk-styles', 'Styles drawer', f'{prefix}.drawer', toggle=True))
    bar.append(menu_button(icon='desk-pilcrow' if slim else None, text='Body', tip='Paragraph style',
                           cid=f'{prefix}.paragraph-style', max_chars=9 if full else None,
                           menu=menu_model(['Title', 'Subtitle', 'Heading 1', 'Heading 2', 'Body',
                                            'Legend', 'Quote'], ['New style from selection…',
                                                                    'Redefine style from selection'])))
    if full:
        bar.append(menu_button(text='None', tip='Character style', cid=f'{prefix}.character-style',
                               max_chars=8,
                               menu=menu_model(['None', 'Emphasis', 'Strong', 'Underline'])))
    bar.append(spacer())
    bar.append(menu_button(text='TeX Gyre Termes', tip='Font', cid=f'{prefix}.font', popover=fonts_popover(),
                           max_chars=13 if full else 9 if compact else 16))
    if full:
        bar.append(dropdown(['Regular', 'Italic', 'Bold', 'Bold italic'], 0, f'{prefix}.typeface',
                            tooltip_text='Type'))
    bar.append(size_spin(11, f'{prefix}.size'))
    color = menu_button(icon='desk-text-color', tip='Text color', cid=f'{prefix}.text-color',
                        popover=colors_popover())
    if full:
        bar.append(color)
        bar.append(menu_button(icon='desk-highlight', tip='Text background color', cid=f'{prefix}.background-color',
                               popover=Gtk.Popover()))
    bar.append(spacer())
    for b in biu(prefix, strike=full):
        bar.append(b)
    if compact:   # a narrow window: the rest goes under More
        bar.append(expand())
        bar.append(menu_button(icon='view-more-horizontal', tip='More format options',
                               cid=f'{prefix}.more', menu=menu_model(['Text color', 'Alignment',
                                                                      'Line spacing', 'List style'],
                                                                     ['Show in Inspector'])))
        return bar
    if not full:
        bar.append(color)
    bar.append(spacer())
    bar.append(align_group(f'{prefix}.alignment', 'flat'))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-line-spacing', tip='Line spacing', cid=f'{prefix}.line-spacing',
                           menu=menu_model(['0.5', '1.0', '1.5', '2.0'], ['Show more…'])))
    if full:
        bar.append(menu_button(icon='desk-columns', tip='Columns', cid=f'{prefix}.columns',
                               menu=menu_model(['1', '2', '3', '4'])))
    bar.append(menu_button(icon='view-list-bullet', tip='List style', cid=f'{prefix}.list',
                           menu=menu_model(['None', 'Bookmark', 'Image', 'Numbered', 'Harvard', 'Legal'])))
    bar.append(expand())
    if full:
        bar.append(menu_button(text='Format', tip='Format menu', cid=f'{prefix}.format-menu',
                               menu=menu_model([('Font', [['Bold', 'Italic', 'Underline', 'Strikethrough', 'Outline'],
                                                           ['Larger', 'Smaller'], ['Spacing', 'Ligatures',
                                                                                'Baseline', 'Capitalization']]),
                                                ('Text', [['Align left', 'Center',
                                                            'Align right', 'Justify']])],
                                               ['Copy paragraph style', 'Copy character style',
                                                'Paste style'],
                                               [('Advanced', [['Set as master object',
                                                               'Make master objects selectable']])])))
    else:
        bar.append(menu_button(icon='view-more-horizontal', tip='More format options',
                               cid=f'{prefix}.more',
                               menu=menu_model(['Character style', 'Strikethrough', 'Text background color',
                                                'Columns'],
                                               ['Copy style', 'Paste style'],
                                               ['Show in Inspector'])))
    return bar


ARRANGE_MENU = [['Bring to front', 'Bring forward', 'Send backward',
                 'Send to back'],
                [('Align objects', [['Left', 'Center', 'Right'], ['On top', 'In the middle', 'Below']]),
                 ('Distribute objects', [['Horizontally', 'Vertically']])],
                ['Flip horizontally', 'Flip vertically'],
                ['Group', 'Ungroup'], ['Lock', 'Unlock']]


def stroke_popover():
    pop = Gtk.Popover()
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(lb, 'boxed-list')
    lb.append(combo_row('Type', ['None', 'Line', 'Dashed', 'Dotted', 'Picture frame'], 1))
    lb.append(spin_row('Width', 1, 0, 100, 0.25, digits=2, subtitle='pt'))
    lb.append(action_row('Color', suffix=swatch('#1e1e1e')))
    lb.set_size_request(260, -1)
    box = vbox(lb, margin_top=10, margin_bottom=10, margin_start=10, margin_end=10)
    pop.set_child(box)
    return pop


def opacity_popover(value=100):
    pop = Gtk.Popover()
    s = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
    s.set_value(value)
    s.set_size_request(180, -1)
    pop.set_child(hbox(s, label(f'{value}%', 'numeric'), spacing=8, margin_top=8, margin_bottom=8,
                       margin_start=12, margin_end=12))
    return pop


def format_bar_object(prefix, kind='image', slim=False, writer=False):
    """The format bar when an object is selected.

    Full (version A): the image or shape mode of the iWork '09 bar, in its
    order: stroke style, thickness and colour; fill (shapes); adjust image and
    mask (images); opacity, shadow, reflection; in Writer, inline or floating
    and the wrap. Slim (version B): one popover for the stroke, toggles for
    shadow and reflection, and the Arrange commands the iWork toolbar had
    (front, back, group), which the Desk header bar does not hold.
    """
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim' if slim else 'formatbar')
    if slim:
        bar.append(menu_button(icon='desk-styles', text='Style', tip='Object styles',
                               cid=f'{prefix}.object-style', popover=Gtk.Popover()))
        bar.append(spacer())
        if kind != 'image':
            bar.append(menu_button(icon='desk-fill', tip='Fill', cid=f'{prefix}.fill',
                                   popover=Gtk.Popover()))
        bar.append(menu_button(icon='desk-stroke', tip='Stroke', cid=f'{prefix}.stroke', popover=stroke_popover()))
        bar.append(icon_button('desk-shadow', 'Shadow', f'{prefix}.shadow', toggle=True, active=True))
        bar.append(icon_button('desk-reflection', 'Reflection', f'{prefix}.reflection', toggle=True))
        bar.append(menu_button(icon='desk-opacity', tip='Opacity', cid=f'{prefix}.opacity',
                               popover=opacity_popover()))
    else:
        bar.append(label('Stroke', 'dimmed', 'tiny', margin_start=4))
        bar.append(dropdown(['None', '————', '- - - -', '· · · ·', 'Frame'], 1, f'{prefix}.stroke-type',
                            tooltip_text='Stroke style'))
        w = Gtk.SpinButton.new_with_range(0, 100, 0.25)
        w.set_digits(0), w.set_value(1), w.set_width_chars(2), w.set_valign(Gtk.Align.CENTER)
        w.set_tooltip_text('Stroke width')
        bar.append(tag(w, f'{prefix}.stroke-width'))
        bar.append(swatch('#1e1e1e', f'{prefix}.stroke-color', tip='Stroke color'))
        if kind != 'image':
            bar.append(spacer())
            bar.append(label('Fill', 'dimmed', 'tiny'))
            bar.append(swatch('#1c71d8', f'{prefix}.fill', tip='Fill'))
    bar.append(spacer())
    if kind == 'image':
        bar.append(icon_button('desk-image-adjust', 'Adjust image', f'{prefix}.adjust-image'))
        bar.append(icon_button('desk-crop', 'Edit mask', f'{prefix}.mask'))
        if slim:
            bar.append(icon_button('desk-wand', 'Instant Alpha', f'{prefix}.alpha'))
        bar.append(spacer())
    if not slim:
        bar.append(label('Opacity', 'dimmed', 'tiny'))
        bar.append(dropdown(['100%', '75%', '50%', '25%', '0%'], 0, f'{prefix}.opacity',
                            tooltip_text='Opacity'))
        bar.append(tag(Gtk.CheckButton(label='Shadow', active=True), f'{prefix}.shadow'))
        bar.append(tag(Gtk.CheckButton(label='Reflection'), f'{prefix}.reflection'))
        bar.append(spacer())
    if writer:
        bar.append(toggle_group([('i', 'Inline', None, None), ('f', 'Floating', None, None)], 1,
                                f'{prefix}.placement', 'flat' if slim else 'round'))
        bar.append(menu_button(icon='desk-wrap', tip='Text wrap', cid=f'{prefix}.wrap',
                               menu=menu_model(['None', 'Automatic', 'Around', 'Above and below',
                                                'On the left', 'On the right'])))
        bar.append(spacer())
    if slim:
        bar.append(icon_button('desk-front', 'Bring to front', f'{prefix}.front'))
        bar.append(icon_button('desk-back', 'Send to back', f'{prefix}.back'))
        bar.append(menu_button(icon='desk-arrange', tip='Arrange', cid=f'{prefix}.arrange',
                               menu=menu_model(*ARRANGE_MENU)))
    bar.append(expand())
    if slim:
        bar.append(menu_button(icon='view-more-horizontal', tip='More format options', cid=f'{prefix}.more',
                               menu=menu_model(['Replace image…', 'Reset mask', 'Original size'],
                                               ['Copy style', 'Paste style'], ['Show in Inspector'])))
    else:
        bar.append(menu_button(text='Format', tip='Format menu', cid=f'{prefix}.format-menu',
                               menu=menu_model([('Image', [['Mask', 'Mask with shape', 'Instant Alpha'],
                                                            ['Original size']]),
                                                ('Shape', [['Make editable', 'Reset text geometry']])],
                                               ['Copy graphic style', 'Paste graphic style'])))
    return bar


# ---------------------------------------------------------- inspector pages


def style_header(name='Body', cid='insp.current-style', changed=True):
    """The current paragraph style, at the top of the text pages."""
    row = action_row(name + ('*' if changed else ''), subtitle='Paragraph style')
    if changed:
        row.add_suffix(classes(Gtk.Button(label='Update', valign=Gtk.Align.CENTER), 'flat'))
    row.add_suffix(Gtk.Image(icon_name='go-next-symbolic'))
    row.set_activatable(True)
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(lb, 'boxed-list')
    lb.append(row)
    return tag(lb, cid)


def text_style_page(prefix='insp.text'):
    font = group(
        'Font',
        action_row('Family', subtitle='TeX Gyre Termes', arrow=True),
        combo_row('Typeface', ['Regular', 'Italic', 'Bold', 'Bold italic']),
        spin_row('Size', 11, 1, 400, 1, subtitle='pt'),
        action_row('Style', suffix=hbox(*biu(prefix + '.f', strike=True), spacing=2)),
        action_row('Text color', suffix=swatch('#1e1e1e')),
        expander_row('Advanced options',
                     spin_row('Character spacing', 0, -100, 100, 1, subtitle='%'),
                     combo_row('Baseline', ['Default', 'Superscript', 'Subscript']),
                     combo_row('Capitalization', ['None', 'All caps', 'Small caps', 'Title']),
                     switch_row('Ligatures', True),
                     action_row('Text background color', suffix=swatch('transparent'))),
        cid=prefix + '.font')
    align = group('Alignment',
                  action_row('Horizontal', suffix=align_group()),
                  action_row('Indent', suffix=hbox(icon_button('format-indent-less', 'Decrease indent'),
                                                  icon_button('format-indent-more', 'Increase indent'),
                                                  spacing=2)),
                  cid=prefix + '.alignment')
    spacing = group('Spacing',
                    spin_row('Between lines', 1.2, 0.5, 4, 0.1, digits=1),
                    spin_row('Before paragraph', 0, 0, 200, 1, subtitle='pt'),
                    spin_row('After paragraph', 6, 0, 200, 1, subtitle='pt'),
                    cid=prefix + '.spacing')
    lists = group('Bullets and lists',
                  combo_row('List style', ['None', 'Bookmark', 'Image', 'Numbered', 'Harvard']),
                  cid=prefix + '.lists')
    return page(font, align, spacing, lists)


def text_layout_page(prefix='insp.layout'):
    return page(
        group('Columns', spin_row('Columns', 1, 1, 10, 1), switch_row('Equal width', True),
              cid=prefix + '.columns'),
        group('Indents', spin_row('First line', 0.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Left', 0.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Right', 0.0, 0, 20, 0.1, digits=2, subtitle='cm'), cid=prefix + '.indents'),
        group('Tabs', spin_row('Default', 1.25, 0, 20, 0.05, digits=2, subtitle='cm'),
              button_row('Add tab stop', 'list-add'), cid=prefix + '.tab-stops'),
        group('Paragraph borders and background',
              combo_row('Border', ['None', 'Line', 'Dotted', 'Dashed']),
              action_row('Background color', suffix=swatch('transparent')), cid=prefix + '.borders'))


def text_more_page(prefix='insp.more'):
    return page(
        group('Pagination and breaks', switch_row('Keep lines together'),
              switch_row('Keep with next paragraph'),
              switch_row('Paragraph starts on a new page'),
              switch_row('Prevent widow and orphan lines', True), cid=prefix + '.pagination'),
        group('Hyphenation and language', switch_row('Remove paragraph hyphenation'),
              combo_row('Language', ['Portuguese (Brazil)', 'English (US)', 'Spanish']),
              cid=prefix + '.language'),
        group('Following style', combo_row('Next paragraph', ['Same', 'Body', 'Heading 1']),
              cid=prefix + '.next'))


def style_chips(cid='insp.object-styles'):
    """Six ready-made object styles, as in the Format pane of iWork."""
    box = hbox(spacing=6, halign=Gtk.Align.CENTER)
    for col in ['#1c71d8', '#26a269', '#e66100', '#c01c28', '#813d9c', '#5e5c64']:
        b = Gtk.Button()
        b.set_child(color_box(col, 'style-chip'))
        classes(b, 'flat', 'chip-button')
        box.append(b)
    return tag(box, cid)


def object_style_page(prefix='insp.style', image=False):
    fill = None if image else group('Fill',
                 combo_row('Type', ['None', 'Color', 'Gradient', 'Advanced gradient', 'Image',
                                    'Tinted image'], 1),
                 action_row('Color', suffix=swatch('#1c71d8')), cid=prefix + '.fill')
    stroke = group('Stroke',
                   combo_row('Type', ['None', 'Line', 'Picture frame'], 1),
                   action_row('Style and color', suffix=[dropdown(['———', '- - -', '· · ·'], 0), swatch('#1e1e1e')]),
                   spin_row('Width', 1, 0, 100, 0.25, digits=2, subtitle='pt'),
                   cid=prefix + '.stroke')
    shadow = group('Shadow', expander_row_switch('Shadow',
                                                 spin_row('Offset', 4, 0, 100, 1, subtitle='pt'),
                                                 spin_row('Blur', 6, 0, 100, 1, subtitle='pt'),
                                                 spin_row('Angle', 315, 0, 360, 1, subtitle='°'),
                                                 action_row('Color', suffix=swatch('#000000'))),
                   switch_row('Reflection'), cid=prefix + '.shadow')
    opacity = group('Opacity', scale_row('Opacity', 100), cid=prefix + '.opacity')
    return page(style_chips(), *[g for g in (fill, stroke, shadow, opacity) if g is not None])


def expander_row_switch(title, *rows, enabled=True):
    r = Adw.ExpanderRow(title=title, show_enable_switch=True, enable_expansion=enabled)
    for row in rows:
        r.add_row(row)
    return r


def arrange_page(prefix='insp.arrange', writer=True):
    groups = []
    if writer:
        groups.append(group(
            'Placement',
            action_row('Object', suffix=toggle_group([('i', 'Inline', None, None),
                                                      ('f', 'Floating', None, None)], 1)),
            cid=prefix + '.placement'))
        groups.append(group(
            'Text wrap',
            combo_row('Wrap', ['None', 'Automatic', 'Around', 'Above and below', 'Left', 'Right'], 1),
            action_row('Outline', suffix=toggle_group([('r', 'Rectangle', None, None),
                                                        ('c', 'Outline', None, None)], 0)),
            spin_row('Extra space', 12, 0, 200, 1, subtitle='pt'),
            cid=prefix + '.wrap'))
    groups.append(group(
        'Size', spin_row('Width', 7.2, 0, 100, 0.1, digits=2, subtitle='cm'),
        spin_row('Height', 5.1, 0, 100, 0.1, digits=2, subtitle='cm'),
        switch_row('Constrain proportions', True), cid=prefix + '.size'))
    groups.append(group(
        'Position and rotation', spin_row('X', 9.4, 0, 100, 0.1, digits=2, subtitle='cm'),
        spin_row('Y', 12.0, 0, 100, 0.1, digits=2, subtitle='cm'),
        spin_row('Angle', 0, 0, 359, 1, subtitle='°'),
        action_row('Flip', suffix=[icon_button('object-flip-horizontal', 'Flip horizontally'),
                                       icon_button('object-flip-vertical', 'Flip vertically')]),
        cid=prefix + '.metrics'))
    groups.append(group(
        'Order and alignment',
        action_row('Order', suffix=hbox(icon_button('desk-front', 'Bring to front'),
                                        icon_button('go-up', 'Bring forward'),
                                        icon_button('go-down', 'Send backward'),
                                        icon_button('desk-back', 'Send to back'), spacing=2)),
        combo_row('Align', ['Left', 'Center', 'Right', 'On top', 'Middle', 'Below'], 0),
        combo_row('Distribute', ['Horizontally', 'Vertically'], 0),
        cid=prefix + '.order'))
    groups.append(group(
        'Group and lock',
        action_row('Group', suffix=hbox(icon_button('desk-group', 'Group'),
                                        icon_button('desk-ungroup', 'Ungroup'), spacing=2)),
        switch_row('Lock object'), cid=prefix + '.group'))
    return page(*groups)


def _mini_spin(value, digits=1):
    s = Gtk.SpinButton.new_with_range(0, 999, 0.1)
    s.set_digits(digits)
    s.set_value(value)
    s.set_width_chars(4)
    s.set_valign(Gtk.Align.CENTER)
    return s


def image_page(prefix='insp.image'):
    return page(
        group('File', action_row('landscape.jpg', subtitle='1,920 × 1,080 px · 412 kB', prop=False),
              button_row('Replace…'), cid=prefix + '.file'),
        group('Mask and transparency', button_row('Edit mask', 'desk-crop'),
              button_row('Instant Alpha', 'desk-wand'), cid=prefix + '.mask'),
        group('Adjust image', scale_row('Exposure', 50), scale_row('Saturation', 50),
              scale_row('Contrast', 50),
              expander_row('More adjustments', scale_row('Temperature', 50), scale_row('Sharpness', 0)),
              action_row('', suffix=hbox(text_button('Enhance', None, None, 'flat'),
                                         text_button('Reset', None, None, 'flat'))),
              cid=prefix + '.adjust'),
        group('Accessibility', Adw.EntryRow(title='Image description'), cid=prefix + '.description'))


def document_page(prefix='insp.doc'):
    return page(
        group('Paper', combo_row('Size', ['A4', 'Letter', 'Legal', 'A5', 'Custom…']),
              action_row('Orientation', suffix=toggle_group([('r', 'Portrait', None, None),
                                                            ('p', 'Landscape', None, None)], 0)),
              cid=prefix + '.paper'),
        group('Margins', spin_row('Top', 2.5, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Bottom', 2.5, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Left', 3.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Right', 2.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              switch_row('Facing pages'), cid=prefix + '.margins'),
        group('Header and footer', switch_row('Header', True), switch_row('Footer', True),
              cid=prefix + '.header'),
        group('Notes', combo_row('Type', ['Footnotes', 'Document endnotes',
                                          'Section endnotes']),
              combo_row('Format', ['1, 2, 3', 'i, ii, iii', 'a, b, c', '*, †, ‡']),
              cid=prefix + '.notes'),
        group('Document text', switch_row('Hyphenate', True), switch_row('Use ligatures', True),
              cid=prefix + '.text'),
        group('Security', switch_row('Require password to open'), cid=prefix + '.password'))


def section_page(prefix='insp.section'):
    return page(
        group('Page numbering', action_row('Numbering', suffix=toggle_group(
            [('c', 'Continue', None, None), ('i', 'Start at', None, None)], 0)),
            combo_row('Format', ['1, 2, 3', 'i, ii, iii', 'I, II, III', 'a, b, c']),
            cid=prefix + '.numbering'),
        group('Headers and footers', switch_row('Different first page'),
              switch_row('Different left and right pages'),
              switch_row('Use those of the previous section', True), cid=prefix + '.setup'),
        group('Section start', combo_row('Section starts on', ['Any page', 'Left page',
                                                                  'Right page']),
              cid=prefix + '.start'))


def info_page(prefix='insp.info'):
    return page(
        group('Information', Adw.EntryRow(title='Author', text='Ana Souza'),
              Adw.EntryRow(title='Title', text='Annual report 2026'),
              Adw.EntryRow(title='Keywords'), Adw.EntryRow(title='Comments'),
              cid=prefix + '.fields'),
        group('Statistics', action_row('Words', subtitle='26,171', prop=True),
              action_row('Pages', subtitle='41', prop=True),
              action_row('Paragraphs', subtitle='612', prop=True),
              action_row('Characters', subtitle='168,304', prop=True),
              combo_row('Range', ['Document', 'Selection']), cid=prefix + '.statistics'))


# --------------------------------------------------------------- bottom bar


def flat_menu(text, tip, cid, menu=None, popover=None, arrow=True):
    b = menu_button(text=text, tip=tip, cid=cid, menu=menu, popover=popover, direction=Gtk.ArrowType.UP,
                    arrow=arrow)
    if not arrow:
        b.set_child(Gtk.Label(label=text))   # a custom child: no arrow
    classes(b, 'flat', 'numeric')
    return b


def thumbs_list(kind='page', count=6, selected=2, cid='side.thumbnails', width=132):
    """The list of page or slide thumbnails of the sidebar."""
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(lb, 'navigation-sidebar')
    kinds = ['cover', 'text', 'image', 'text', 'table', 'text', 'text', 'image']
    for i in range(1, count + 1):
        if kind == 'page':
            th = canvas.page_thumb(i, selected=(i == selected), kind=kinds[(i - 1) % len(kinds)])
        else:
            th = canvas.slide_thumb(i, selected=(i == selected), w=width)
        th.set_halign(Gtk.Align.CENTER)
        num = label(str(i + (4 if kind == 'page' else 0)), 'thumb-label', 'dimmed', xalign=0.5)
        if kind == 'page':
            row = vbox(th, num, spacing=2)
        else:
            row = hbox(classes(label(str(i), 'thumb-label', 'dimmed', xalign=1.0, width_chars=2), 'numeric'),
                       th, spacing=4)
            row.get_first_child().set_valign(Gtk.Align.START)
        classes(row, 'thumb-row')
        lb.append(row)
    sw = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    sw.set_child(lb)
    return tag(sw, cid)
