# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desk Tables in the three versions."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import canvas
import parts
import shell
from kit import (Scene, action_row, button_row, classes, color_box, combo_row, dropdown, expand, group,
                 hbox, icon_button, label, menu_button, menu_model, page, spacer, spin_row, split_button,
                 switch_row, tag, text_button, toggle_group, vbox)

APP = 'tables'
SHEETS = ['Summary', 'Expenses', 'Income', 'Charts']
OBJECTS = [('Fixed expenses', 'desk-table'), ('Expenses by month', 'desk-chart'), ('Rates', 'desk-table')]
FUNCTIONS = menu_model(['Sum', 'Average', 'Minimum', 'Maximum', 'Count', 'Product'],
                       ['More functions…', 'Formula editor'])
CALCS = [('SUM', '20,849'), ('AVERAGE', '3,474.83'), ('MIN', '516'), ('MAX', '9,600'), ('COUNT', '6')]
ADD_SHEET = menu_model(['Blank sheet'], ['Duplicate sheet'])


def sheet_view(selection='range', editing=False, overlays=()):
    cv = tag(canvas.tables_canvas(selection=selection, editing=editing), 'doc.sheet')
    if overlays:
        ov = Gtk.Overlay(child=cv)
        for w in overlays:
            ov.add_overlay(w)
        return ov
    return cv


