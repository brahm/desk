# Desk history

A short record of what is being done in Desk, newest first, for anyone
visiting the repository. Each line is one change; the reasons and the
details are in the [project log](docs/project-log.md), the
[decisions](docs/decisions.md) and the pull requests, and the current state
is on the [progress dashboard](https://brahm.github.io/desk/painel/).

## Changes since 0.7.0 (unreleased)

* Changed: Screen View places objects as Pages 15.4 does: those that
  move with the text show after their paragraph, centred at their size,
  with text boxes and rectangles with text across the column, and those
  fixed on a page are hidden. Its column is 664 points long at most, in
  the middle of the window, a table in the text is that wide, and
  pictures and shapes keep their size on the screen. Text shows at the
  sizes Pages 15.4 shows it there, from 10 to 26 points at 100%, without
  its left indent, with its line spacing in one of four steps of the
  line of the font, space before and after in quarter lines, list
  labels 18 points in at each level, and tabs every 36 points.
* Changed: documents are laid out as Pages 15.4 lays them out: text
  wraps tightly around the outline that Pages traces from the pixels of
  a picture, tables of contents, superscripts, text that overflows the
  bottom of a shape, the line of a sound, and hyphenation, which breaks
  a word only where its next letter would not fit.
* Added: Screen View, as in Pages (View menu, Ctrl+Shift+R): the text
  flows in one column as wide as the window, without pages, margins,
  headers or footers, with the objects in the flow and Text Size to make
  the text bigger or smaller; a preference opens new documents in it.
* Added: dragging the selected cells of a table, as in Pages: they move,
  with Alt they are copied, onto another table they go there, and onto
  the page they make a new table; a table that grows moves the objects
  beside and under it aside; and custom formats of cells, made, edited,
  renamed, moved and deleted from the Data Format menu.
* Added: the title of a table, as in Pages: its name centered above it,
  with a line around it when asked; and locked tables and charts, which
  are not edited until unlocked.
* Added: table and chart styles, as in Pages: six of each in a new
  document, shown as pictures in the Table and Chart buttons, where a
  click inserts a table or chart of the style, and in the Table and Chart
  tabs, where a click applies it; Copy Style, Paste Style and Create Style
  pass a look to another table or chart and keep it as a new style.
* Added: threads of text boxes, as in Pages: a circle on a selected box
  starts a thread or adds a box to it, each box shows its number in the
  color of its thread, and its menu moves it to another thread, puts the
  thread in layout order, creates a new one or removes the box.
* Added: columns given to selected text, between layout breaks, ending at
  the same height when text follows them.
* Added: dragging the bullet of a list item moves it, with its sub-items,
  to another place and level, shown by a blue line and a triangle.
* Added: Color, Gradient, Advanced Gradient, Image and Tinted Image fills
  of text in the Font group of the sidebar.
* Added: text filled with a gradient or a picture, drawn and kept in
  ODF documents.
* Added: the six Drop Cap Styles of Pages, and saving, redefining and
  deleting styles of drop caps.
* Changed: a drop cap stands out into the margin by a percent of its
  width, up to 200 %, and the border of its shape can be dashed or
  dotted.
* Added: a shape behind the letters of a drop cap, with its fill,
  border, letter size and rounded corners.
* Added: the options of drop caps: raised lines, text that follows the
  shape of the letters, spacing and outdent into the margin.
* Added: drop caps, in the Style tab of text, kept in the standard ODF
  drop cap.
* Added: Language and Region in the main menu, which sets how the tables
  and date fields of a document write dates and numbers.
* Added: Set as Default Text Box Appearance, which makes a text box, with
  its text, the look of the new text boxes of a document.
* Added: Default Font in the preferences, the font and size of new
  documents of Desk Writer.
* Added: a choice of double and single quotes for smart quotes, smart
  dashes, and Convert Quotes for the quotes already typed.
* Changed: the cells of tables take the corrections of typing, such as
  smart quotes and the replacement list.
* Added: Format Fractions Automatically in the preferences, which makes
  1/2 into ½ while typing.
* Added: Start Case in the capitalization of Desk Writer, and Make Upper
  Case, Make Lower Case and Capitalize, which change the characters.
* Changed: Desk Writer places lines on the page as Pages 15.4 does, a
  fraction of a point higher than Pages '09.
* Changed: Baskerville joins fi and fl, Didot Bold is no longer kerned,
  and a space after a comma may hang past the end of a line, as in
  Pages 15.4.
* Added: 18 rows of the parity matrix of Desk Writer for the features of
  Pages 15, seen in Pages 15.4 on the Mac, under the phase W1.
* Changed: Pages 15.4 was studied on the Mac for phase W1: drop cap,
  automatic fractions, Screen View, table title and lock, and the
  missing parts of eleven partial rows of Desk Writer.
* Added: The plan of phase W1 of Desk Writer, which brings its text,
  styles, view and tables to the behaviour of Pages 15.4.
* Changed: Four rows of Desk Writer moved out of W1: the review
  highlight to W2, right-to-left and vertical text and right-to-left
  tables to W6, and the missing functions to T2.
* Changed: The common base (B1) is done; Writer to the behaviour of 15
  (W1) is the phase in progress.
* Changed: The 39 finished rows of the common base that the validation
  on the Mac checked count as done on the dashboard.
* Added: The table of contents in the sidebar of Desk Writer, with Edit
  and Insert.
* Added: Document Warnings in the View menu of Desk Writer and Desk
  Slides.
* Added: Preferences in the main menu of Desk Tables.
* Changed: Twelve rows of the common base moved to the phases that build
  their engines: the cells and organizing of Desk Tables, its media and
  warnings, Build Order and the slideshow preferences of Desk Slides, and
  Screen View of Desk Writer.
* Added: Every TODO, FIXME, XXX or HACK in the code cites an open backlog
  row, and a check in the CI fails on one that does not (D-026).
* Added: Zoom to Selection in the Zoom menus of Desk Writer, Desk Tables
  and Desk Slides, and Zoom to Fit Content and Auto-Center in Desk Slides.
* Added: First Page and Last Page in the View menu of Desk Writer.
* Fixed: Undo and Redo in the context menu of Desk Tables name the change.
* Added: The 32 finished rows of the common base were compared with
  Pages, Numbers and Keynote 15.4 on a Mac, and the other 21 have the
  reference of iWork 15.4; three gaps went to the backlog.
* Changed: The screenshots of Desk Writer in its metainfo show the new
  window, with the Format pane, Fonts, Find and the dark style.
* Added: None in the Colors popover takes away a background, a fill, a
  line or a shadow.
* Added: The Colors popover shows the colors the document already
  uses, under the palette.
* Added: The Font group of the Inspector has a Typeface row and Advanced
  Options with character spacing, baseline, capitalization, ligatures
  and the background of the characters.
* Changed: The documents of Pages in the corpus are compared with PDFs
  of Pages 15.4, except 20 that keep those of Pages '09 for now.
* Changed: The reference cases of Writer are measured against Pages
  15.4 instead of Pages '09, and all 13 match.
* Changed: Writer lays out line spacing in multiples, the bottom of the
  page, Title Case and the spaces of small caps as Pages 15.4 does.
* Changed: Writer saves documents by itself as they are edited and
  closes them without asking; Ctrl+S keeps a version.
* Added: Browse Versions in the main menu, to restore an older version of
  a document or open it as a copy.
* Changed: Tables shows its sheets as tabs in the bottom bar, with a +
  button to add one; the sidebar shows the tables and charts of the
  sheet in view, and the zoom floats over the sheet.
* Changed: Writer and Slides have no bottom bar: the zoom and the word
  count float over the document, and Go to Page and Navigate By are in
  the View menu.
* Changed: The format bar is gone; its controls are in the Format pane
  of the Inspector, with the paragraph style over the tabs. Slides has a
  Slide tab, and Tables has Cell and Text tabs.
* Added: A Colors popover with the GNOME palette, the eyedropper and a
  custom color, on every color row.
* Added: Documents open as tabs of one window, with an overview of the
  open documents; Ctrl+T opens a new document in a tab. The Sidebar button
  only shows the sidebar, and View is in the main menu.
* Changed: Desk is made for users of GNOME first: where GNOME and iWork
  differ, Desk follows GNOME. The document tabs take GNOME's shortcuts in
  the three apps, and the sheets of Tables move to Alt+PageUp and
  Alt+PageDown.
* Changed: Three Pages '09 test documents (custom bullets, sound and
  movie, movie settings) were made again with a sound, a movie and pictures
  made by the project, so no file from Apple software is left in them.
