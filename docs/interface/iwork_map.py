#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Every command and panel of iWork '09, with its place in the window of Desk (M+).

Reads the inventory taken on the reference virtual machine (Pages 4.3,
Numbers 2.3, Keynote 5.3) and writes one row per menu command, toolbar item,
format bar mode, Inspector tab, preference pane and panel, with the place it
has in window M+ (decision D-008) and the component of the mockups that
holds it.

    python3 iwork_map.py INVENTORY_DIR iwork09-to-desk.tsv mockups/catalog.txt

The inventory stays outside the repository, because it was read from Apple
software (D-011). It is written in Portuguese, so the patterns that read its
headings and notes are in Portuguese too.
"""
import csv
import re
import sys

SRC, OUT = sys.argv[1], sys.argv[2]
CATALOG = sys.argv[3] if len(sys.argv) > 3 else ''
APPS = (('Pages', 'pages.md', 'Writer'), ('Numbers', 'numbers.md', 'Tables'), ('Keynote', 'keynote.md', 'Slides'))
COLLAPSE = {'Chart', 'Chart Type', 'Shape', 'Mask with Shape', 'Open Recent', 'Header Rows', 'Header Columns',
            'Footer Rows', 'Format X-Axis', 'Format Y-Axis', 'Merge Field', 'Sender Field', 'Bibliography Format',
            'Smart Build', 'Table'}
HEADERS = ('Send via Mail', 'Send To', 'Export')
ADAPTED, SAME, SYSTEM, OUT_OF_SCOPE = 'adapted', 'same', 'system', 'out'

# Places that many rules share.
FORMAT = 'Inspector ▸ Format'
SEARCH = 'Search Commands (Ctrl+/)'
CONTEXT_TABLE = 'Context menu of the table and ' + SEARCH
VIEW = 'Main menu ▸ View'

# (regex over "Menu ▸ Submenu ▸ Item", place in Desk, component of the mockups, how)
MENU_RULES = [
    (r'^(Pages|Numbers|Keynote) ▸ About', 'Main menu ▸ About', 'head.menu', SAME),
    (r'^(Pages|Numbers|Keynote) ▸ Preferences', 'Main menu ▸ Preferences', 'dialog.preferences', SAME),
    (r'^(Pages|Numbers|Keynote) ▸ (Try|Provide|Register)', 'Does not apply to Desk', '', OUT_OF_SCOPE),
    (r'^(Pages|Numbers|Keynote) ▸ Quit', 'Ctrl+Q, with no menu item', '', ADAPTED),
    (r'^(Pages|Numbers|Keynote) ▸', 'The system: GNOME takes care of windows and apps', '', SYSTEM),
    (r'^File ▸ New from (Template|Theme) Chooser', 'Template chooser inside the window', 'templates.window', ADAPTED),
    (r'^File ▸ (New|Open…|Open Recent( \(.*\))?|Save As…|Print…)$', 'Main menu', 'head.menu', SAME),
    (r'^File ▸ Save$', 'Automatic saving and versions (D-001, DOC-11)',
     'head.menu', ADAPTED),
    (r'^(File|Share) ▸ Export ▸ iPod', 'Out of scope: an Apple device; the movie comes out of Export ▸ Movie', '',
     OUT_OF_SCOPE),
    (r'^File ▸ Export', 'Main menu ▸ Export', 'head.menu', SAME),
    (r'^File ▸ Close All', 'Ctrl+Alt+W and ' + SEARCH, 'commands.dialog', ADAPTED),
    (r'^File ▸ Close', 'Close button of the document tab, and Ctrl+W', 'tabs.documents', ADAPTED),
    (r'^File ▸ Revert', 'Main menu: browse and restore the versions (D-001, DOC-11)', 'head.menu', ADAPTED),
    (r'^File ▸ Reduce File Size', FORMAT + ' ▸ Image and ' + SEARCH, 'insp.image.file', ADAPTED),
    (r'^File ▸ Page Setup', 'Inspector ▸ Document (paper and margins) and the Print dialog', 'insp.doc.paper', ADAPTED),
    (r'^File ▸ Show Print View', VIEW, 'head.menu', ADAPTED),
    (r'^File ▸ (Record Slideshow|Clear Recording)', 'Menu of the Play button and Inspector ▸ Document ▸ Audio',
     'insp.audio.recording', ADAPTED),
    (r'^File ▸ Choose Theme', 'Inspector ▸ Document ▸ Theme', 'insp.show.theme', ADAPTED),
    (r'^File ▸ (Save as Template|Save Theme)', SEARCH + ' (Save as template)', 'commands.dialog', ADAPTED),
    (r'^Edit ▸ Find ▸ Search', 'Sidebar ▸ Search results', 'side.mode', ADAPTED),
    (r'^Edit ▸ Find', 'Find bar, from the Find button', 'find.bar', ADAPTED),
    (r'^Edit ▸ (Spelling|Proofreading|Writing Tools)', 'Context menu of the text and ' + SEARCH, 'commands.dialog',
     ADAPTED),
    (r'^Edit ▸ Track Changes', SEARCH + '; the bar of tracked changes shows under the header bar', 'commands.dialog',
     ADAPTED),
    (r'^Edit ▸ Mail Merge', SEARCH + '; it opens a dialog', 'commands.dialog', ADAPTED),
    (r'^Edit ▸ EndNote', SEARCH + ', in the phase of citations', 'commands.dialog', ADAPTED),
    (r'^Edit ▸ Delete Page', 'Context menu of the thumbnails and the Delete key', 'side.thumbnails', ADAPTED),
    (r'^Edit ▸ (Delete Row|Delete Column|Paste Values|Mark for Move|Move)', CONTEXT_TABLE, 'doc.sheet', ADAPTED),
    (r'^Edit ▸', 'Shortcut, context menu and ' + SEARCH, 'commands.dialog', ADAPTED),
    (r'^Insert ▸ Text Box$', 'Text button of the header bar', 'head.insert.text', SAME),
    (r'^Insert ▸ Shape( \(.*\))?$', 'Shape button of the header bar', 'head.insert.shape', SAME),
    (r'^Insert ▸ Table( \(.*\))?$', 'Table button of the header bar', 'head.insert.table', SAME),
    (r'^Insert ▸ Chart( \(.*\))?$', 'Chart button of the header bar', 'head.insert.chart', SAME),
    (r'^Insert ▸ Comment$', 'Comment button of the header bar', 'head.insert.comment', SAME),
    (r'^Insert ▸ Choose', 'Insert menu ▸ Choose, and the Media button', 'head.insert.media', SAME),
    (r'^Insert ▸ Sheet$', 'The + button of the bottom bar, before the sheet switcher', 'bottom.add', ADAPTED),
    (r'^Numbers\|Insert ▸ Function', 'Function button of the formula bar', 'formula.function', ADAPTED),
    (r'^Insert ▸ Function', 'Context menu of the cell and ' + SEARCH + ', with a table cell selected',
     'commands.dialog', ADAPTED),
    (r'^Insert ▸ (Copied Rows|Copied Columns)', CONTEXT_TABLE, 'doc.sheet', ADAPTED),
    (r'^Insert ▸ (Text )?Hyperlink', 'Insert menu ▸ Link, which opens a popover', 'head.insert', ADAPTED),
    (r'^Insert ▸ (Merge Field|Sender Field)', 'Insert menu ▸ Field', 'head.insert', ADAPTED),
    (r'^Insert ▸ (EndNote|MathType)', 'Insert menu, in the phase of citations and equations', 'head.insert', ADAPTED),
    (r'^Insert ▸ Fill', 'Context menu of the cell and ' + SEARCH, 'commands.dialog', ADAPTED),
    (r'^Insert ▸', 'Insert menu', 'head.insert', SAME),
    (r'^Slide ▸ New Slide', 'The + button at the top of the sidebar', 'side.add', ADAPTED),
    (r'^Slide ▸ Skip Slide', 'Context menu of the navigator and ' + FORMAT + ' ▸ Slide', 'side.navigator', ADAPTED),
    (r'^Slide ▸ Show/Hide Slide Numbers', FORMAT + ' ▸ Slide ▸ Appearance', 'insp.slide.appearance', ADAPTED),
    (r'^Slide ▸ Go To', 'Navigator in the sidebar and the page keys', 'side.navigator', ADAPTED),
    (r'^Slide ▸', 'Context menu of the navigator and ' + SEARCH, 'side.navigator', ADAPTED),
    (r'^Table ▸ Show Reorganize Panel', 'Inspector ▸ Data', 'insp.level1', ADAPTED),
    (r'^Table ▸ (Add|Delete) (Row|Column)', 'Insert menu ▸ Rows and columns, context menu of the table and Alt+arrows',
     'head.insert', ADAPTED),
    (r'^Table ▸ (Header|Footer|Freeze)', FORMAT + ' ▸ Table ▸ Headers and footer', 'insp.table.headers', ADAPTED),
    (r'^Table ▸ (Resize|Distribute|Unhide)', FORMAT + ' ▸ Table ▸ Rows and columns, and context menu of the table',
     'insp.table.size', ADAPTED),
    (r'^Table ▸ Enable All Categories', 'Inspector ▸ Data ▸ Categories', 'insp.reorg.categories', ADAPTED),
    (r'^Table ▸', CONTEXT_TABLE, 'doc.sheet', ADAPTED),
    (r'^Format ▸ Font ▸ Show Fonts', FORMAT + ' ▸ Style ▸ Font, whose family row opens the Fonts popover',
     'insp.text.font', ADAPTED),
    (r'^Format ▸ Font ▸ (Bold|Italic|Underline)', FORMAT + ' ▸ Style ▸ Font, and the usual shortcuts',
     'insp.text.f.bold', ADAPTED),
    (r'^Format ▸ Font ▸ (Bigger|Smaller)', 'Size field in ' + FORMAT + ' ▸ Style ▸ Font; Ctrl++ and Ctrl+− are the zoom',
     'insp.text.font', ADAPTED),
    (r'^Format ▸ Font', FORMAT + ' ▸ Style ▸ Font', 'insp.text.font', ADAPTED),
    (r'^Format ▸ Text ▸ (Align|Center|Justify|Auto Align)', FORMAT + ' ▸ Style ▸ Alignment, and shortcuts',
     'insp.text.alignment', ADAPTED),
    (r'^Format ▸ Text ▸ (Decrease|Increase)', 'Shortcuts and ' + FORMAT + ' ▸ Style ▸ Bullets and lists',
     'insp.text.lists', ADAPTED),
    (r'^Format ▸ Text ▸ (Copy|Paste) Ruler', 'Context menu of the text and ' + SEARCH, 'commands.dialog', ADAPTED),
    (r'^Format ▸ Table', FORMAT + ' ▸ Table and context menu of the table', 'insp.table.headers', ADAPTED),
    (r'^Format ▸ Chart ▸ Chart Type', FORMAT + ' ▸ Chart', 'insp.chart.options', ADAPTED),
    (r'^Format ▸ Chart ▸ Show Data Editor', FORMAT + ' ▸ Chart ▸ Edit data, and the context menu of the chart',
     'insp.chart.options', ADAPTED),
    (r'^Format ▸ Chart', FORMAT + ' ▸ Chart, Axis and Series', 'insp.axis.labels', ADAPTED),
    (r'^Format ▸ (Shape|Text Box|Image)', 'Context menu of the object and ' + SEARCH, 'commands.dialog', ADAPTED),
    (r'^Format ▸ (Copy|Paste) (Character |Paragraph )?(Style|Animation)', 'Shortcuts, context menu and ' + SEARCH,
     'commands.dialog', ADAPTED),
    (r'^Format ▸ (Create New Paragraph Style|Import Styles)', 'Sidebar ▸ Styles, and the style row of the Inspector',
     'insp.current-style', ADAPTED),
    (r'^Format ▸ Create New Table Style', FORMAT + ' ▸ Table ▸ Table styles', 'insp.table.styles', ADAPTED),
    (r'^Format ▸ Reapply Master', 'Context menu of the navigator and ' + FORMAT + ' ▸ Slide', 'side.navigator', ADAPTED),
    (r'^Format ▸ (Show Conditional|Create Custom Cell|Manage Custom Cell)', FORMAT + ' ▸ Cell', 'insp.cell.conditional',
     ADAPTED),
    (r'^Format ▸ (Mask|Instant Alpha|Remove Instant Alpha)', FORMAT + ' ▸ Image', 'insp.image.mask', ADAPTED),
    (r'^Format ▸ (Reapply)', 'Context menu of the object and ' + SEARCH, 'commands.dialog', ADAPTED),
    (r'^Format ▸ Advanced', SEARCH + ' (authoring of templates and themes)', 'commands.dialog', ADAPTED),
    (r'^Format ▸', FORMAT + ', context menu and ' + SEARCH, 'insp.pane', ADAPTED),
    (r'^Arrange ▸ (Bring to Front|Send to Back)', FORMAT + ' ▸ Arrange, and Ctrl+Shift+] and Ctrl+Shift+[',
     'insp.arrange.order', ADAPTED),
    (r'^Arrange ▸', FORMAT + ' ▸ Arrange and the context menu of the object', 'insp.arrange.order', ADAPTED),
    (r'^View ▸ Slide Only', 'Sidebar button, which hides it (F9)', 'head.sidebar', ADAPTED),
    (r'^View ▸ Light Table', VIEW + ' ▸ Light table, in the document area', 'doc.light-table', ADAPTED),
    (r'^View ▸ (Page Thumbnails|Navigator|Outline|Search|Show Styles Drawer|Show Master Slides|Show Formula List)',
     'Sidebar, from its content switcher', 'side.mode', ADAPTED),
    (r'^View ▸ Zoom', 'Zoom control over the document', 'osd.zoom', ADAPTED),
    (r'^View ▸ Show Presenter Notes', 'Notes under the slide, and ' + VIEW + ' ▸ Presenter notes', 'doc.notes',
     ADAPTED),
    (r'^View ▸ Show Inspector', 'Inspector button of the header bar', 'head.inspector', SAME),
    (r'^View ▸ New Inspector', 'Does not apply: there is one Inspector', '', OUT_OF_SCOPE),
    (r'^View ▸ Show Colors', 'Colors popover, from the color rows of ' + FORMAT, 'colors.popover', ADAPTED),
    (r'^View ▸ Show Adjust Image', FORMAT + ' ▸ Image ▸ Adjust image', 'insp.image.adjust', ADAPTED),
    (r'^View ▸ Show Media Browser', 'Media button of the header bar, which opens a popover', 'head.insert.media',
     ADAPTED),
    (r'^View ▸ Enter Full Screen', VIEW + ' ▸ Fullscreen (F11)', 'head.menu', ADAPTED),
    (r'^View ▸ Show Document Outline', 'Sidebar ▸ Outline', 'side.mode', ADAPTED),
    (r'^View ▸ Show Document Warnings', 'Banner of warnings over the document', 'notice.banner', ADAPTED),
    (r'^View ▸ Show Function Browser', 'Function button of the formula bar', 'formula.function', ADAPTED),
    (r'^View ▸ (Show Toolbar|Hide Toolbar|Customize Toolbar)', 'Does not apply: the header bar is not customizable',
     '', OUT_OF_SCOPE),
    (r'^View ▸ (Show Format Bar|Hide Format Bar)', 'Does not apply: M+ has no format bar; its controls are in '
     + FORMAT, '', OUT_OF_SCOPE),
    (r'^View ▸', VIEW, 'head.menu', ADAPTED),
    (r'^Play ▸ Customize Presenter Display', 'Preferences ▸ Slideshow', 'dialog.preferences', ADAPTED),
    (r'^Play ▸', 'Play button of the header bar and its menu', 'head.play', ADAPTED),
    (r'^Window ▸', 'The system: GNOME takes care of the windows; the documents of an app are tabs of one window',
     'tabs.documents', SYSTEM),
    (r'^Share ▸ (Share via iWork|Show Shared|No unread|Sign In)', 'Out of scope: iWork.com was shut down', '',
     OUT_OF_SCOPE),
    (r'^Share ▸ Send To', 'Out of scope: Apple services and programs', '', OUT_OF_SCOPE),
    (r'^Share ▸ Send via Mail', 'Main menu ▸ Export ▸ Send by email', 'head.menu', ADAPTED),
    (r'^Share ▸ Export', 'Main menu ▸ Export', 'head.menu', ADAPTED),
    (r'^Share ▸', 'Out of scope: iWork.com was shut down', '', OUT_OF_SCOPE),
    (r'^Help ▸ Keyboard Shortcuts', 'Main menu ▸ Keyboard Shortcuts', 'dialog.shortcuts', ADAPTED),
    (r'^Help ▸ (Welcome|Video Tutorials|iWork Resources|Service and Support)', 'Out of scope: Apple material', '',
     OUT_OF_SCOPE),
    (r'^Help ▸', 'Main menu ▸ Help', 'head.menu', SAME),
]

TOOLBAR = [
    (r'^(View)$', 'Sidebar button, and ' + VIEW, 'head.sidebar', ADAPTED),
    (r'^Full Screen$', VIEW + ' ▸ Fullscreen (F11)', 'head.menu', ADAPTED),
    (r'^Outline$', 'Sidebar ▸ Outline', 'side.mode', ADAPTED),
    (r'^(Pages|New)$', 'The + button at the top of the sidebar', 'side.add', ADAPTED),
    (r'^Sheet$', 'The + button of the bottom bar, before the sheet switcher', 'bottom.add', ADAPTED),
    (r'^(Columns|Style|Character|List|Increase|Decrease|Bigger|Smaller|Subscript|Superscript)$',
     FORMAT + ' ▸ Style and Layout', 'insp.tabs', ADAPTED),
    (r'^Guides$', VIEW, 'head.menu', ADAPTED),
    (r'^Share$', 'Out of scope: iWork.com was shut down', '', OUT_OF_SCOPE),
    (r'^Send To$', 'Out of scope: Apple services and programs', '', OUT_OF_SCOPE),
    (r'^Track Changes$', SEARCH, 'commands.dialog', ADAPTED),
    (r'^Wrap$', FORMAT + ' ▸ Arrange ▸ Text wrap', 'insp.arrange.wrap', ADAPTED),
    (r'^(Copy Style|Paste Style)$', 'Shortcuts, context menu and ' + SEARCH, 'commands.dialog', ADAPTED),
    (r'^(Lock|Unlock|Group|Ungroup|Flip Horizontal(ly)?|Flip Vertical(ly)?|Connect)$',
     FORMAT + ' ▸ Arrange and the context menu of the object', 'insp.arrange.group', ADAPTED),
    (r'^(Front|Back|Forward|Backward)$', FORMAT + ' ▸ Arrange', 'insp.arrange.order', ADAPTED),
    (r'^Text Box$', 'Text button of the header bar', 'head.insert.text', SAME),
    (r'^Shapes$', 'Shape button of the header bar', 'head.insert.shape', SAME),
    (r'^Tables?$', 'Table button of the header bar', 'head.insert.table', SAME),
    (r'^Charts$', 'Chart button of the header bar', 'head.insert.chart', SAME),
    (r'^Comment$', 'Comment button of the header bar', 'head.insert.comment', SAME),
    (r'^Objects$', 'The object buttons of the header bar; below 720 px, the Insert menu', 'head.insert', ADAPTED),
    (r'^Inspector$', 'Inspector button of the header bar', 'head.inspector', SAME),
    (r'^Media$', 'Media button of the header bar', 'head.insert.media', SAME),
    (r'^Adjust$', FORMAT + ' ▸ Image ▸ Adjust image', 'insp.image.adjust', ADAPTED),
    (r'^Colors$', 'Colors popover, from the color rows of ' + FORMAT, 'colors.popover', ADAPTED),
    (r'^Fonts$', 'Fonts popover, from the family row of ' + FORMAT + ' ▸ Style ▸ Font', 'fonts.popover', ADAPTED),
    (r'^Format Bar$', 'Does not apply: M+ has no format bar; its controls are in ' + FORMAT, '', OUT_OF_SCOPE),
    (r'^(Alpha|Instant Alpha|Mask)$', FORMAT + ' ▸ Image', 'insp.image.mask', ADAPTED),
    (r'^Data Editor$', FORMAT + ' ▸ Chart ▸ Edit data', 'insp.chart.options', ADAPTED),
    (r'^Find$', 'Find button of the header bar', 'head.find', SAME),
    (r'^Print$', 'Main menu ▸ Print', 'head.menu', ADAPTED),
    (r'^(Customize|Customize Toolbar)$', 'Does not apply: the header bar is not customizable', '', OUT_OF_SCOPE),
    (r'^Function$', 'Function button of the formula bar', 'formula.function', ADAPTED),
    (r'^Reorganize$', 'Inspector ▸ Data', 'insp.level1', ADAPTED),
    (r'^Formula List$', 'Sidebar, from its content switcher', 'side.mode', ADAPTED),
    (r'^Delete$', 'Delete key and context menu of the navigator', 'side.navigator', ADAPTED),
    (r'^Skip$', 'Context menu of the navigator and ' + FORMAT + ' ▸ Slide', 'side.navigator', ADAPTED),
    (r'^(Play|Rehearse|Record)$', 'Play button of the header bar and its menu', 'head.play', ADAPTED),
    (r'^Masters$', FORMAT + ' ▸ Slide ▸ Layout', 'insp.slide.master', ADAPTED),
    (r'^Themes$', 'Inspector ▸ Document ▸ Theme', 'insp.show.theme', ADAPTED),
    (r'^Edges On$', VIEW, 'head.menu', ADAPTED),
    (r'^Smart Builds$', 'Insert menu', 'head.insert', ADAPTED),
]

# The format bar of iWork '09 has no counterpart in M+: each mode is a tab of
# the Format pane of the Inspector, as in iWork 15.
FORMATBAR = [
    (r'^text$', FORMAT + ' ▸ Style, Layout and More (text)', 'insp.tabs'),
    (r'^outliner$', FORMAT + ' ▸ Style, Layout and More (text in the outline)', 'insp.tabs'),
    (r'^(graphic|shape|image|line|movie|maskedImage|imageMask|drawable|group)',
     FORMAT + ' ▸ Style, Image or Text, and Arrange (object)', 'insp.tabs'),
    (r'^(table|tableCell|formula|cell)', FORMAT + ' ▸ Table, Cell, Text and Arrange, and the formula bar', 'insp.tabs'),
    (r'.', FORMAT + ' ▸ Chart, Axis, Series and Arrange', 'insp.tabs'),
]


# Inspector panes and tabs, preference panes and panels:
# app|kind|item|Apple's Brazilian Portuguese name|place in Desk|component|how
MANUAL = '''
Pages|Inspector|Document ▸ Document|Documento|Inspector ▸ Document ▸ Document|insp.doc.paper|adapted
Pages|Inspector|Document ▸ TOC|Índice|Inspector ▸ Format, with the table of contents selected|insp.tabs|adapted
Pages|Inspector|Document ▸ Info|Info|Inspector ▸ Document ▸ Info|insp.info.fields|same
Pages|Inspector|Layout ▸ Layout|Leiaute|Inspector ▸ Format ▸ Layout (columns) and Inspector ▸ Document ▸ Section|insp.layout.columns|adapted
Pages|Inspector|Layout ▸ Section|Seção|Inspector ▸ Document ▸ Section|insp.section.setup|adapted
Pages|Inspector|Wrap|Ajuste|Inspector ▸ Format ▸ Arrange ▸ Text wrap|insp.arrange.wrap|adapted
Pages|Inspector|Text ▸ Text|Texto|Inspector ▸ Format ▸ Style|insp.text.spacing|adapted
Pages|Inspector|Text ▸ List|Lista|Inspector ▸ Format ▸ Style ▸ Bullets and lists|insp.text.lists|adapted
Pages|Inspector|Text ▸ Tabs|Tabulações|Inspector ▸ Format ▸ Layout ▸ Tabs|insp.layout.tab-stops|adapted
Pages|Inspector|Text ▸ More|Mais|Inspector ▸ Format ▸ More|insp.more.pagination|same
Pages|Inspector|Graphic|Gráfico|Inspector ▸ Format ▸ Style, with an object selected|insp.style.stroke|adapted
Pages|Inspector|Metrics|Métricas|Inspector ▸ Format ▸ Arrange ▸ Size, Position and rotation|insp.arrange.metrics|adapted
Pages|Inspector|Table ▸ Table|Tabela|Inspector ▸ Format ▸ Table|insp.table.headers|adapted
Pages|Inspector|Table ▸ Format|Formato|Inspector ▸ Format ▸ Cell|insp.cell.format|adapted
Pages|Inspector|Chart|Gráfico (dados)|Inspector ▸ Format ▸ Chart, Axis and Series|insp.chart.options|adapted
Pages|Inspector|Link ▸ Hyperlink|Hiperlink|Link popover, from Insert ▸ Link and the context menu|head.insert|adapted
Pages|Inspector|Link ▸ Bookmark|Marcador|Sidebar, from its content switcher, and Insert ▸ Bookmark|side.mode|adapted
Pages|Inspector|Link ▸ Merge|Mesclar|Dialog of the mail merge|commands.dialog|adapted
Pages|Inspector|QuickTime|QuickTime|Inspector ▸ Format, with a sound or movie selected|insp.tabs|adapted
Numbers|Inspector|Document|Documento|Inspector ▸ Document ▸ Info|insp.info.fields|adapted
Numbers|Inspector|Sheet|Folha|Inspector ▸ Document ▸ Sheet|insp.sheet.printing|adapted
Numbers|Inspector|Table|Tabela|Inspector ▸ Format ▸ Table|insp.table.headers|adapted
Numbers|Inspector|Cells|Células|Inspector ▸ Format ▸ Cell|insp.cell.format|adapted
Numbers|Inspector|Chart|Gráfico (dados)|Inspector ▸ Format ▸ Chart, Axis and Series|insp.chart.options|adapted
Numbers|Inspector|Text ▸ Text|Texto|Inspector ▸ Format ▸ Text|insp.text.spacing|adapted
Numbers|Inspector|Text ▸ Columns|Colunas|Inspector ▸ Format ▸ Text|insp.layout.columns|adapted
Numbers|Inspector|Text ▸ Bullets|Marcadores|Inspector ▸ Format ▸ Text ▸ Bullets and lists|insp.text.lists|adapted
Numbers|Inspector|Graphic|Gráfico|Inspector ▸ Format ▸ Style, with an object selected|insp.style.stroke|adapted
Numbers|Inspector|Metrics|Métricas|Inspector ▸ Format ▸ Arrange|insp.arrange.metrics|adapted
Numbers|Inspector|Hyperlink|Hiperlink|Link popover, from Insert ▸ Link and the context menu|head.insert|adapted
Numbers|Inspector|QuickTime|QuickTime|Inspector ▸ Format, with a sound or movie selected|insp.tabs|adapted
Keynote|Inspector|Document ▸ Document|Documento|Inspector ▸ Document ▸ Document|insp.show.options|adapted
Keynote|Inspector|Document ▸ Audio|Áudio|Inspector ▸ Document ▸ Audio|insp.audio.soundtrack|same
Keynote|Inspector|Document ▸ Spotlight|Spotlight|Inspector ▸ Document ▸ Info|insp.info.fields|adapted
Keynote|Inspector|Slide ▸ Transition|Transição|Inspector ▸ Animation, with nothing selected|insp.transition.effect|adapted
Keynote|Inspector|Slide ▸ Appearance|Aparência|Inspector ▸ Format ▸ Slide, with nothing selected|insp.slide.appearance|adapted
Keynote|Inspector|Build ▸ Build In|Entrada|Inspector ▸ Animation ▸ Build In|insp.build-in.effect|adapted
Keynote|Inspector|Build ▸ Build Out|Saída|Inspector ▸ Animation ▸ Build Out|insp.build-out.effect|adapted
Keynote|Inspector|Build ▸ Action|Ação|Inspector ▸ Animation ▸ Action|insp.action.effect|adapted
Keynote|Inspector|Build ▸ Build Order (drawer)|Ordem das composições|Inspector ▸ Animation: the order is at the foot of each tab|insp.build-in.order|adapted
Keynote|Inspector|Text ▸ Text|Texto|Inspector ▸ Format ▸ Text|insp.text.spacing|adapted
Keynote|Inspector|Text ▸ Columns|Colunas|Inspector ▸ Format ▸ Text|insp.layout.columns|adapted
Keynote|Inspector|Text ▸ Bullets|Marcadores|Inspector ▸ Format ▸ Text ▸ Bullets and lists|insp.text.lists|adapted
Keynote|Inspector|Graphic|Gráfico|Inspector ▸ Format ▸ Style, with an object selected|insp.style.stroke|adapted
Keynote|Inspector|Metrics|Métricas|Inspector ▸ Format ▸ Arrange|insp.arrange.metrics|adapted
Keynote|Inspector|Table ▸ Table|Tabela|Inspector ▸ Format ▸ Table|insp.table.headers|adapted
Keynote|Inspector|Table ▸ Format|Formato|Inspector ▸ Format ▸ Cell|insp.cell.format|adapted
Keynote|Inspector|Chart|Gráfico (dados)|Inspector ▸ Format ▸ Chart, Axis and Series|insp.chart.options|adapted
Keynote|Inspector|Hyperlink|Hiperlink|Link popover, from Insert ▸ Link and the context menu|head.insert|adapted
Keynote|Inspector|QuickTime|QuickTime|Inspector ▸ Format, with a sound or movie selected|insp.tabs|adapted
Pages|Preferences|General|Geral|Preferences ▸ General|dialog.preferences|same
Pages|Preferences|Rulers|Réguas|Preferences ▸ Rulers|dialog.preferences|same
Pages|Preferences|Auto-Correction|Correção Automática|Preferences ▸ Auto-Correction|dialog.preferences|same
Numbers|Preferences|General|Geral|Preferences ▸ General|dialog.preferences|same
Numbers|Preferences|Rulers|Réguas|Preferences ▸ Rulers|dialog.preferences|same
Numbers|Preferences|Auto-Correction|Correção Automática|Preferences ▸ Auto-Correction|dialog.preferences|same
Keynote|Preferences|General|Geral|Preferences ▸ General|dialog.preferences|same
Keynote|Preferences|Rulers|Réguas|Preferences ▸ Rulers|dialog.preferences|same
Keynote|Preferences|Slideshow|Apresentação de Slides|Preferences ▸ Slideshow|dialog.preferences|same
Keynote|Preferences|Presenter Display|Monitor do Apresentador|Preferences ▸ Slideshow ▸ Presenter display|dialog.preferences|adapted
Keynote|Preferences|Remote|Remoto|Out of scope: Keynote Remote was an iPhone app|-|out
Keynote|Preferences|Auto-Correction|Correção Automática|Preferences ▸ Auto-Correction|dialog.preferences|same
Pages|Windows and sheets|Template Chooser|Seletor de Modelos|Template chooser inside the window|templates.window|adapted
Pages|Windows and sheets|Find & Replace|Buscar e Substituir|Find bar under the header bar|find.bar|adapted
Pages|Windows and sheets|Search (side search)|Buscar|Sidebar ▸ Search results|side.mode|adapted
Pages|Windows and sheets|Spelling and Proofreader|Ortografia e Revisor|Popover on the text and the context menu|commands.dialog|adapted
Pages|Windows and sheets|Export|Exportar|Export dialog, from Main menu ▸ Export|head.menu|adapted
Pages|Windows and sheets|Save options|Opções de salvar|File chooser of GNOME; the options are in Inspector ▸ Document|insp.doc.password|adapted
Pages|Windows and sheets|Mail Merge|Mala direta|Dialog, from Search Commands|commands.dialog|adapted
Pages|Windows and sheets|Page numbering, sections and deleting pages|-|Inspector ▸ Document ▸ Section and the context menu of the thumbnails|insp.section.numbering|adapted
Pages|Windows and sheets|Styles: create, delete, import|-|Small dialogs, from Sidebar ▸ Styles|side.mode|adapted
Pages|Windows and sheets|EndNote|-|The phase of citations|commands.dialog|adapted
Pages|Windows and sheets|Warnings on opening, pasting and saving|-|Banner of warnings, toasts and alert dialogs|notice.banner|adapted
Pages|Windows and sheets|Styles Drawer|Gaveta de Estilos|Sidebar ▸ Styles|side.mode|adapted
Pages|Windows and sheets|Fonts (system panel)|Fontes|Fonts popover, from Inspector ▸ Format ▸ Style ▸ Font|fonts.popover|adapted
Pages|Windows and sheets|Colors (system panel)|Cores|Colors popover, from the color rows of the Inspector|colors.popover|adapted
Pages|Windows and sheets|Media Browser|Navegador de Mídia|Popover of the Media button|head.insert.media|adapted
Pages|Windows and sheets|Adjust Image|Ajustar Imagem|Inspector ▸ Format ▸ Image ▸ Adjustments|insp.image.adjust|adapted
Pages|Windows and sheets|Document Warnings|Avisos do Documento|Banner of warnings, which opens the list|notice.banner|adapted
Pages|Windows and sheets|Customize Toolbar|Personalizar Barra de Ferramentas|Does not apply: the header bar is not customizable|-|out
Pages|Windows and sheets|Full screen|Tela Cheia|Full screen of GNOME (F11), with the bars hidden|head.menu|adapted
Numbers|Windows and sheets|Template Chooser|Seletor de Modelos|Template chooser inside the window|templates.window|adapted
Numbers|Windows and sheets|Reorganize (sort, filter, categories)|Reorganizar|Inspector ▸ Data ▸ Sort, Filter and Categories|insp.reorg.sort|adapted
Numbers|Windows and sheets|Formula List and search|Lista de Fórmulas|Sidebar, from its content switcher|side.mode|adapted
Numbers|Windows and sheets|Function Browser and formula editor|Navegador de Funções|Popover of the Function button and editing in the formula bar|formula.function|adapted
Numbers|Windows and sheets|Find & Replace|Buscar e Substituir|Find bar under the header bar|find.bar|adapted
Numbers|Windows and sheets|Export|Exportar|Export dialog, from Main menu ▸ Export|head.menu|adapted
Numbers|Windows and sheets|Print|Imprimir|Print dialog of GNOME, with the print preview|head.menu|adapted
Numbers|Windows and sheets|Table styles and predefined tables|-|Inspector ▸ Format ▸ Table ▸ Table styles, and the popover of the Table button|head.insert.table|adapted
Numbers|Windows and sheets|Sheets pane, styles and instant calculations|-|Sheet switcher and instant calculations in the bottom bar; the tables and charts of the sheet in the sidebar|bottom.sheets|adapted
Keynote|Windows and sheets|Theme Chooser|Seletor de Temas|Template chooser inside the window|templates.window|adapted
Keynote|Windows and sheets|Export|Exportar|Export dialog, from Main menu ▸ Export|head.menu|adapted
Keynote|Windows and sheets|Send To|Enviar a|Out of scope: Apple services and programs|-|out
Keynote|Windows and sheets|Print|Imprimir|Print dialog of GNOME, with the options for slides, notes and handouts|head.menu|adapted
Keynote|Windows and sheets|Find & Replace|Buscar e Substituir|Find bar under the header bar|find.bar|adapted
Keynote|Windows and sheets|Choose Theme and slide size|Escolher Tema|Inspector ▸ Document ▸ Theme and Slide size|insp.show.size|adapted
Keynote|Windows and sheets|Save and passwords|-|File chooser of GNOME; the passwords are in Inspector ▸ Document|insp.show.options|adapted
Keynote|Windows and sheets|Customize Presenter Display|Personalizar Monitor do Apresentador|Preferences ▸ Slideshow ▸ Presenter display|dialog.preferences|adapted
Keynote|Windows and sheets|Running slideshow (Play, Rehearse, Record)|-|Slideshow window in full screen and the presenter display|head.play|adapted
Keynote|Windows and sheets|Smart Builds and Magic Move|Composições Inteligentes e Movimento Mágico|Insert menu and Inspector ▸ Animation|insp.transition.effect|adapted
Keynote|Windows and sheets|Light Table|Mesa de Luz|Main menu ▸ View ▸ Light table, in the document area|doc.light-table|adapted
Keynote|Windows and sheets|Master Slides (list)|Slides Mestre|Sidebar ▸ Master slides|side.mode|adapted
Keynote|Windows and sheets|Presenter Notes|Notas do Apresentador|Notes area under the slide|doc.notes|adapted
Keynote|Windows and sheets|Automator actions|-|Out of scope: Automator|-|out
'''


def parse_menus(lines):
    """Yield (path list, pt-BR name, shortcut, number of children collapsed)."""
    stack, rows, header = [], [], None
    for raw in lines:
        m = re.match(r'^(\s*)- "([^"]+)"(?: / pt-BR "([^"]*)")?(.*)$', raw)
        if not m:
            if '———' in raw:
                header = None           # a separator ends the group of a header
            continue
        ind, name, pt, rest = len(m.group(1)), m.group(2).strip(), (m.group(3) or '').strip(), m.group(4)
        depth = 0 if ind <= 2 else (ind - 2) // 2
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if depth == 0:
            header = None
        # Share lists its groups flat, under headers that the nib keeps disabled
        if stack and stack[-1][1] == 'Share' and 'desativado no nib' in rest and name in HEADERS:
            header = name
            continue
        if header and stack and stack[-1][1] == 'Share':
            name = f'{header} ▸ {name}'
        sc = re.search(r'— `([^`]*(?:Cmd|Ctrl|Opt|Shift)\+[^`]*|F\d+)`', rest)
        parent_collapsed = any(s[2] for s in stack)
        collapsed = name in COLLAPSE and depth >= 1 and '▸' in rest
        if parent_collapsed:
            for s in stack:
                if s[2]:
                    s[3]['n'] += 1
        else:
            row = {'path': [s[1] for s in stack] + [name], 'pt': pt, 'sc': sc.group(1) if sc else '', 'n': 0,
                   'menu': '▸' in rest}
            if depth >= 1:
                rows.append(row)
            stack.append((depth, name, collapsed, row))
            continue
        stack.append((depth, name, False, {'n': 0}))
    return rows


def section(lines, start, stop):
    a = next(i for i, l in enumerate(lines) if re.match(start, l))
    b = next((i for i, l in enumerate(lines) if i > a and re.match(stop, l)), len(lines))
    return lines[a + 1:b]


def first(rules, text, app=''):
    for rule in rules:
        if re.search(rule[0], text) or re.search(rule[0], f'{app}|{text}'):
            return rule[1:]
    raise SystemExit('no rule for: ' + text)


def catalogue_ids(path):
    """The ids of mockups/catalog.txt: every component of every version."""
    with open(path, encoding='utf-8') as f:
        return {line.split(' | ')[0] for line in f if ' | ' in line and not line.startswith('#')}


def main():
    out = []
    known = catalogue_ids(CATALOG) if CATALOG else None
    for app, fname, desk in APPS:
        lines = open(f'{SRC}/{fname}', encoding='utf-8').read().split('\n')
        for row in parse_menus(section(lines, r'^### 2\.1 ', r'^### 2\.2 ')):
            if row['menu'] and not row['n'] and row['path'][-1] not in COLLAPSE:
                continue                    # a submenu title; its items follow
            path = ' ▸ '.join(row['path'])
            place, comp, how = first(MENU_RULES, path, app)
            name = path + (f' ({row["n"]} items)' if row['n'] else '')
            out.append([app, desk, 'Menu', name, row['pt'] or '-', row['sc'] or '-', place, comp or '-', how])
        seen = set()
        # "All the items that the toolbar accepts", in the inventory
        tb = section(lines, r'^\*\*Todos os itens que a barra aceita', r'^### 3\.3')
        for raw in tb:
            m = re.match(r'^- "([^"]+)"(?: / pt-BR "([^"]*)")?', raw)
            if not m or m.group(1).strip() in seen:
                continue
            label = m.group(1).strip()
            seen.add(label)
            place, comp, how = first(TOOLBAR, label)
            out.append([app, desk, 'Toolbar', label, (m.group(2) or '-').strip() or '-', '-', place,
                        comp or '-', how])
        for raw in section(lines, r'^### 3\.3', r'^### 3\.4'):
            m = re.match(r'^\| `([A-Za-z]+)` \| (.*) \|$', raw)
            if not m:
                continue
            controls = len(re.findall(r'`[A-Za-z]+`', m.group(2)))
            place, comp = first(FORMATBAR, m.group(1))
            out.append([app, desk, 'Format bar', f'{m.group(1)} mode ({controls} controls)', '-', '-', place,
                        comp, ADAPTED])
    for raw in MANUAL.strip().split('\n'):
        app, kind, item, pt, place, comp, how = raw.split('|')
        desk = dict((a, d) for a, _f, d in APPS)[app]
        out.append([app, desk, kind, item, pt, '-', place, comp, how])
    if known:
        bad = sorted({r[7] for r in out if r[7] != '-' and r[7] not in known})
        if bad:
            raise SystemExit('component not in the catalogue: ' + ', '.join(bad))
    order = {'Menu': 0, 'Toolbar': 1, 'Format bar': 2, 'Inspector': 3, 'Preferences': 4, 'Windows and sheets': 5}
    apps = [a for a, _f, _d in APPS]
    out.sort(key=lambda r: (apps.index(r[0]), order[r[2]]))
    with open(OUT, 'w', encoding='utf-8', newline='') as f:
        f.write('# SPDX-FileCopyrightText: 2026 Desk contributors\n# SPDX-License-Identifier: GPL-3.0-or-later\n#\n'
                '# Every menu command, toolbar item, format bar mode, Inspector tab, preference\n'
                '# pane and panel of Pages 4.3, Numbers 2.3 and Keynote 5.3 (iWork \'09 9.3), read\n'
                '# on the reference virtual machine, and its place in the window of Desk (M+,\n'
                '# decision D-008). how: same (same place and form), adapted (another place or\n'
                '# form, the GNOME way), system (GNOME does it), out (out of scope). component\n'
                '# names a row of mockups/catalog.txt. apple_pt_br is the name in the Brazilian\n'
                '# Portuguese of iWork \'09, which Search Commands also finds. Made by\n'
                '# iwork_map.py. Tab-separated.\n')
        w = csv.writer(f, delimiter='\t', lineterminator='\n', quoting=csv.QUOTE_NONE, quotechar=None)
        w.writerow(['iwork', 'desk', 'where_in_iwork', 'item', 'apple_pt_br', 'iwork_shortcut', 'place_in_desk',
                    'component', 'how'])
        w.writerows(out)
    from collections import Counter
    print(len(out), Counter((r[0], r[2]) for r in out))
    print(Counter(r[8] for r in out))
    return out


if __name__ == '__main__':
    main()