def formula_bar(editing=False):
    """The cell reference, the function button and the content of the cell."""
    ref = menu_button(text='F2', tip='Active cell', cid='formula.cell',
                      menu=menu_model(['Go to cell…'], [{'title': 'Tables'}, 'Fixed expenses', 'Rates']))
    classes(ref, 'flat', 'numeric')
    fx = menu_button(icon='desk-function', tip='Function', cid='formula.function', menu=FUNCTIONS)
    entry = Gtk.Entry(text='=SUM(B2:E2)', hexpand=True)
    classes(entry, 'formula-entry')
    kids = [ref, fx, tag(entry, 'formula.field')]
    if editing:
        kids += [icon_button('process-stop', 'Cancel', 'formula.cancel'),
                 icon_button('object-select', 'Accept', 'formula.accept')]
    bar = hbox(*kids, spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim')
    return tag(bar, 'formula.bar')


def format_bar_cell(prefix='fmt', slim=False):
    """The format bar when cells are selected.

    Full (version A): the table mode of the Numbers '09 bar, in its order:
    font, typeface, size, colour; alignment with automatic, vertical alignment,
    wrap; the number formats and the decimals; borders; fill. Slim (version B):
    what is used all the time, the rest under More.
    """
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim' if slim else 'formatbar')
    bar.append(menu_button(text='Adwaita Sans', tip='Font', cid=f'{prefix}.font', popover=Gtk.Popover(),
                           max_chars=12))
    if not slim:
        bar.append(dropdown(['Regular', 'Bold'], 1, f'{prefix}.typeface', tooltip_text='Typeface'))
    bar.append(parts.size_spin(9, f'{prefix}.size'))
    if slim:
        bar.append(spacer())
        for b in parts.biu(prefix):
            bar.append(b)
    bar.append(menu_button(icon='desk-text-color', tip='Text color', cid=f'{prefix}.text-color',
                           popover=Gtk.Popover()))
    bar.append(spacer())
    aligns = [('l', None, 'format-justify-left', 'Align left'),
              ('c', None, 'format-justify-center', 'Center'),
              ('r', None, 'format-justify-right', 'Align right'),
              ('j', None, 'format-justify-fill', 'Justify')]
    if not slim:
        aligns.append(('a', None, 'desk-align-auto', 'Align automatically'))
    bar.append(toggle_group(aligns, 2, f'{prefix}.alignment', 'flat'))
    if not slim:
        bar.append(menu_button(icon='desk-valign-middle', tip='Vertical alignment', cid=f'{prefix}.alignment-v',
                               menu=menu_model(['On top', 'In the middle', 'Below'])))
        bar.append(icon_button('view-wrapped', 'Wrap text in cell', f'{prefix}.adjust', toggle=True,
                               active=True))
    bar.append(spacer())
    more_formats = menu_model(['Date and time', 'Duration', 'Fraction', 'Numeral system', 'Scientific', 'Text'],
                              ['Slider', 'Step by step', 'Pop-up menu'], ['Custom…'])
    if slim:
        bar.append(menu_button(icon='desk-number-format', text='Number', tip='Data format',
                               cid=f'{prefix}.format',
                               menu=menu_model(['Automatic', 'Number', 'Currency', 'Percentage', 'Checkbox'],
                                               [('More formats', [['Date and time', 'Duration', 'Fraction',
                                                                    'Scientific', 'Text'],
                                                                   ['Slider', 'Step by step',
                                                                    'Pop-up menu']])],
                                               ['Custom…'])))
    else:
        for t, tip, c in (('1.0', 'Number', 'number'), ('$', 'Currency', 'currency'), ('%', 'Percentage', 'percent')):
            bar.append(text_button(t, tip, f'{prefix}.{c}', 'flat', 'numeric'))
        bar.append(icon_button('checkbox-checked', 'Checkbox', f'{prefix}.checkbox'))
        bar.append(menu_button(icon='view-more-horizontal', tip='More formats', cid=f'{prefix}.more-formats',
                               menu=more_formats))
    bar.append(icon_button('value-decrease', 'Fewer decimals', f'{prefix}.fewer-decimals'))
    bar.append(icon_button('value-increase', 'More decimals', f'{prefix}.more-decimals'))
    bar.append(spacer())
    borders = menu_button(icon='desk-borders', tip='Cell borders', cid=f'{prefix}.borders',
                          menu=menu_model(['All', 'Outside', 'Inside', 'None'],
                                          ['Style, width and color…']))
    fill = menu_button(icon='desk-fill', tip='Cell fill', cid=f'{prefix}.fill',
                       popover=Gtk.Popover())
    for w in ((borders, fill) if not slim else (fill, borders)):
        bar.append(w)
    bar.append(expand())
    if slim:
        bar.append(menu_button(icon='view-more-horizontal', tip='More format options', cid=f'{prefix}.more',
                               menu=menu_model(['Align automatically', ('Vertical alignment',
                                                                            [['On top', 'In the middle', 'Below']]),
                                                'Wrap text in cell', 'Merge cells'],
                                               ['Copy style', 'Paste style'],
                                               ['Show in Inspector'])))
    else:
        bar.append(menu_button(text='Format', tip='Format menu', cid=f'{prefix}.format-menu',
                               menu=menu_model(['Font', 'Text', 'Table', 'Chart'],
                                               ['Headers and footer', 'Show table name'],
                                               ['Copy style', 'Paste style'],
                                               ['Create custom cell format…'])))
    return tag(bar, f'{prefix}.bar')


def calc_chips(cid='calc.results', dim=False):
    """Instant calculations for the selected cells; each can be dragged to a cell."""
    box = hbox(spacing=6)
    for name, value in CALCS:
        chip = hbox(label(name, 'tiny', 'dimmed'), label(value, 'numeric', 'tiny'), spacing=5)
        classes(chip, 'calc-chip')
        box.append(chip)
    return tag(box, cid)


# ------------------------------------------------------------------ panes


def sheets_sidebar():
    """Version B: the sheets, and the tables and charts of the current one."""
    sb = Adw.Sidebar()
    s1 = Adw.SidebarSection()
    for n in SHEETS:
        s1.append(Adw.SidebarItem(title=n, icon_name='desk-sheet-symbolic'))
    s2 = Adw.SidebarSection(title='In “Expenses”')
    for n, ic in OBJECTS:
        s2.append(Adw.SidebarItem(title=n, icon_name=ic + '-symbolic'))
    sb.append(s1)
    sb.append(s2)
    sb.set_selected(1)
    return tag(sb, 'side.sheets')


def sheets_pane_a():
    """Version A: the Sheets pane of Numbers '09: a tree, the table styles and
    the results of the instant calculations."""
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(lb, 'navigation-sidebar')
    sel = None
    for n in SHEETS:
        open_ = (n == 'Expenses')
        row = hbox(Gtk.Image(icon_name='pan-down-symbolic' if open_ else 'pan-end-symbolic'),
                   Gtk.Image(icon_name='desk-sheet-symbolic'), label(n), spacing=6)
        r = Gtk.ListBoxRow(child=row)
        lb.append(r)
        if open_:
            for on, ic in OBJECTS:
                child = Gtk.ListBoxRow(child=hbox(Gtk.Image(icon_name=ic + '-symbolic'), label(on, 'tiny'),
                                                  spacing=6, margin_start=28))
                lb.append(child)
                if on == 'Fixed expenses':
                    sel = child
    lb.select_row(sel)
    tree = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True, child=lb)
    styles = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(styles, 'navigation-sidebar')
    for i, n in enumerate(['Basic', 'No grid', 'Gray', 'Gray headers', 'Ledger', 'Blue']):
        styles.append(Gtk.ListBoxRow(child=label(n, 'tiny')))
    styles.select_row(styles.get_row_at_index(5))
    calc = vbox(spacing=2, margin_start=12, margin_end=12, margin_top=6, margin_bottom=8)
    for name, value in CALCS:
        calc.append(hbox(label(name.lower(), 'tiny', 'dimmed', hexpand=True), label(value, 'tiny', 'numeric')))
    head = lambda t, extra=None: hbox(label(t, 'heading', hexpand=True), extra, margin_start=12, margin_end=6,  # noqa: E731
                                      margin_top=8, margin_bottom=2)
    pane = vbox(
        tag(vbox(head('Sheets', icon_button('list-add', 'New sheet')), tree), 'side.a.sheets'),
        Gtk.Separator(),
        tag(vbox(head('Styles'), styles), 'side.a.styles'),
        Gtk.Separator(),
        tag(calc, 'side.a.calculations'))
    pane.set_size_request(168, -1)
    return pane


