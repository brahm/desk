# Desk history

A short record of what is being done in Desk, newest first, for anyone
visiting the repository. Each line is one change; the reasons and the
details are in the [project log](docs/project-log.md), the
[decisions](docs/decisions.md) and the pull requests, and the current state
is on the [progress dashboard](https://brahm.github.io/desk/painel/).

## Changes since 0.7.0 (unreleased)

* Added: 3D objects from PRC, the 3D of PDF documents, by their
  tessellations (#279).
* Added: 3D objects from FBX files, with their materials, pictures and
  animation (#278).
* Added: the animation of USDZ models plays in the Object tab (#277).
* Added: 3D objects from OBJ files, with their materials and pictures
  (#276).
* Added: 3D objects from glTF and GLB, whose animation plays in the
  Object tab (#275).
* Added: 3D objects from USDZ files, drawn on the page and turned by
  dragging the handle in their middle or in the Object tab (#274).
* Added: donut, radar and bubble charts, shared X values, rounded
  corners of columns and bars, and the bevel of 3D charts (#273).
* Added: Insert > Record Audio records from the microphone, the Movie and
  Sound group trims with handles and keeps a description, and movies and
  sounds can be optimized, on insert or from the Format pane (#272).
* Added: Insert > Drawing draws freehand with a pen, a pencil, a crayon
  and a fill, and the Drawing tab animates the drawing (#271).
* Added: Reverse Colors turns a gradient fill around, and Saved Fills
  keeps a color or a gradient to use again in any document (#270).

* Added: The Shape button opens a shape library by category, with a
  search field, and Arrange > Shape > Save to My Shapes keeps a shape of
  the page in My Shapes, where a right click renames or deletes it
  (#269).

* Added: Arrange > Shape unites, intersects, subtracts and excludes the
  selected shapes into one, holes and all, and Break Apart makes a shape
  of several parts into one for each; ODT keeps the parts as one path
  (#268).

* Added: Insert > Image Gallery puts several pictures in one frame,
  stepped through with arrows on the page; the Gallery tab adds, orders
  and removes them, gives each a caption or one for all, and describes
  each; ODT keeps them all and shows the current one to other programs
  (#267).

* Added: The Image tab shows Exposure and Saturation with sliders and an
  Enhance button that sets them from the picture; Adjust Image opens the
  full window with the levels over the histogram, Auto Levels and
  sharpness (#265).

* Changed: checkout, upload-artifact, download-artifact,
  upload-pages-artifact and deploy-pages moved off the deprecated
  Node.js 20, reuse-action went to version 6, and Dependabot proposes
  the updates of the actions in one pull request a week (#264, which
  takes #208 to #212).

* Added: SVG images, drawn sharp at every size, and HEIC photographs
  can be inserted, dragged in and opened from ODT and Pages files; in
  ODT a PNG goes beside them for other programs (#263).

* Added: Objects have a title, above or below them, and a caption,
  turned on in the Style tab, edited in place and kept in ODF (#262).

* Added: Pictures, shapes and equations pasted into the text of a text
  box or a shape go in its lines, and are kept in ODF (#261).

* Changed: The parity matrix of Desk Writer describes the objects and
  charts of Pages 15.4, and nine new rows test the image gallery,
  titles and captions, 3D objects, drawings, combined shapes, the new
  chart types, interactive charts, rounded corners and reference lines
  (#260).

* Added: The plan of phase W4 of Desk Writer, which brings the objects
  and charts of Pages 15.4: the shape library and combined shapes, the
  image gallery, captions, 3D objects, drawings, recorded sound, SVG
  and HEIC images, and the donut, radar, bubble and interactive charts.
  3D objects will open from USDZ, glTF, GLB, U3D, PRC, FBX and OBJ
  (#259).
* Changed: Writer structure and templates (W3) is done, checked on the
  Mac with Pages 15.4; Writer objects and charts (W4) is the phase in
  progress (#259).
* Fixed: The dashboard counts the rows of W2 that the validation on the
  Mac checked as done (#259).

* Changed: The menu of merge fields offers the fields of Pages 15.4 first,
  in its order, and the others under More Fields. (#258)

* Added: The artistic lines of Pages, chalk, thick brush, chisel pen,
  dry brush and uneven ink, for the lines of shapes, pictures and text
  boxes and the border of a drop cap, drawn by Desk in its own way.
  Other programs show them as solid lines. (#257)

* Added: A mail merge can take the contacts of GNOME, all of them or a
  group, and the Preview, Merge and Fill With My Card put your own card
  in the sender fields. A vCard file dropped on a merge field fills the
  fields. (#256)

* Added: Mail Merge in the Document tab: Populate With a vCard file or a
  spreadsheet, Add Merge Field, a Preview that steps through the
  records, and Merge, which opens a new document with a letter for each
  record. Insert > Field > Mail Merge Field puts a field, and its menu
  changes it. (#255)

* Added: The records of a mail merge are read from vCard files and from
  the tables of spreadsheets, ODS or CSV, whose columns fill the merge
  fields of the same name, in English or Portuguese. (#254)

* Added: Merge fields of a mail merge, for the recipient and the sender,
  are kept in documents as fields of a column of data, which LibreOffice
  reads, with the source of their data; those of Pages '09 open as merge
  fields. (#253)

* Added: Insert > Equation (Ctrl+Alt+E) writes an equation in LaTeX or
  MathML with a preview of it, in the line of the text at its size and
  color, or on the page of a page layout document; a double click or
  the Equation tab edits it again. (#252)

* Added: Equations are drawn on the page and in PDF, from their MathML,
  and LaTeX turns into MathML: fractions, roots, scripts, sums and
  integrals with their limits, fences, accents, matrices and cases.
  (#251)

* Added: Equations are kept in documents as MathML, the formula objects
  that LibreOffice Math reads and writes, with the LaTeX they were
  written in. (#250)

* Added: Page templates in the window: Page Templates and Reapply Page
  Template in the Add Page menu, the Page Template of the Section tab, a
  template made from the page in sight, and the Placeholder and Tag of
  the selected objects in the Arrange tab. (#249)

* Added: Page templates of page layout documents, as in Pages 15.4: a
  page added from a template copies its objects, its text and media
  placeholders keep their content when the template is applied again or
  changed, by tag, and all of it is saved in the document. (#248)

* Added: Links to a phone number, and links on shapes, pictures, text
  boxes and groups, as in Pages 15.4. (#247)

* Added: The Bookmarks tab of the Document pane of Pages 15.4, and the
  bookmarks that a link suggests from the paragraphs of the styles
  chosen in it, Heading 2 and Heading 3 in a new document. (#246)

* Added: Each table of contents lists the whole document, its section
  or up to the next table, chosen in Insert > Table of Contents and
  changed in the Range of the Format pane, as in Pages 15.4. (#245)

* Added: Captured Pages, the Capture Pages of Pages '09: a section
  captured under a name is inserted again from Insert > Break or the Add
  Page menu, and saved with the document. (#244)

* Added: The Section tab of Pages 15.4: each section writes the numbers
  of its pages in its own format, can hide its headers and footers on
  its first page, and a new section starts from the tab. (#243)

* Added: Each section can fill its pages with a color, a gradient or a
  picture, as the background of the Section tab of Pages 15.4. (#242)

* Added: Headers and footers have the three fields of Pages 15.4, left,
  centre and right, and a header with text in the left field alone stays
  the free header of Pages '09. (#241)

* Changed: The parity rows of sections, headers and footers, tables of
  contents, bookmarks, links and mail merge now describe Pages 15.4, and
  new rows test page templates, page backgrounds and equations, from a
  session with Pages 15.4 on the Mac. (#240)

* Added: The plan of phase W3 of Desk Writer, which brings page
  templates, the background and border of pages, tables of contents
  per section, bookmarks and links of Pages 15.4, equations and mail
  merge (#239).
* Changed: Writer review (W2) is done, checked on the Mac with Pages
  15.4; Writer structure and templates (W3) is the phase in progress
  (#239).
* Changed: in Desk Writer, the bar of comments and changes, Undo, the
  count of Find & Replace and the word count of a selection say what
  Pages 15.4 says, as the validation of W2 on the Mac found (#238).
* Added: in Desk Writer, Save a Copy as Final, in the menu of the bar
  of comments and changes, saves a copy with every tracked change
  accepted and no comments, and the document keeps them (#235).
* Added: in Desk Writer, a table cell written, formatted or filled
  while tracking changes keeps what it was until the change is
  accepted or rejected, shows a mark in its top right corner in its
  author's colour, gray when several authors changed it, and the bar
  of comments and changes goes to it; ODT keeps it (#234).
* Added: in Desk Writer, an object moved or resized while tracking
  changes shows in a dashed frame in its author's colour, with its
  bubble, and Previous, Next, Accept and Reject of the bar of comments
  and changes go to it and take it (#233).
* Added: in Desk Writer, a floating object moved or resized while
  tracking changes keeps where it was until the change is accepted or
  rejected, also in ODT, and an object in the line of the text that a
  deletion took is kept in ODT (#232).
* Fixed: in Desk Writer, a paragraph whose break before it was deleted
  while tracking changes keeps the section it starts, its entry of a
  table of contents and the style overriding its list in ODT (#231).
* Added: in Desk Writer, the views of markup without deletions join
  the paragraphs whose break was deleted in notes, headers, footers,
  text boxes and shapes too (#230).
* Fixed: in Desk Writer, a tracked change that another change
  interrupts opens from ODT as one change (#229).
* Fixed: in Desk Writer, a paragraph break that another author added
  keeps that insertion when it is deleted while tracking changes, and
  rejecting the deletion gives it back, also in ODT (#228).
* Added: in Desk Writer, the views of markup without deletions join the
  paragraphs whose break was deleted, as accepting the deletion would
  (#227).
* Fixed: in Desk Writer, the tracked change of the formatting of a
  paragraph that a deleted break joins to another stays on its
  paragraph in ODT (#226).
* Fixed: in Desk Writer, text that another author added or formatted
  keeps that change when it is deleted while tracking changes, and
  rejecting the deletion gives it back, also in ODT (#225).
* Added: Desk Writer accepts or rejects the tracked change selected,
  or those of the selection, from its bubble, the tracking bar and the
  command search (#224).
* Added: in Desk Writer, the tracked change that Previous or Next
  selects shows a bubble with its author, its date and what it did
  (#223).
* Added: Desk Writer shows tracked changes with their marks, without
  the text deleted, or as final, from View > Change Markup and the
  tracking bar, keeps the view in the file, and marks the lines with
  changes with a bar in the margin (#222).
* Added: Desk Writer has the bar of comments and changes, which turns
  change tracking on, pauses it and turns it off, goes through the
  comments and changes, and accepts or rejects them all; comments and
  changes made under an earlier author name show under the current one
  (#221).
* Added: while a document tracks its changes, Desk Writer keeps what
  is typed, deleted and formatted as changes of the author, with the
  date, and ODT files keep whether tracking is on or paused (#220).
* Added: Desk Writer keeps the changes of paragraph styles and of the
  formatting of paragraphs tracked in ODT files, with what the
  paragraphs were before (#219).
* Added: Desk Writer keeps the changes of formatting tracked in ODT
  files, with the formatting the text had before, and marks them with a
  dashed line in the colour of their author (#218).
* Added: Desk Writer keeps the insertions and deletions tracked in the
  notes, headers, footers, text boxes and shapes of ODT files, and the
  list item of a paragraph whose break before it was deleted (#217).
* Added: Desk Writer keeps the insertions and deletions tracked in the
  body of ODT files, and shows them in the colour of their author,
  instead of accepting them when the file opens (#216).
* Added: the author's name in Preferences, and View > Author Color in
  Desk Writer; comments and highlights are drawn in the colour of
  their author (#215).
* Added: Insert > Highlight (Ctrl+Shift+H) in Desk Writer marks the
  selected text for review, and Remove Highlights and Comments takes
  them off the selection (#214).
* Changed: a file that Desk opened with content it does not read is no
  longer saved over on its own; Save asks first, to save over it or as a
  new file (D-031).
* Changed: the review of all the code (phase RV) is over, and Writer
  review (W2) resumes.
* Changed: a link to the web opens at once, asks first or never opens, as
  a new preference says, which the first such link asks for; other links
  outside the document ask, and the address of a link shows under the
  pointer (D-030).
* Changed: the Flatpak of Desk is built without network access, from
  crates checked against Cargo.lock, and reads JPEG 2000 in PDFs through
  OpenJPEG (review RV, batch 6).
* Fixed: text and pictures pasted from other programs are read with a
  limit, a tab dragged to another window keeps full screen and its
  shortcuts, and the address shown for a link is the one it opens
  (review RV, batch 5).
* Fixed: drafts are offered back after a crash in Flatpak, autosave no
  longer writes over a file whose original it cannot keep, Revert keeps
  the changes as a version, and Save As asks before replacing a file
  (review RV, batch 5).
* Fixed: Desk no longer hangs or runs out of memory on narrow table
  columns, tiny fonts, huge wrap margins, huge PNG pictures or text
  shadows, nor closes on charts without categories or with extreme
  values (review RV, batch 4).
* Fixed: pasted chart numbers such as 1.234,56, the ffl ligature in
  PDFs, notes numbered by page, and page breaks with footnotes.
* Fixed: Desk no longer closes or hangs on movies and sounds made to
  break Reduce Media, nor on documents of thousands of linked text boxes,
  paragraphs or objects (review RV, batch 3).
* Fixed: Undo keeps the selection of each action after a thousand of
  them, and typing after a Redo is a new step (review RV, batch 3).
* Fixed: typing after an Undo is a new step of Undo, and conditional
  formatting rules copied into new rows and columns compare with the
  right cells.
* Fixed: Desk no longer closes, or hangs, on formulas nested thousands
  of levels deep, long running balances, formulas over large ranges or
  texts that double from cell to cell, number formats of many zeros, or a
  long text pasted into a table (review RV, batch 2).
* Fixed: formulas keep their small constants when rows are inserted, and
  conditional rules follow the cells they compare with.
* Fixed: sorting by a formula with TODAY, merging cells with numbers, and
  dates typed without a year use the right day, language and year.
* Fixed: rows added after a footer join it, splitting a cell keeps the
  merged cells beside it, and Jan, Feb fills on in English.
* Fixed: sums of hundreds of cells written by other spreadsheets open,
  and documents of many tables, or with wildcard searches over long
  texts, no longer hold Desk while they open.
* Fixed: Desk no longer closes, or runs out of memory, on Pages '09 and
  ODT files made to loop or to repeat cells without end, nor on GIF
  pictures that declare a huge size (review RV, batch 1).
* Fixed: Pages '09 files larger than 256 MB open, and their pictures keep
  their size instead of coming in at 256 pixels.
* Fixed: a Pages package folder can no longer bring into a document,
  through a link, a file of yours from outside the package.
* Fixed: files with control characters pasted from the web open again in
  LibreOffice, and chart rows written once for several are read whole.
* Changed: development is paused for a review of all the code for bugs
  and optimizations (phase RV); Writer review (W2) resumes after it.
* Fixed: the tables of Pages '09 documents open in Desk Writer with
  their fills, lines, alternating rows, text styles, cell formats and
  conditional formatting, as Pages shows them.
* Fixed: Desk Writer opens the tables in the text of Pages '09
  documents, which it dropped, with their cells, formulas, merged cells
  and the comments of their cells.
* Changed: the comment of a table cell in Desk Writer is a comment
  like the others, with its author, date and replies, listed by page
  and gone to from Next Comment; ODT keeps it as the annotation of the
  cell.
* Added: Desk Writer puts comments on objects, and opens those of Pages
  '09 documents on shapes.
* Added: Desk Writer writes comments in headers, footers, notes, text
  boxes and shapes, and lists every comment by page.
* Added: Desk Writer keeps the comments of notes, headers, footers,
  text boxes and shapes of ODT files.
* Added: Desk Writer opens the comments of Pages '09 documents on
  their text.
* Added: Desk Writer marks each comment in the margin and shows it in a
  bubble at its text, where a new comment is written too.
* Added: Desk Writer writes, answers, edits and deletes comments in the
  Comments page of the sidebar, and marks the text under them.
* Fixed: the cursor and the selection after text in another format, in
  the middle of a line, no longer go back to before it.
* Added: Desk Writer keeps the comments of ODT files, with their
  replies, on the text they are on, and saves them.
* Added: The plan of phase W2 of Desk Writer, which brings comments
  with replies, highlights and the tracking of changes of Pages 15.4.
* Changed: Writer to the behaviour of 15 (W1) is done, checked on the
  Mac with Pages 15.4; Writer review (W2) is the phase in progress.
* Changed: lists, tabs and indents are laid out as Pages 15.4 lays them
  out: a list item's text goes to a tab stop when its label needs the
  room, its label stands alone on its line near the right edge, text on
  a right, centred or decimal stop counts its kerning, tab stops stop
  short of the edge, and the Indents of the inspector keep 18 points of
  the column, with First where the first line starts.
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