* Added: Mockups of what opens from the chosen window (M+): menus,
  Fonts and Colors from the Format pane, find, dialogs, the template
  chooser, a narrow window and the small screen notice; the component
  inventory now lists them.
* Changed: The description of the interface, its component inventory and
  the map of every iWork '09 command are in English and describe the
  chosen window (M+); the mockups are written in English and drawn in
  Portuguese only for the site.
* Changed: The parity matrices, the parity report and the corpus cases
  are in English, and the matrices describe the chosen window (M+) and
  automatic saving with versions.
* Added: Tests for six more rows of Writer tables and charts (62 of 76
  tested); selection and keyboard navigation of cells, and copying cells
  as text, moved into the table engine shared by the three apps; the 14
  rows that need later features moved to phases W1, W4, T2, T3, T4, S3
  and X1.
* Added: Full definitions of the 43 `desk:` extensions to ODF (element,
  value, fallback, matrix rows), and a check that keeps the catalogue and
  the code in step; phase B1 starts.
* Changed: The README and the website start with what Desk is and what each
  app does, shown with mockups of the chosen window (M+) in the language of
  the page; then the story and the reasons behind Desk, its architecture,
  where it stands, a high-level roadmap, how to help and a link to the
  progress dashboard. They no longer say parity with iWork '09, and they
  say that Desk is not a clone of iWork.
* Added: This history file, kept up to date at the end of each set of
  tasks.