def table_page(prefix='insp.table'):
    chips = hbox(spacing=6, halign=Gtk.Align.CENTER)
    for col in ['#3d5373', '#1c71d8', '#26a269', '#e66100', '#813d9c', '#5e5c64']:
        b = Gtk.Button(child=color_box(f'linear-gradient(180deg, {col} 0 30%, white 30% 53%, '
                                       f'color-mix(in srgb, {col} 14%, white) 53% 76%, white 76%)', 'style-chip'))
        classes(b, 'flat', 'chip-button')
        chips.append(b)
    return page(
        tag(chips, prefix + '.styles'),
        group('Headers and footer', spin_row('Header rows', 1, 0, 5), spin_row('Header columns', 1, 0, 5),
              spin_row('Footer rows', 1, 0, 5), switch_row('Freeze headers', True),
              cid=prefix + '.headers'),
        group('Rows and columns', spin_row('Rows', 8, 1, 9999), spin_row('Columns', 6, 1, 999),
              spin_row('Row height', 0.64, 0, 50, 0.01, digits=2, subtitle='cm'),
              spin_row('Column width', 1.9, 0, 50, 0.01, digits=2, subtitle='cm'),
              button_row('Fit to content'), cid=prefix + '.size'),
        group('Grid and background', switch_row('Gridlines', True), switch_row('Alternating row color', True),
              cid=prefix + '.grid'),
        group('Name', switch_row('Show table name', True), cid=prefix + '.name'))


def cell_page(prefix='insp.cell'):
    return page(
        group('Data format',
              combo_row('Format', ['Automatic', 'Number', 'Currency', 'Percentage', 'Date and time', 'Duration',
                                    'Fraction', 'Scientific', 'Text', 'Checkbox', 'Slider',
                                    'Custom…'], 1),
              spin_row('Decimals', 0, 0, 30), switch_row('Thousands separator', True),
              combo_row('Negatives', ['-100', '(100)', '-100 in red']), cid=prefix + '.format'),
        group('Fill and border', action_row('Fill', suffix=parts.swatch('#ffffff')),
              action_row('Border', suffix=[dropdown(['———', '- - -', '· · ·'], 0), parts.swatch('#9a9996')]),
              cid=prefix + '.border'),
        group('Text in cell', switch_row('Wrap text in cell', True), cid=prefix + '.wrap'),
        group('Conditional highlighting', button_row('Show rules…'), cid=prefix + '.conditional'))


def format_bar_chart(prefix='fmt'):
    """Version B, a chart selected: its type, the data and the look."""
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim')
    bar.append(menu_button(icon='desk-chart', text='Columns', tip='Chart type', cid=f'{prefix}.chart-type',
                           popover=parts.gallery_popover('chart')))
    bar.append(tag(classes(Gtk.Button(label='Edit data', tooltip_text='Edit data references'), 'flat'),
                   f'{prefix}.edit-data'))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-fill', tip='Chart colors', cid=f'{prefix}.fill',
                           popover=Gtk.Popover()))
    bar.append(menu_button(icon='desk-stroke', tip='Stroke', cid=f'{prefix}.stroke', popover=parts.stroke_popover()))
    bar.append(icon_button('desk-shadow', 'Shadow', f'{prefix}.shadow', toggle=True))
    bar.append(spacer())
    bar.append(icon_button('desk-front', 'Bring to front', f'{prefix}.front'))
    bar.append(icon_button('desk-back', 'Send to back', f'{prefix}.back'))
    bar.append(menu_button(icon='desk-arrange', tip='Arrange', cid=f'{prefix}.arrange',
                           menu=menu_model(*parts.ARRANGE_MENU)))
    bar.append(expand())
    bar.append(menu_button(icon='view-more-horizontal', tip='More format options', cid=f'{prefix}.more',
                           menu=menu_model(['Legend', 'Title', 'Error bars', 'Trendline'],
                                           ['Copy style', 'Paste style'], ['Show in Inspector'])))
    return tag(bar, f'{prefix}.bar')


def chart_pages():
    chart = page(
        parts.style_chips('insp.chart.styles'),
        group('Chart', combo_row('Type', ['Columns', 'Stacked columns', 'Bars', 'Rows', 'Area', 'Pie',
                                            'Scatter', 'Mixed', 'Two axes'], 0),
              switch_row('Title', True), switch_row('Legend'), switch_row('Hidden table text'),
              cid='insp.chart.options'),
        group('Columns', spin_row('Gap between columns', 40, 0, 500, 5, subtitle='%'),
              spin_row('Gap between groups', 100, 0, 500, 5, subtitle='%'), cid='insp.chart.gaps'),
        group('3D scene', switch_row('3D chart'), cid='insp.chart.3d'))
    axis = page(
        tag(toggle_group([('y', 'Value (Y)', None, None), ('x', 'Category (X)', None, None)], 0, None,
                         homogeneous=True), 'insp.axis.which'),
        group('Scale', combo_row('Scale', ['Linear', 'Logarithmic']),
              Adw.EntryRow(title='Minimum', text='Automatic'), Adw.EntryRow(title='Maximum', text='Automatic'),
              spin_row('Steps', 4, 1, 50), cid='insp.axis.scale'),
        group('Labels and lines', switch_row('Value labels', True), switch_row('Gridlines', True),
              switch_row('Tick marks'), combo_row('Format', ['Same as data', 'Number', 'Currency',
                                                                     'Percentage']), cid='insp.axis.labels'))
    series = page(
        group('Data', Adw.EntryRow(title='Values', text='Fixed expenses :: B8:E8'),
              Adw.EntryRow(title='Labels', text='Fixed expenses :: B1:E1'),
              combo_row('Value labels', ['None', 'On top', 'In the middle', 'Below']), cid='insp.series.data'),
        group('Trendline', combo_row('Type', ['None', 'Linear', 'Logarithmic', 'Polynomial',
                                                        'Power', 'Exponential', 'Moving average']),
              cid='insp.series.trendline'),
        group('Error bars', combo_row('Type', ['None', 'Positive and negative', 'Positive only',
                                                    'Negative only']), cid='insp.series.error'))
    return [('chart', 'Chart', None, chart), ('axis', 'Axis', None, axis), ('series', 'Series', None, series),
            ('arrange', 'Arrange', None, parts.arrange_page(writer=False))], 'chart'


def format_pages():
    return [('table', 'Table', None, table_page()), ('cell', 'Cell', None, cell_page()),
            ('text', 'Text', None, parts.text_style_page('insp.text')),
            ('arrange', 'Arrange', None, parts.arrange_page(writer=False))], 'table'


def reorganize_pages():
    sort = page(
        group('Sort', combo_row('Sort', ['The whole table', 'The selected rows']),
              action_row('Total', suffix=toggle_group([('a', 'Ascending', None, None),
                                                       ('d', 'Descending', None, None)], 1)),
              button_row('Add column', 'list-add'), cid='insp.reorg.sort'),
        classes(Gtk.Button(label='Sort now', halign=Gtk.Align.CENTER), 'pill'))
    filt = page(group('Filter', switch_row('Show rows that match', True),
                      combo_row('Match', ['All rules', 'Any rule']),
                      action_row('Total', subtitle='is greater than 1,000'),
                      button_row('Add rule', 'list-add'), cid='insp.reorg.filter'))
    cat = page(group('Categories', combo_row('Categorize by', ['None', 'Column A', 'Total']),
                     cid='insp.reorg.categories'))
    return [('sort', 'Order', None, sort), ('filter', 'Filter', None, filt),
            ('categories', 'Categories', None, cat)], 'sort'