* Changed: The Flatpak metainfo of Desk Writer describes the new goals.
* Added: The project log, the long history of the project with each
  decision and its reason (#120, #122).
* Removed: The text engine experiment and 28 stale remote branches; the
  reference documents that carry media from Apple software are listed to
  be remade (#121).
* Added: The progress dashboard, generated from the repository and
  published at brahm.github.io/desk/painel (#118).
* Changed: New target: feature parity with the current Pages, Numbers and
  Keynote (15.4), keeping what iWork '09 did and the newer versions lost,
  and leaving out what needs AI, an online service or a payment (#117).
* Added: Goals, twenty decisions, an inventory of 566 features, the M+
  window, a catalogue of the ODF extensions, the architecture, a roadmap
  of nineteen phases with parity forecast for the end of January 2027,
  a register of what is left undone, and guides for contributors and
  their coding agents (#117).
* Changed: Checking has two references: iWork 15.4 on a Mac, and
  iWork 9.3 on a Lion virtual machine for what only iWork '09 has (#120).
* Removed: The legal review of the reader of current iWork files (#119).
* Added: Desk Writer tables and charts, phase 7, with 56 of its 76 matrix
  rows tested; the rest moved to phase B1 (#116).
* Added: The Desk Tables window: sheets, several tables on a sheet, the
  formula bar and instant calculations (#115).
* Added: The Desk Slides window: slide navigator, outline, light table and
  presenter notes (#114).
* Added: Object styles and the Text tab of shapes in the Inspector (#113).
* Changed: Ten names and shortcuts approved by the owner; documents can be
  renamed from the title (#112).
* Changed: Desk Writer moved to the interface B window in seven steps: the
  window, header bar and menus, slim format bar, two-level Inspector,
  sidebar pages, bottom bar, and the commands and shortcuts of the
  inventory (#104, #106 to #111).

## Changes in 0.7.0 (14 Nov 2026)

* Added: An inventory of 933 interface items of Pages, Numbers and Keynote
  '09 placed in Desk, and mockups of three windows drawn with the real
  GTK widgets; interface B chosen (#102, #103).
* Changed: The minimum screen is 1280 × 720, after display scaling (#105).
* Changed: GNOME 51, GTK 4.24, Libadwaita 1.10 and Rust 1.100, with the
  plan to bring the interface in line with the GNOME HIG (#100).
* Fixed: The gaps of the sixth review, with the matrices and roadmaps
  aligned to the decisions of 13 November (#101).

## Changes in 0.6.0 (13 Nov 2026)

* Added: A second reference machine, OS X Lion with iWork 9.3, to compare
  the interface with Pages 4.3.
* Added: Metric-compatible substitutes for the 41 fonts of the Pages
  templates, and formatted copy (#97).
* Changed: Undo names the change; Alt-arrow copies; Free Form; the outline
  prints as Pages does (#98, #99).
* Added: Desk Writer objects, phase 6: shapes, text boxes, pictures and
  groups; move, resize, turn and arrange; linked text boxes; text in
  shapes; inline objects; gradients, line ends, shadows and reflections;
  masks, Instant Alpha and Adjust Image; the pen and connection lines;
  movies and sounds; the Media Browser; master objects; page layout
  documents (#49 to #95).
* Added: Desk Neue, a free font with the widths and kerning of Helvetica
  Neue, so that lines break where Pages breaks them (#95).
* Fixed: A long document came out with 72 pages against 62 in Pages; line
  metrics, blank pages at section starts and the justification rule now
  match (#87, #88, #91).
* Added: The parity matrices of Desk Tables and Desk Slides (#95).

## Changes in 0.5.0 (4 Nov 2026), the first beta

* Added: Sections, columns and facing pages; headers, footers and fields;
  footnotes and endnotes; table of contents, hyperlinks and bookmarks;
  outline mode, thumbnails and page modes (#34 to #48).
* Added: Tab stops and rulers, character and paragraph effects, styles,
  lists, Find & Replace, auto-correction, hyphenation, spelling and the
  proofreader (#25 to #30).
* Added: The editor: open, edit and save, the format bar, the Inspector,
  invisibles, screen reader support and the Portuguese translation
  (#21 to #24).
* Added: The reader of Pages '09 documents, substitutes for missing fonts,
  a log of what an import leaves out, and one Pages document per row of the
  parity matrix (#13 to #20).
* Added: The signed Flatpak repository, with nightly and beta channels.
* Added: The language policy (#50).
* Fixed: 23 items found by the first gap review (#31 to #33).
* Removed: The Pages '08 format.

## Changes on 31 Oct 2026, the first day of code

* Added: The document model with undo and redo, the text engine,
  pagination, PDF output, the ODT reader and writer, validation against
  the ODF schemas and fuzzing of the reader (#3 to #10).
* Added: App and document icons and the suite logo (#4).
* Added: The website and the README, in English and Brazilian Portuguese
  (#11).
* Added: Layout parity with twelve reference documents made in Pages '09
  (#12).
* Added: The licences, GPL-3.0-or-later for the apps and MPL-2.0 for the
  file format libraries, the Cargo workspace and the CI (#1, #2).

## Planning on 30 Oct 2026

* Added: Feature inventories of Pages, Numbers and Keynote '09, the
  architecture and the first roadmap.
* Added: A reference virtual machine with iWork 9.3.