def document_pages():
    sheet = page(
        group('Sheet', Adw.EntryRow(title='Name', text='Expenses'), cid='insp.sheet.name'),
        group('Printing', action_row('Orientation', suffix=toggle_group([('r', 'Portrait', None, None),
                                                                         ('p', 'Landscape', None, None)], 1)),
              combo_row('Paper', ['A4', 'Letter', 'Legal']),
              spin_row('Content scale', 100, 10, 400, 5, subtitle='%'),
              spin_row('Numbering starts at', 1, 1, 9999), cid='insp.sheet.printing'),
        group('Margins', spin_row('Top', 1.5, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Bottom', 1.5, 0, 20, 0.1, digits=2, subtitle='cm'), cid='insp.sheet.margins'))
    return [('sheet', 'Sheet', None, sheet), ('info', 'Info', None, parts.info_page())], 'sheet'


LEVELS = [shell.FORMAT, shell.DATA, shell.DOCUMENT]


def inspector_b(level=shell.FORMAT, context='table'):
    if level == shell.DATA:
        pages, vis = reorganize_pages()
        return shell.inspector_b(LEVELS, level, 'Fixed expenses', None, pages, vis)
    if level == 'Document':
        pages, vis = document_pages()
        return shell.inspector_b(LEVELS, level, None, None, pages, vis)
    if context == 'chart':
        pages, vis = chart_pages()
        return shell.inspector_b(LEVELS, level, 'Chart', None, pages, vis)
    pages, vis = format_pages()
    return shell.inspector_b(LEVELS, level, 'Table', None, pages, vis)


A_ICONS = [('doc', 'x-office-document', 'Document'), ('sheet', 'desk-sheet', 'Sheet'),
           ('table', 'desk-table', 'Table'), ('format', 'desk-number-format', 'Format'),
           ('chart', 'desk-chart', 'Chart (data)'), ('text', 'desk-character-style', 'Text'),
           ('graphic', 'desk-fill', 'Graphic'), ('metrics', 'desk-ruler', 'Metrics'),
           ('link', 'desk-link', 'Link')]


def inspector_a():
    cg, cr = shell.compact_group, shell.compact_row
    groups = [
        cg('Name', shell.plain_row(Gtk.Entry(text='Fixed expenses', hexpand=True)), cid='insp.a.name'),
        cg('Headers and footer', cr('Header rows', 1, '', 0), cr('Header columns', 1, '', 0),
           cr('Footer rows', 1, '', 0), cid='insp.a.headers'),
        cg('Rows and columns', cr('Row height', 0.64, 'cm', 2), cr('Column width', 1.9, 'cm', 2),
           shell.switch_line('Fit to content', False), cid='insp.a.size'),
        cg('Cell borders', shell.plain_row(hbox(dropdown(['———', '- - -'], 0), parts.swatch('#9a9996'),
                                                   parts._mini_spin(0.5, 2), spacing=8)), cid='insp.a.borders'),
        cg('Background', shell.plain_row(hbox(label('Cell background', hexpand=True), parts.swatch('#ffffff'))),
           shell.switch_line('Alternating row color', True), cid='insp.a.back'),
    ]
    return shell.inspector_a(A_ICONS, 2, 'Table', None, groups)


# ---------------------------------------------------------------- versions


def version_a(win):
    start = [text_button('Open', 'Open a document', 'head.open'),
             menu_button(text='View', tip='View menu', cid='head.view-menu',
                         menu=parts.view_menu(APP)),
             menu_button(text='Insert', tip='Insert menu', cid='head.insert-menu',
                         menu=parts.insert_menu(APP, objects=True)),
             menu_button(icon='desk-function', text='Function', tip='Insert function', cid='head.function',
                         menu=FUNCTIONS)]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('image-x-generic', 'Media', 'head.media', toggle=True),
           icon_button('desk-sort', 'Organize', 'head.reorganize', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    inner = shell.split(inspector_a(), sheet_view(), shell.INSPECTOR_A_W, Gtk.PackType.END)
    body = shell.split(sheets_pane_a(), inner, 168)
    bottom = shell.bottom_bar(parts.flat_menu('100%', 'Zoom', 'bottom.zoom', menu=parts.zoom_menu()), expand(),
                              tag(label('Sheet 2 of 4', 'dimmed', 'tiny', 'numeric', margin_end=8),
                                  'bottom.count'))
    win.set_content(shell.toolbar_view(body, top=[hb, format_bar_cell('fmt'), formula_bar()], bottom=[bottom]))


def version_b(win, level=shell.FORMAT, selection='range', editing=False):
    side = shell.nav_sidebar(APP, sheets_sidebar(), ['Sheets', 'Table styles', 'Comments',
                                                     'Search results'], 'Add sheet', ADD_SHEET)
    start = [split_button(icon='sidebar-show', tip='Sidebar', drop_tip='View',
                          cid='head.view', menu=parts.view_menu(APP)),
             spacer(),
             parts.insert_menu_button(APP)]
    start += parts.insert_buttons()
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('edit-find', 'Find', 'head.find', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    context = 'chart' if selection == 'chart' else 'table'
    body = shell.split(inspector_b(level, context), sheet_view(selection, editing), shell.INSPECTOR_W,
                       Gtk.PackType.END)
    bottom = shell.bottom_bar(
        parts.flat_menu('100%', 'Zoom', 'bottom.zoom', menu=parts.zoom_menu()),
        expand(), calc_chips(), expand(),
        parts.flat_menu('6 cells', 'Selection', 'bottom.selection', arrow=False,
                        menu=menu_model(['Sum', 'Average', 'Minimum', 'Maximum', 'Count', 'Product'],
                                        ['Copy result'])))
    fbar = format_bar_chart('fmt') if context == 'chart' else format_bar_cell('fmt', slim=True)
    content = shell.toolbar_view(body, top=[hb, fbar, formula_bar(editing)], bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def version_c(win, level=shell.FORMAT):
    start = [menu_button(icon='desk-insert', tip='Insert', cid='head.insert-all', popover=Gtk.Popover())]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu',
                       menu=parts.main_menu(APP, lean=True)),
           spacer(),
           icon_button('x-office-document', 'Document', 'head.document', toggle=True),
           icon_button('desk-sort', 'Organize', 'head.reorganize', toggle=True),
           icon_button('desk-format', 'Format', 'head.format', toggle=True, active=True),
           spacer(),
           icon_button('edit-find', 'Find', 'head.find', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    view = Adw.TabView()
    zoom = shell.osd_pill(icon_button('zoom-out', 'Zoom out'), label('100%', 'numeric', margin_start=4, margin_end=4),
                          icon_button('zoom-in', 'Zoom in'), cid='osd.zoom',
                          halign=Gtk.Align.END, valign=Gtk.Align.END, margin_end=14, margin_bottom=12)
    chosen = None
    for n in SHEETS:
        child = sheet_view(overlays=(zoom,)) if n == 'Expenses' else Gtk.Box()
        pg = view.append(child)
        pg.set_title(n)
        if n == 'Expenses':
            chosen = pg
    view.set_selected_page(chosen)
    tabs = Adw.TabBar(view=view, autohide=False, expand_tabs=False)
    tabs.set_end_action_widget(icon_button('list-add', 'Add sheet', 'tabs.add'))
    tag(tabs, 'tabs.sheets')
    pages, vis = format_pages()
    body = shell.split(shell.inspector_c(None, pages, vis), view, shell.INSPECTOR_W, Gtk.PackType.END)
    cellbar = hbox(menu_button(icon='desk-function', tip='Function', cid='formula.function', menu=FUNCTIONS),
                   tag(label('F2', 'dimmed', 'numeric'), 'formula.cell'),
                   tag(classes(label('=SUM(B2:E2)', 'formula-entry', hexpand=True), 'numeric'), 'formula.field'),
                   calc_chips(), spacing=8)
    classes(cellbar, 'toolbar', 'bottombar')
    tag(cellbar, 'bottom.cell')
    win.set_content(shell.toolbar_view(body, top=[hb, tabs], bottom=[cellbar]))


SCENES = [
    Scene('tables-a', 'Desk Tables · A Classic', lambda w: version_a(w), APP, 'A'),
    Scene('tables-b', 'Desk Tables · B Equilibrada', lambda w: version_b(w), APP, 'B'),
    Scene('tables-c', 'Desk Tables · C Enxuta', lambda w: version_c(w), APP, 'C'),
]
