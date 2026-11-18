# Project log

The history of Desk from its first day, written to be read on its own: what
was done, each decision with its reason, and what each decision led to. It
is the source for posts about the project and for the project page, and it
is the long form of [decisions.md](decisions.md), which keeps one row per
decision.

**This is a living document.** Whoever closes a set of tasks adds an entry
to [the timeline](#timeline) and, when a decision was taken or replaced, to
[the decisions](#decisions-with-their-reasons-and-consequences). See
[Keeping this log](#keeping-this-log).

Dates are the owner's local dates (Brazil, UTC−3). Pull request numbers
refer to <https://github.com/brahm/desk>.

## Desk in one paragraph

Desk is a free office suite for GNOME: Desk Writer, Desk Tables and Desk
Slides, written in Rust with GTK 4 and Libadwaita, with OpenDocument as
its native format. It began on 30 October 2026 as a "retro clone" of Apple
iWork '09 (version 9.3) and, since 16 November 2026, aims at feature
parity with the current Pages, Numbers and Keynote (15.4), keeping what
iWork '09 did and the current versions lost. It is developed by its owner, Brahm,
who supervises, decides and approves, together with Claude, an AI model
made by Anthropic, which writes the code, the tests and the documentation.

## Why the project exists

In the owner's words, from the text he wrote for the website on
31 October 2026: he used Linux at university, moved to the Mac around 2008
because he needed Microsoft Office and Photoshop, met iWork there and
dropped Microsoft Office for it. Years later Apple rewrote iWork for
64 bits and released it without several useful features, some of which
never came back. Back on Linux as his main system, he decided to build an
equivalent of iWork for it, because LibreOffice has much that he does not
use and does not look like a GNOME application, and GNOME is his favourite
desktop.

Two more intentions were stated on the same day and still hold: the engine
is built apart from the interface so that the libraries can be used by
other projects, and the project is open about being written by one person
with an AI model, under ordinary software engineering discipline: a
feature inventory, parity matrices, tests named after matrix rows,
reference documents and continuous integration.

## The seventeen days in numbers

Read on 16 November 2026, at the merge of pull request 117:

| | |
| --- | --- |
| Days since the first commit | 17 |
| Pull requests merged | 117 |
| Commits on `main` | 280 |
| Rust in `crates/` | about 92,000 lines |
| Beta releases on the Flatpak beta channel | 3 (0.5.0, 0.6.0, 0.7.0) |
| Features of the new target in scope | 494 |
| Of those, done and needing no second look | 62 (12.6 %) |
| Forecast of full parity | end of January 2027 |

The last three rows come from the inventory and the roadmap written on
16 November. By the older target (iWork '09) Desk Writer had 158 of 304
matrix rows done on 9 November, before the change of target.

## Timeline

### 30 October: planning

- The owner opened the project with the three iWork '09 manuals and asked
  for a plan. His brief: an architecture document built from a feature
  inventory, a shared base for the three apps so that what the manuals
  describe across them works (a Numbers table in Pages, for example),
  Pages first, everything developed remotely, parity of features and
  behaviour plus document compatibility, and ODF as the native format with
  the extensions needed.
- Feature inventories were written from the manuals: Pages '09 (260
  features, 69 of them needing an ODF extension), then Numbers '09 and
  Keynote '09. An architecture document and an implementation roadmap for
  Desk Writer in eleven phases followed.
- The apps were named Desk Writer, Desk Tables and Desk Slides. The
  working name "Gnome Desk" was dropped the same day.
- The licences were chosen (pull request 1), the Cargo workspace and the
  continuous integration were created (pull request 2), and the namespace
  of the ODF extensions was fixed.
- A reference virtual machine was built on the owner's PC ("Defiant"):
  macOS Mojave with iWork '09 updated to 9.3, driven over SSH and
  AppleScript.

### 31 October: development starts, icons, logo, website

- Phase 1 of Writer in one day (pull requests 3 to 10): the document model
  with undo and redo, the text engine, pagination, PDF output, the ODT
  reader and writer, validation of the written ODF against the schemas,
  and fuzzing of the reader.
- A proof of concept decided the text engine (pull request 5).
- Application and document icons and the suite logo, in the Adwaita style
  and inspired by the iWork '09 originals (pull request 4).
- The landing page and the README, in English with a Brazilian Portuguese
  version, published with GitHub Pages at <https://brahm.github.io/desk/>
  (pull request 11).
- The owner set the distribution: Flatpak from the project's own
  repository hosted on GitHub, targeting the newest GNOME 51.
- First layout parity: twelve reference documents made in Pages '09 lay
  out the same in Desk (pull request 12), and the `.pages` reader began
  (pull request 13).

### 1 to 4 November: Writer phases 2 to 5

- **Phase 2, Pages documents** (pull requests 13 to 20): the reader of
  Pages '09 files, free substitutes for missing fonts, a log of what an
  import leaves out, one Pages document for each row of the parity matrix
  (114 documents), and validation of the ODT written from each of them.
  The older Pages '08 format was dropped.
- **Phase 3, the editor** (pull requests 21 to 24): open, edit and save;
  the format bar, the Inspector and invisibles; screen reader support, the
  Portuguese translation and a nightly Flatpak.
- **Phase 4, text** (pull requests 25 to 30): tab stops and rulers,
  character and paragraph effects, styles, lists, Find & Replace,
  auto-correction, hyphenation, spelling and the proofreader.
- **The first gap review.** Before phase 5 the owner asked to make sure
  nothing had been left behind without a later phase to do it, naming
  shadows in ODF as an example. The review found 23 items, 7 of them ODF
  extensions; they were closed in pull requests 31 to 33.
- **Phase 5, structure** (pull requests 34 to 48): sections, columns and
  facing pages; headers, footers and fields; footnotes and endnotes; table
  of contents, hyperlinks and bookmarks; outline mode, thumbnails and page
  modes. Three more gap reviews ran inside the phase.
- The Flatpak repository got its signed nightly and beta channels, and the
  first beta, 0.5.0, was published on 4 November.
- A side question from the owner (how many languages GNOME 51 supports)
  became the language policy and pull request 50.

### 4 to 9 November: Writer phase 6, objects

- More than forty pull requests (49 to 95): shapes, text boxes, pictures and
  groups; selection, move, resize, rotate and arrange; linked text boxes;
  text in shapes; inline objects; gradients, line ends, shadows and
  reflections; image fills, masks, Instant Alpha and Adjust Image;
  parametric shapes, the pen and connection lines; movies and sounds; the
  Media Browser; masters and background objects; page layout documents;
  and the same objects read from Pages files.
- From 7 November the gap reviews ran in a thread of their own. They found
  that Tables and Slides had no roadmap (written on 9 November), that no
  object was being compared with the PDF that Pages produces, and that a
  long real document came out with 72 pages in Desk against 62 in Pages.
- Closing that last difference took four findings: the line metrics of
  each face of Helvetica Neue (pull request 87), blank pages at section
  starts (pull request 88), the justification rule of Pages (pull request
  91) and a metric-compatible substitute font, Desk Neue (pull request
  95). At the end every compared line of every reference document was in
  the same place as in Pages.

### 9 to 13 November: review before phase 7, and a new machine

- On 9 November the owner asked what was missing for full parity of
  Writer: phases 7 to 11, 146 of 304 rows, estimated at 16 to 26 days.
- A survey before phase 7 listed six points to solve and two decisions for
  the owner: build a second virtual machine to see the interface of iWork,
  and whether to read the older iWork formats (left out of scope).
- On 11 November the owner reinstalled Defiant, moving from Ubuntu 26.04
  to Fedora 45. The project folder, the worktrees and the virtual machine
  were checked and repaired, and the build environment was rebuilt.
- On 13 November the second virtual machine was ready: OS X Lion 10.7.5
  with iWork 9.3, which draws the documents on screen. With it the
  interface of Desk was compared with Pages 4.3, and metric-compatible
  substitutes were made for the 41 fonts of the Pages templates (pull
  requests 97 to 99). The second beta, 0.6.0, was published.

### 13 and 14 November: the interface question

- The owner asked where Pages conflicts with the GNOME Human Interface
  Guidelines and for a middle way where they diverge a lot. The answer was
  a document with 24 conflicts, 7 of them structural, each with a proposed
  middle way. His answers that day are "the decisions of 13 November".
- He ordered the project moved to the newest stable libraries: GTK 4.24,
  Libadwaita 1.10, Rust 1.100 and the GNOME 51 runtime (pull request 100,
  which also applied the interface plan to Writer).
- A sixth gap review (pull request 101) aligned the matrices and the
  roadmaps with those decisions and settled the shortcut rule. The third
  beta, 0.7.0, was published on 14 November.
- On 14 November the owner asked for a new inventory of features and
  interface items from the manuals and from what the Lion machine shows,
  and for mockups of three versions of the window for the three apps. The
  inventory read the resources of the apps and placed 933 items of Pages
  4.3, Numbers 2.3 and Keynote 5.3 in Desk. The mockups were drawn with
  the real GTK and Libadwaita widgets. **Version B was chosen**, and the
  plan of the whole suite was rewritten on it (pull requests 102 and 103).
- The minimum screen was set to 1280 × 720 (pull request 105).

### 14 to 16 November: interface B, Tables, Slides, Writer phase 7

- Writer moved to the interface B window in seven steps (pull requests
  104, 106 to 111): the window skeleton, the header bar and menus, the
  slim format bar, the two-level Inspector, the sidebar pages, the bottom
  bar, and the commands and shortcuts found by the inventory.
- The ten names and shortcuts proposed by the plan were approved on
  15 November (pull request 112), and the two gaps left by the migration,
  object styles and the Text tab of shapes, were closed (pull request
  113).
- On 15 November the owner set a standing rule: tell him whenever a task
  is left undone or moved to a later phase.
- Three tracks ran in parallel and landed on 16 November: the Desk Slides
  window (pull request 114), the Desk Tables window (pull request 115) and
  phase 7 of Writer, tables and charts (pull request 116, with 56 of its
  76 matrix rows tested).

### 16 November: comparison with the current iWork, and the new target

- In the morning the owner asked for an inventory of the newest Pages and
  of Pages '09 from manuals and documentation, a comparison between them,
  and then the same for Keynote and Numbers, marking for each feature
  which version is better, which is closer to the GNOME guidelines, and
  which features need an online service, AI or a paid subscription.
- He then asked what Desk already had by the rule "the better version of
  each feature, the newer one on a tie", and how long full parity would
  take without the online, AI and paid features.
- The pending checks were made on a real Mac with iWork 15.4.
- In the afternoon he rewrote the goals of the project. The replanning
  (pull request 117) recorded nineteen decisions, a new inventory of 566
  features, three window proposals with M+ chosen, a catalogue of ODF
  extensions, the architecture with diagrams, a roadmap of nineteen
  phases, a register of what is left undone, and guides for contributors
  and their coding agents.
- He asked for a progress dashboard kept up to date and published on the
  website (pull request 118), and for this log.
- He asked for a cleanup of the repository and of the project folder on
  Defiant, and approved the list of candidates. On the PC: five worktrees
  and eleven local branches whose work is on `main`, the two installer
  images of the virtual machines, the resources taken from the iWork '09
  apps for the inventory of 14 November, and old captures, about 7.5 GB.
  In the repository: the text engine experiment and 28 remote branches
  (pull request 121). Both virtual machines stay: the readers of Numbers '09 and
  Keynote '09 still need documents made in iWork '09, and D-020 keeps the
  Lion machine as the reference for the features only iWork '09 has.
  Left undone and moved to phase B1: three reference documents that carry
  media from Apple software are remade there (backlog item B-010).
  The owner then approved the last two items, the Compressor and Perian
  installers downloaded on 31 October for a video export that never
  worked in the virtual machine, and the merge of pull requests 120 and
  121. The project folder on Defiant went from about 50 GB to 42 GB.

### 16 November: the README and the website for the new goals

- The owner asked for the README and the landing page to be rewritten
  for the new goals, in a new order: what Desk is and what each app does,
  shown with the mockups of the chosen M+ window (each page in its own
  language); his story and motivation; a short status; a high-level
  roadmap; how to help, with a link to the progress dashboard. He also
  asked for the decisions of the architecture to be presented, such as
  the Rust engine kept apart from the GTK 4 and Libadwaita interface so
  that other projects can reuse it, and for a `HISTORY.md` at the root of
  the repository, in the style of the history file of Textpattern.
- His motivation, in his words paraphrased: Desk did not come from a lack
  of office suites on Linux, since LibreOffice and ONLYOFFICE do that job
  well, but neither follows the GNOME guidelines or feels at home in
  GNOME. He prefers simple tools that are no less powerful, so he went in
  the direction of iWork rather than of Microsoft 365, where LibreOffice
  tends today. He never got over the move of iWork from 32 to 64 bits,
  which broke his files and took away features he used daily; the current
  versions brought many of them back, some behind a paywall. **Desk is not
  a clone of iWork**: it is a suite of simpler, focused apps perfectly
  integrated with GNOME, and Desk Tables does not try to be a business
  intelligence tool or a database.
- The website and the README were rewritten on those lines, with a new
  section on the architecture (engine apart from the interface, MPL file
  format libraries, one document model with reversible operations, ODF
  first, native widgets, Flatpak on GNOME 51 and nothing online, checking
  against iWork itself). The roadmap is shown by track (B1, Writer,
  Tables, Slides, X1) with the forecast of parity near the end of January
  2027. The Portuguese page uses the Portuguese mockups, kept in
  `site/pt-BR/img` as part of the translation of the site that D-018
  keeps; they were checked against the English ones and needed no new
  drawing. The description of the Writer metainfo was updated too.
- `HISTORY.md` was created with the whole history of the project by
  release, one line per change, and joined this log, the dashboard and
  the backlog among the documents kept up to date at the end of each set
  of tasks (`AGENTS.md`).
- Backlog item B-007 was closed. Left undone and moved to phase B1: the
  screenshots of the Writer metainfo, which must be captures of the real
  app in the M+ window (B-011).

### 16 November: phase B1 starts, with the ODF extensions defined

- The owner approved starting phase B1, the common base, because the three
  app tracks depend on it. The replanning (phase R) was closed: its
  backlog rows had all moved to B1 or been settled. B1 is planned as a
  series of small pull requests that can each be reviewed on its own: the
  ODF extensions, the 20 rows left from Writer phase 7, the reference
  documents with Apple media, the material translated to English, the M+
  window in the three apps, automatic saving with versions, and the
  secondary scenes and screenshots.
- First of them: the 43 `desk:` extensions that Desk already writes are
  now each defined in `docs/odf-extensions.md`, with the element they sit
  on, the form of their value and its range, what the standard part of the
  document says in their place, and the matrix rows they serve (backlog
  B-008). Writing them down from the code, rather than from memory, showed
  that every one already had a standard fallback except the purely visual
  ones (tint, reflection, Instant Alpha, raised bullets), which other
  programs simply do not show.
- Why a check came with it: rule 5 of the catalogue says each extension is
  documented before it is merged, but nothing enforced it, which is how 43
  of them accumulated with one sentence each. `ci/check-extensions.py`
  compares the names used in `crates/desk-odf/src` with the headings of the
  catalogue and fails on either side's leftovers; it runs in
  `ci/check-all.sh` and in the CI.

### 16 November: the rows left from Writer phase 7

- Backlog item B-001 listed the 20 rows of Writer phase 7 (tables and
  charts) that had no tests. Each was read against the code. Six were
  implemented and only lacked tests: the look of the parts of a table
  (TAB-04), selecting cells across merged cells (TAB-07), moving among
  cells with the arrows, Tab and Return (TAB-08), the text style of a chart
  (GRF-08), formulas that name headers (IOP-15) and cells copied and
  pasted as text (IOP-19). They now have tests, and Writer phase 7 has 62
  of its 76 rows tested.
- Why some code moved: selection and navigation lived inside the GTK
  editor of Writer, where no test can reach them, and Desk Tables and
  Desk Slides will need the same rules. They are now pure functions of
  `desk-table` (`Table::selection_area`, `Table::step`, `Table::area_text`,
  `Table::paste_text`), which the Writer editor calls. The comments of the
  editor that cited the wrong matrix rows (the ids of an older numbering)
  were corrected.
- The other 14 rows need features that belong to later phases, and moved
  there with the owner told: pushing objects aside, dragging cells,
  managing custom formats and Copy/Paste Table Style to W1 (B-012); text
  of one chart element to W4 (B-013); live links and copying among the
  apps to X1 (B-014); cell controls and formulas that cite another table
  to T2 (B-015); filters and categories to T3 (B-016); a Writer chart in
  Tables to T4 (B-017); the animation of tables and charts from Tables to
  S3 (B-018). The reason in each case is that the feature either does not
  exist yet or needs another app to read and write tables first.

### 16 November: the matrices in English and for window M+

- The three parity matrices, the text of the parity report, the titles and
  descriptions of the corpus cases and the feature names of the manifest
  of Pages documents were translated to English (part of B-009, decision
  D-018). Ids, phases and page references did not change. The text inside
  the case documents stays in Portuguese, because the Pages references
  were made from it and changing it would mean making them again.
- Why a fix to the CI went into PR #124: the name of the new extension
  check, "Check that every desk: extension is defined", had a colon that
  made GitHub read the whole workflow file as invalid, so no job ran on
  the PR. The step was renamed.
- The `gnome` column of the matrices described interface B and path L,
  both replaced on 16 November. It now describes window M+ (D-008): the
  six object buttons in the header bar beside Insert, documents in tabs,
  no format bar (its controls are in the Format pane), the zoom and the
  word count overlaid on the document, the sheets of Tables in a switcher
  of the bottom bar, and automatic saving with versions in place of the
  recovery draft. The rows about the old bottom bar (GNM-13, KGNM-14,
  NGNM-13) were rewritten for what replaces it. `corpus/README.md` now
  says the target is the current iWork and that these matrices come from
  iWork '09 (part of B-006).
- Consequence: rows whose `gnome` text changed are not met until the
  window is rebuilt, later in B1; the UI tests still check interface B and
  change with it.
- What is left of B-006 and B-009 is `docs/interface` (its README, the
  two inventories and the mockup scripts), which will be rewritten in
  English for M+ in one go rather than translated first and rewritten
  after. Both items stay open in B1.

### 16 November: docs/interface in English and for window M+

- `docs/interface/README.md` was rewritten in English for window M+
  (D-008): the regions of the window, the sidebar, the two-level
  Inspector, the menus (the main menu takes View and loses Save), the
  breakpoints, the small screen notice, the shortcuts under D-017 with the
  iWork 15.4 column where it differs from '09, and the five steps that move
  the apps from interface B to M+ in B1. Interface B is kept as history.
  B-006 and B-009 are closed.
- The mockup scripts are now written in English in the source, ids
  included (`cab.inserir.tabela` became `head.insert.table`). Before, the
  scenes were written in Portuguese and `english.py` translated them while
  they loaded. Why the direction was turned rather than the translator
  just kept: D-018 puts everything in the repository in English, and the
  Portuguese pictures are needed only for the Portuguese page of the site,
  which is a translation. So `translate.py` and `mockups/pt_BR.tsv` now draw
  the Portuguese version. Every English picture came out the same as
  before, except the secondary scenes of interface B, which had stayed in
  Portuguese and are now English, and every Portuguese picture came out the
  same as before.
- Three Portuguese words had two translations in the same function of a
  scene (Nenhum and Nenhuma, Agrupar and Grupo, Filtrar and Filtro), so a
  table keyed by the English text can give only one of them there; none of
  those spots is visible in a picture, which is why the Portuguese pictures
  still came out the same. Elsewhere a context column (the module or the
  function) chooses between the two.
- `components.tsv` is now the inventory of the M+ scenes (101 components),
  and four secondary M+ scenes were added (selected image, Document, dark
  style, Data and Animation) so that the Inspector levels are in it. The
  state of each component is now measured against the Desk of today, with
  interface B built, instead of the Desk Writer of 14 November: 69 exist,
  11 change and 21 are new. Five components that only M+ has entered the
  catalogue (Open documents, the document tabs, the sheet switcher, its +
  button, and the objects of the sheet in the sidebar).
- `iwork09-to-desk.tsv` (renamed from `iwork-para-desk.tsv`) gives each of
  the 933 items of iWork '09 its place in M+: what was in the format bar is
  now in the Format pane of the Inspector, so the three commands that show
  and hide the format bar and the toolbar item for it are out of scope (66
  items out, 124 the same, 725 adapted, 18 left to the system).
- Consequence: the popovers and dialogs (Fonts, Colors, Insert, Search
  Commands, Preferences, the template chooser) are still drawn only in the
  scenes of interface B, so they are not yet in `components.tsv`. That is
  B-003, which stays open in B1. The pictures of interface B in
  `docs/interface/janelas` were removed; they are in the history of the
  repository.
- Open question for the owner, found while writing the README: the
  document tabs of M+ need a shortcut to switch, and GNOME uses
  Ctrl+PageUp and Ctrl+PageDown, which Tables uses for the sheets.

### 16 November: the secondary scenes of M+

- Twelve more scenes of M+ were drawn (backlog item B-003, done): the
  Insert menu, the Shape popover, the main menu, Fonts and Colors, the find
  bar with the banner of import warnings, Search Commands, Preferences,
  Keyboard Shortcuts, the template chooser, the window at half the minimum
  screen (640 × 720) and the small screen notice (1024 × 600). With the
  five drawn before, their pictures are in `docs/interface/scenes`, and
  `components.tsv` now has 138 components: 90 exist, 19 change, 29 are new.
- Fonts and Colors open from the Family and Text color rows of the Format
  pane, since M+ has no format bar. The popover is attached to the row
  after the scene is built, so the main pictures did not change.
- There is no close dialog in M+. *Why:* saving is automatic (D-001), so
  closing a document has nothing to ask; `dialog.close` stays in the
  catalogue as a piece of interface B only.
- At half the screen the object buttons go into Insert, as the README
  already said, and Open documents also leaves the header bar, because the
  title had no room; the tabs still switch documents. The word count goes
  above the zoom control, since the two overlapped. This was a choice of
  layout made while drawing, written into the breakpoint table of the
  README; the owner can change it.
- Some rows of the catalogue still described interface B (one window per
  document, the formula bar under the format bar, the bottom bar of the
  three apps); they now describe M+ and say what B had.
- `ci/setup-env.sh` now installs `python3-gi-cairo`: without it the
  mockups drew the document area empty, because the page is drawn with
  cairo from Python.

### 16 November: three fixtures made again without Apple media

- `lst-03-bullets`, `obj-18-audio-video` and `obj-19-movie-settings` held
  files that came with Apple software (the Pages icon used as the default
  image bullet, the Glass alert sound, the Sunset sample movie), which
  rule 3 of D-011 keeps out of the repository. They were made again in
  Pages 4.3 (iWork '09 9.3) on the reference Mojave virtual machine, with
  media generated by the project: a 20 s test pattern movie (ffmpeg
  `testsrc2`, 640 × 360), a 2 s tone at 880 Hz, and the Desk Writer icon as
  the image bullet. The PDFs were exported again. Backlog item B-010 is
  done.
- Pages '09 copies its own speaker icon, `AudioOnlyMovie@2x.tiff`, into
  every document with a sound. *Decision of the owner:* replace it with a
  picture the project draws, of the same name, size and mode, rather than
  remove the file or keep it as an exception to D-011. *Why:* the package
  stays as Pages writes it, and no Apple image is kept. The picture is made
  by `corpus/fixtures/media/make-audio-only-image.py`. The exported PDFs do
  not draw that icon, which was checked.
- *Consequences:* the image bullet of LST-03 falls exactly where the old
  one did (a 12 point square), so no expectation moved; the movie of
  OBJ-18 is now 16:9, and the comparison with the new PDF passes as it is;
  the poster time of OBJ-19 is now 16049 of the 1000 units a second of the
  new movie, where the test expected 9573 from the old one. The script that
  drives the virtual machine to make these fixtures uses the new media;
  choosing the picture of the image bullet and replacing the speaker icon
  are still done by hand after it.

### 16 November: the shortcuts of the document tabs (D-021)

- The open question of the README was answered by the owner: follow GNOME
  in every shortcut for an action like Desk's, whenever it does not
  conflict (D-021, which confirms D-017).
- *Consequence:* Writer and Slides switch documents with Ctrl+PageUp and
  Ctrl+PageDown, as GNOME does. In Tables that would conflict with the
  sheets, which keep Ctrl+PageUp and Ctrl+PageDown, so the documents there
  use Ctrl+Tab and Ctrl+Shift+Tab, which `AdwTabView` also accepts. Alt+1 to
  Alt+9 go to a tab in the three apps. Ctrl+Home and Ctrl+End, which the
  tab view would take for the first and last tab, are turned off there,
  because they move to the start and end of the text and of the table.

### 16 November: Desk is for GNOME users first (D-022)

- The owner set the direction of the product: Desk is made for users of
  the GNOME desktop, used to its workflow and its shortcuts. It aims to be
  familiar to those who come from iWork, the owner included, but where the
  two pull apart, GNOME wins. *Why:* once the 1.0 versions of the three
  apps are done, Desk will grow new features of its own and an ever closer
  integration with the GNOME desktop and its applications, so compatibility
  with GNOME is what lasts. Recorded as D-022 in `docs/decisions.md`, in
  `docs/goals.md` (Who Desk is for) and in `AGENTS.md`.
- *Consequences, reviewed the same day:*
  - D-021, an hour old, kept Ctrl+PageUp and Ctrl+PageDown for the sheets
    of Tables, against GNOME's use of those keys for tabs. It is replaced
    by D-023: the document tabs take GNOME's shortcuts in the three apps,
    and the sheets move to Alt+PageUp and Alt+PageDown, by rule 3 of D-017
    (the command of iWork that collides gets a new shortcut).
  - Fonts had Ctrl+T, as in iWork, which GNOME gives to a new tab. The
    owner chose that Fonts has no shortcut of its own (D-024): Ctrl+T opens
    a new document in a tab, and Fonts opens from the Font row of the
    Format pane and from Search Commands, as in the apps of GNOME, which
    have no shortcut for fonts.
  - The rest of the shortcut table already put GNOME first (D-017), and
    nothing else in it collides with a standard shortcut of GNOME.

### 16 November: step 1 of window M+, the document tabs

- The three apps open their documents as tabs of one window
  (AdwTabView), with a tab bar that shows from the second tab and the tab
  overview behind the new Open documents button of the header bar. Ctrl+N
  and Ctrl+T open a new document in a tab, Ctrl+W closes the tab and
  Ctrl+Shift+W the window; New Window still opens a window of its own.
  Rows W013, T016 and S013 (several documents) are done.
- The Sidebar button only shows and hides the sidebar. View moved to a
  submenu of the main menu, and Save left the main menu for the More menu,
  since saving is automatic in M+ (step 5).
- *How, and why:* each document keeps its own actions, and the window
  forwards its actions to the document of the selected tab. GTK finds the
  application's shortcuts and the D-Bus actions only in the window's own
  action map, so actions set on each tab would not answer either; the
  forwarding keeps both working. The breakpoints of the narrow window moved
  from the window to each document, since a window applies only one set.
- *Consequences:* the window's title is the name of the document shown,
  so the title of Tables no longer ends in "Desk Tables". The UI tests
  follow the new shortcuts (D-023, D-024) and the Open documents button.
  The matrix rows of the menus (GNM-10, NGNM-10, KGNM-10) were brought in
  line with M+, where View is in the main menu.

### 16 November: step 2 of window M+, the Format pane

- The format bar is gone from the three apps. Its controls moved to the
  Format pane of the Inspector, as in the iWork of today: the paragraph
  style sits over the tabs; the Style tab of text has the Font group
  (family, size, bold, italic, underline, strikethrough, text color and
  character style), alignment, spacing, lists and shadow; the More tab
  has the links; the Arrange tab has Bring to Front, Send to Back and a
  menu to align, distribute and flip. The Format Bar item and its
  shortcut (Ctrl+Shift+R) are gone.
- Slides shows a Slide tab when nothing is selected (skip the slide; the
  master, background and transition rows are placeholders until the
  phases that make them). Tables shows Cell and Text tabs in place of its
  two format bars.
- The color rows open a new Colors popover in place of the Colors window
  of iWork: the palette of the GNOME guidelines, the eyedropper and
  Custom…, which opens the color chooser of GTK. Libadwaita has no color
  popover, so it is a custom widget, recorded in the component table.
  Fonts and Colors open the Format pane first, on the tab of the
  selection, and then their popover on its row.
- *Why:* M+ moves every format control to the pane (INT-16), and with
  D-022 the pane follows the look of GNOME's own settings: groups of
  rows, a popover instead of a floating window.
- *Left for later:* the row of the document's own colors in the Colors
  popover, and the Typeface row and the advanced options of the Font
  group, are in the backlog as B-019, still in phase B1.
- *Consequences:* the UI tests open the Inspector before they look for a
  control that the format bar used to show, and the command search finds
  the Format menu and the Objects menu of the pane.

### 17 November: step 3 of window M+, the controls over the document

- Writer and Slides lost their bottom bar. The zoom sits in a pill of
  GNOME's osd style over the end corner of the document, between Zoom Out
  and Zoom In, with its menu on the percentage; the count of Writer sits
  in a pill over the foot of the document, with the same menu as before.
  In a window narrower than 720 px the count moves over the zoom. In the
  light table of Slides the pill holds the size of the slides instead.
- Go to Page (Go to Slide) moved to the View menu, as a field with "of
  41" after it, with the previous and the next place, named after what
  Navigate By chose ("Next Section"), and Navigate By as a submenu.
- *Why:* M+ has no bottom bar (GNM-13, KGNM-14), and the matrices
  already put Go to Page and Navigate By in the View menu (INT-19). The
  position "Page 6 of 41" had no place left on the screen; the field of
  Go to Page shows the page in sight when it opens.
- *Consequences:* the arrows of the bar are gone, so the previous and the
  next place are items of the View menu. The UI test of the bottom bar
  became that of the controls, and opens Go to Page by the mnemonics of
  the menu.

### 17 November: step 4 of window M+, the bottom bar of Tables

- The sheets of Desk Tables left the sidebar for a switcher in the bottom
  bar, as the tabs of Numbers 15: a toggle group of Libadwaita, one toggle
  per sheet, with the + button before it, which adds a sheet from the
  same menu as before. A secondary click on a sheet opens its menu,
  whose Move items now say Left and Right. The instant calculations and
  the count of the cells stay at the end of the bar.
- The sidebar now shows only the tables and charts of the sheet in
  view, under "In “Sheet 1”", with "Tables and Charts" as its heading
  and Add Table or Chart as its + button.
- The zoom left the bar and floats over the sheet in the same pill as in
  Writer and Slides; the pill is now one shared function.
- *Why:* the M+ mockup puts the sheets in the bottom bar (NGNM-13) and the
  objects of the sheet in the sidebar (NGNM-12), as Numbers 15 does; a
  sidebar with every sheet and every table grew long, while the tabs keep
  the sheets in sight beside the cells.
- *Consequences:* the position "Sheet 1 (1 of 2)" is no longer a label of
  the bar; each toggle carries it as its tooltip and its description, so
  a screen reader still hears it. The UI test reads the sheet in view
  from the switcher, and chooses a sheet through the action of its
  toggle.

### 17 November: step 5 of window M+, automatic saving and versions

- A document of Desk Writer with an ODT file saves itself two seconds
  after the editing pauses, and when its tab closes, without asking; its
  title no longer shows unsaved changes. Ctrl+S saves at once and keeps a
  version, as Cmd-S of iWork 15 does.
- Versions are kept the first time the document saves after it opens
  (the file as it was opened), every hour while it is edited, with
  Ctrl+S, and before an older version takes the place of the document.
  Old ones are thinned as macOS thins them: all of the last day, one a
  day for a month, one a week after.
- Browse Versions, in the main menu after Save As, lists them by day.
  Each can be restored, which keeps the current state as a version first,
  or opened as an untitled copy beside the document, to take parts from
  it. Revert to Saved became Revert to Last Opened, in the command
  search.
- New documents, and files of another kind such as Pages documents, have
  no file to save themselves to: they keep the recovery draft of path L
  and the question on closing until Save As.
- *Decision D-025:* the owner chose to keep the versions in the data
  folder of Desk (`~/.local/share/desk/versions`), outside the file.
- *Why:* iWork 15 saves by itself and keeps versions (D-001, DOC-11).
  macOS keeps them outside the file, and so does Desk: the ODF file stays
  standard and the same size, and nothing of its past goes along when it
  is sent. GNOME has no timeline of versions, so they are a list of
  Libadwaita rows.
- *Consequences:* the versions do not follow a file copied to another
  computer, as on macOS. They follow a rename made in Desk. The UI tests
  that save with Ctrl+S still pass, since Ctrl+S still writes the file.
  Desk Tables and Desk Slides do not save files yet; they take the same
  saving when they do (phase 1 of each).

### 17 November: the reference cases redone on Pages 15.4

- The 13 cases of `corpus/writer` have their `reference.pdf` exported
  again, by Pages 15.4 on the owner's Mac (macOS 26.7), from the same
  `case.pages` files that Pages '09 built. Pages 15.4 opened them without
  converting them and with the Liberation fonts installed. The work was
  done by controlling the screen of the Mac from the project, after a
  session on the Mac could not reach it.
- Against the new references, 9 cases were the same and 4 differed. Each
  difference is a change of Pages 15.4, and Desk now follows it:
  - *Line spacing in multiples (FMT-14):* Pages 15.4 scales the line
    with the leading under the text as the font has it and does not
    round: double spacing of Liberation Serif at 12 points is 29.49
    points, where Pages '09 made it 29.
  - *The bottom of the page (EST-05, INT-02):* a line fits on a page only
    when all of it fits, the leading below its text included; Pages '09
    let that leading stick out. The case of text across pages now says
    that its paragraphs have widow control, as the Pages document has,
    since the difference only shows with it.
  - *Title Case (FMT-07):* a document of '09 with Title capitals opens in
    Pages 15.4 with Title Case, which leaves the short words that join
    others in lower case ("Inicial de Cada Palavra em Maiúscula"). Pages
    tells those words by the grammar of the sentence, as two sentences
    tried on the Mac showed; Desk uses lists of articles, short
    prepositions and conjunctions of Portuguese and English, by the
    language of the text.
  - *Small caps (FMT-07):* Pages 15.4 keeps the spaces at the size of the
    text; Pages '09 made them small too.
  - *Ligatures in the PDF (DOC-05):* Pages 15.4 draws a ligature apart,
    in a span that names its letters, and Poppler reads it out of place,
    which split "officia" in two. The comparison puts the letters back
    into their word; the layout of Desk was already the same.
- *Why:* the target is the current iWork (D-001); the cases measured
  Desk against Pages '09 until now.
- *Consequences:* all 13 cases match Pages 15.4. Of the documents of
  `corpus/fixtures/pages`, with the PDFs of Pages '09, only the one of
  capitals disagreed, on the spaces of small caps: its PDF was exported
  again by Pages 15.4. Two tests of the text engine held the multiples
  that Pages '09 rounded; they now hold what Pages 15.4 laid out on the
  Mac with Liberation Sans and Helvetica Neue Light. The other documents
  stay on Pages '09 for now; whether to export all their PDFs again on
  Pages 15.4 is an open question for the owner.
- The check of advisories found a new one in the zlib that the font
  library builds with (RUSTSEC-2026-0544); `libz-sys` went from 1.1.31
  to 1.1.33, which fixes it.

### 17 November: the PDFs of the Pages documents redone on Pages 15.4

- The owner answered the open question of the last entry: export the
  PDFs of every document of `corpus/fixtures/pages` again on Pages 15.4.
  The 136 documents besides `fmt-07-capitalization`, which already had
  one, were exported on the owner's Mac by controlling its screen, in
  about an hour, with File > Export To > PDF and the options Pages
  offers. One document, `mal-01-07-08-fields`, brought up a system alert
  in front of Pages; the export went on around it, without granting
  anything.
- *What the comparison found:* with the new PDFs, 28 tests of
  `desk-testkit` failed. Two causes were in the comparison, not in Desk:
  - *Ligatures at the start or the end of a word:* the fix of the last
    entry only put back a ligature inside a word, by guessing where its
    line starts. Poppler loses the move to the start of the run, which
    is the origin of the line, of the text box or of the list item, so
    the guess failed in lists and text boxes. The tests now read such a
    PDF after Cairo writes it again (`pdftocairo -pdf`), which puts each
    glyph in its place with its letters. That fixed ten documents.
  - *A word in two runs (EST-16):* Pages 15.4 draws the text around a
    page number field in separate runs that touch, which Poppler reads as
    separate words ("sectio", "5", "n"). Words whose boxes touch on the
    same line are now one word, as Desk's own PDF has them.
- *Where Pages 15.4 differs and Desk keeps the PDF of '09:*
  - Features that only Pages '09 has (D-020): the outline (six
    documents, which Pages 15.4 prints as body text); Never Hyphenate,
    which lives in the hyphenation exceptions of the '09 machine and not
    in the document, so Pages 15.4 hyphenates the word; and a date field
    that updates on open, whose PDF of 15.4 holds the day of the export.
  - Behaviour of Pages 15.4 that Desk does not follow yet, each moved to
    phase W1 with its row of the backlog: lines placed without rounding
    the top margin and the ascent, so glyphs sit a fraction of a point
    higher (B-020, the pixel tests of two documents); the metrics of
    Baskerville and Didot on the current macOS (B-021); the table of
    contents as Pages 15.4 rebuilds it (B-022); justification and
    hyphenation (B-023); text inside an oval (B-024); and the box of an
    inline sound (B-025).
  - Leaving the rounding out of the layout was tried: it broke 35 tests,
    so the rule of 15.4 is not that simple and has to be measured.
- *Why:* the target is the current iWork (D-001), and parity documents
  are made on Pages 15.4 except for what only '09 has (D-020).
- *Consequences:* 116 PDFs of the fixtures are now from Pages 15.4, and
  Desk matches all the documents that the tests compare with them. 20
  keep the PDF of Pages '09, 8 because the feature is only in '09 and 12
  until the backlog rows B-020 to B-025 are done; the manifest says so
  on each row. A test of the comparison reads `fmt-09-ligatures` and
  checks that its ligatures stay in their words.

### 17 November: the colors of the document and the advanced options of the Font group (B-019)

- The owner asked to do B-019 now, the part of step 2 of window M+ that
  had stayed in the backlog: the row of the document's own colors in the
  Colors popover, and the Typeface row and the Advanced Options of the
  Font group, as the scenes of M+ draw them.
- *Colors of the document:* the model gives the colors a document uses
  (`Document::colors`), each once, in the order they first appear: the
  color, background and custom line colors of its text, the backgrounds
  and rules of its paragraphs, in the body and in its objects, then the
  fills and lines of its objects. The popover asks for them each time it
  opens, since the document changes while it is closed, and shows up to
  two rows of the palette under it, as "From the Document". Each tab of
  the window has its own document, so the Inspector of the tab, not the
  window, says where the colors come from.
- *Typeface:* a row under Family lists the faces of the family, as the
  Fonts popover does, and chooses the face nearest to the weight and
  slant of the selection; choosing one sets both. Shapes and text boxes
  get it too, since they share the Font group.
- *Advanced Options:* an expander at the end of the Font group holds
  character spacing, the baseline (superscript and subscript) and its
  shift in points, capitalization, ligatures and the background of the
  characters. These rows moved from the Characters group, which keeps
  the outline and the language. Baseline, its shift and capitalization
  had no control in the interface before: only the model, the formats
  and the automatic superscript of ordinal suffixes set them.
- *Left out:* the None button that the scene draws beside Custom… is
  not part of B-019, and the text color always has a color; the rows
  whose color can be off keep their switch. The owner asked for a row of
  the backlog for it: B-026, still in phase B1.
- *Consequences:* B-019 is closed; components.tsv counts the colors of
  the document as existing (91 existing, 28 new). The UI tests open the
  Advanced Options and look for its rows, and check that the Colors
  popover shows the colors of the document.

### 17 November: None in the Colors popover (B-026)

- The owner asked for the None button of the Colors popover, which the
  scene of window M+ draws beside Custom…, first as a row of the backlog
  (B-026) and then to do it.
- *Where it shows:* only where the row can have no color. The rows with a
  switch (the backgrounds of characters and paragraphs, the shadow of
  the text, the colors of underline and strikethrough in the Fonts
  popover, the color of a list label) turn the switch off. In the Graphic
  group of objects, None on the fill takes the fill away, or only the
  tint of a tinted picture; on the line it chooses None in the Stroke
  list; on the shadow it turns the Shadow switch off. The text color, the
  line of a paragraph border, whose switch is hidden, and the end of a
  gradient always have a color, so their popovers have no None.
- *Why this way:* each row keeps one way to say "no color" (its switch or
  its list), and None only sets it, so undo, the model and the files are
  unchanged.
- *Consequences:* B-026 is closed; components.tsv counts None as existing
  (92 existing, 27 new). The UI test turns the background of the
  characters on, takes it away with None, and checks that the popover of
  the text color has no None.

### 17 November: Screenshots of the new window in the metainfo (B-011)

- The owner asked for the screenshots only, leaving the final validation
  on the Mac for later.
- *What was done:* four captures of Desk Writer in the M+ window replace
  the two old ones in data/screenshots: a report with the Format pane
  (the default), the Fonts popover opened from the Family row, Find in
  the header bar, and the same window in the dark style. Each has a
  caption in English and in Brazilian Portuguese. The description of the
  metainfo now says the window with tabs, the Format pane and automatic
  saving with versions are done, and that comments and change tracking,
  templates, Word files and printing are still to come.
- *How:* the app ran in the GNOME 51 chroot on an X server without a
  window manager, with the accent forced to blue, and was driven by its
  own actions over D-Bus and by the keyboard; the sample document was
  made for the purpose and has no Apple material.
- *Why there is no Colors capture:* without a window manager its popover
  falls outside the window, and a capture of half a popover is worse
  than none.
- *Consequences:* B-011 is closed. AppStream validation passes.

### 17 November: Final validation of B1 on the Mac

- The owner asked for the final validation of B1 on his Mac with iWork
  15.4, after the screenshots, and then for the checks left out of it,
  with the apps in front.
- *What was done:* the 32 rows of B1 that are done (15 of Writer, 8 of
  Tables, 9 of Slides) were compared with Pages, Numbers and Keynote
  15.4: their menus, toolbars, Format panes, settings, tabs, dark
  appearance, zoom and navigation, the name of the change in Undo, and
  what closing a new document asks. The verdicts are in
  corpus/mac-review.tsv: 18 rows the same, 10 in another form that a
  decision sets (mostly D-008, the GNOME window), and 4 with a gap.
- *The gaps:* Zoom to Selection, which the Zoom menu of the three apps
  has (and Auto-Center and Zoom to Fit Content in Keynote), became B-027;
  First Page and Last Page in the Go To menu of Pages became B-028; the
  name of the change in Undo and Redo, which Numbers shows and Desk
  Tables does not, became B-029. All three stay in B1.
- *The 21 rows not finished:* for each, the file records what iWork 15.4
  does (the table of contents in the sidebar, Screen View, the Organize
  pane, instant calculations, the Functions browser, Build Order, the
  Arrange and Adjust Image windows, the settings of Keynote), so that the
  second round, when Desk finishes them, only has to look at Desk.
- *How:* first with the apps in the background, so the owner could keep
  using the Mac; the name in Undo and the menus of the toolbar need the
  app in front, which the owner then allowed.
- *Why the captures are not in the repository:* they show Apple's
  interface, which D-011 keeps out; they are in the project files.

### 17 November: The gaps of the B1 validation (B-027 to B-029)

- The owner asked for the three gaps that the validation on the Mac found
  to be done in one pull request.
- *Zoom to Selection (B-027):* in the Zoom menus of the three apps, as
  Pages, Numbers and Keynote 15.4 have it. It zooms so that the selected
  objects, or else the selected text, or in Desk Tables the selected cells
  or table, fill the view, and puts them in its middle; with nothing
  selected it rings the bell. The zoom is free, not one of the levels,
  as in iWork, between 25 % and 400 %.
- *Zoom to Fit Content and Auto-Center (B-027):* in Desk Slides, as in
  Keynote. Fit Content zooms to the slide and to whatever of its objects
  reaches beyond it. Auto-Center keeps the slide in the middle of the
  view when the zoom changes; it is on by default, as in Keynote.
- *First Page and Last Page (B-028):* in the navigation of the View menu
  of Desk Writer, beside the previous and the next place, as in the Go To
  menu of Pages; they put the cursor at the start of the page, as Go to
  Page does.
- *Undo names the change in Desk Tables (B-029):* the context menu of a
  table now says Undo Rename Table or Redo Add Row Below, as Numbers
  does; the names of the changes of Desk Tables joined the list of names
  that Desk translates.
- *Consequences:* the three items are closed; the gaps in
  corpus/mac-review.tsv stay as the record of the validation.

### 17 November: The remaining rows of B1 (B-030 to B-038)

- The validation on the Mac left 21 rows of B1 not finished. The owner
  asked why; the answer was that nobody had worked on them yet, and that
  most looked like features of Desk Writer still to be connected to Desk
  Tables and Desk Slides. He agreed to do those and to move the seven
  that depend on the engines to later phases.
- *What reading the code changed:* the cells of Desk Tables have no
  content, editing or format yet, and its window inserts no pictures or
  objects. So four of its rows were not a matter of connecting: the
  Functions browser on typing "=" and the fonts and colors of the cells
  need the cells of T2, the Media button needs the objects of T4, and
  the warnings need the files of other apps of T5. Screen View in Desk
  Writer, the text in one flow without pages, changes the layout engine.
  The owner chose to move these five rows too rather than bring that work
  into B1, which would have added days before closing the phase.
- *The phases:* the first answer had named T4, T5 and S5 for the seven,
  after the old notes of the inventory, which used the numbers of the old
  phases. The roadmap gives each a better place, and they went there:
  - the Organize menu and pane (T001, T009) to T3, sort and filter (B-030);
  - the formula in the cell and the choice of instant calculations
    (T004, T008) to T2, cells and formulas (B-031);
  - table styles (T007) to T1, beside row T068 (B-032);
  - Build Order (S005) to S3, animation (B-033);
  - the Slideshow and Remote preferences (S015) to S4, presenting
    (B-034);
  - the Functions browser and the fonts and colors of the cells (T010,
    T012) to T2 (B-035);
  - the Media button of Desk Tables (T011) to T4 (B-036);
  - the warnings of Desk Tables (T013) to T5 (B-037);
  - Screen View (W012) to W1 (B-038).
- *Done in B1:*
  - The table of contents in the sidebar of Desk Writer (W011), as Pages
    15.4 has it: a page of the sidebar, also in the View menu, that lists
    the headings by level; a click goes to the heading, Edit opens the
    Table of Contents group of the Inspector and Insert puts a table in
    the text. It lists the styles that the tables of contents list, or
    the Heading styles when the document chose none.
  - Document Warnings in the View menu of Desk Writer and Desk Slides
    (W009, S010), as Show Warnings in Pages and Keynote. Replace Fonts
    was already in the Warnings window, one font at a time.
  - Preferences in the main menu of Desk Tables (T018), the same dialog
    as Desk Writer without automatic lists, the word count and the color
    of invisibles.
  - Checked with the app open: the Media button, the font pop-ups, Adjust
    Image and Arrange in Desk Slides (S006 to S009), which come from the
    window of Desk Writer, and Fullscreen in Desk Tables (T015). The test
    display has no window manager, so the screen of Fullscreen itself was
    read from the code, which is the same as in Desk Writer.
- *Consequences:* corpus/mac-review.tsv has a new verdict, "moved", for
  the twelve rows. With these nine, every row of the inventory that B1
  owns is done and no backlog item is open for B1, so the phase can be
  closed when the owner decides.

### 17 November: The review of the B1 rows marked "done, to review"

- The owner asked to check the real state of the B1 rows that the
  dashboard showed as done but to review, and whether any had a pending
  item or a problem.
- *What was found:* the 39 rows marked done in B1 all had a verdict in
  corpus/mac-review.tsv: 22 the same as iWork 15.4, 13 different by a
  decision, and 4 gaps (W010, T014, T019, S011) that pull request #140
  closed (B-027 to B-029). Their review mark had stayed because the
  validation did not clear it, so the mark was cleared, and mac-review.tsv
  says that pull request #140 closed the gaps.
- *What stays for later phases, and where:* some of these rows are done
  as a window but show controls that do nothing yet, because their engine
  comes in a later phase; each has its own row in the inventory, so none
  needed a backlog item:
  - in Desk Tables, the Chart, Text Box, Shape, Media and Comment buttons
    (T4, T2 and T3; the Media button is B-036), the controls of the Cell
    and Text tabs of the Format pane (T1 and T2) and the Data level of the
    Inspector (T3);
  - in Desk Slides, the master slide, background and transition of the
    Slide tab (S1 and S3), and the Animation level, which has what the
    old phase 2 gave (S3);
  - Document Warnings in Desk Slides opens only when a presentation has
    warnings, which come with the files of S5;
  - Fullscreen in Desk Tables was checked in the code, not on screen.

### 17 November: rules against distraction by notes in the code (D-026)

- The owner asked to adopt a set of rules against the distraction of
  coding agents, taken from Google's article "Agent distraction" and the
  kit those rules came from: an agent reads every `TODO` it meets and
  sometimes acts on it unasked; in Google's measures about 41% of those
  detours chased bugs already closed, and each cost about 70% more steps
  and 77% more tokens.
- *What was done:* `AGENTS.md` gained rules 12 ("Work lean, on the task")
  and 13 ("No stale notes"); `CONTRIBUTING.md` says how a note cites the
  backlog; `docs/backlog/README.md` gained rules 5 and 6.
  `ci/check-backlog.py` now also reads every file git tracks and fails on
  a `TODO`, `FIXME`, `XXX` or `HACK` that cites no row, a row that is not
  in the register, or a row that is no longer open. It already ran in
  `ci/check-all.sh`; it now runs in the CI as well, in a job of its own,
  which it did not before.
- *Old notes:* the repository had none. A search of every tracked text
  file found no `TODO`, `FIXME`, `XXX` or `HACK`, so nothing had to be
  turned into a backlog row or removed.
- *What is not scanned:* Markdown, the register itself and the script,
  which name the markers in prose.
- *Consequences:* a note can no longer outlive the work it names, and the
  register is the one place where pending work is written down.

### 17 November: B1 closed, W1 next

- The owner closed phase B1 once all its inventory rows were done, the
  Mac had checked them, and no backlog item was open for it
  (`ci/check-backlog.py --phase B1` passes).
- It took two days, 16 and 17 November, against the seven estimated. It
  moved 12 of its 53 rows to later phases (B-030 to B-038), so part of
  that difference went to them; the estimate of the other phases stays
  until each one ends.
- *Next:* asked which phase to start, the owner chose W1, Writer to the
  behaviour of 15, over T1 and S1: Writer has its whole engine and is the
  closest to a usable app, and W1 holds the most open backlog items (B-012,
  B-020 to B-025 and B-038).

### 18 November: phase W1 starts, with its plan

- The owner closed phase B1 and chose W1, Writer brought to the behaviour
  of Pages 15.4, as the next phase. Its first pull request is the plan the
  roadmap asks for when a phase starts,
  [docs/phases/W1.md](phases/W1.md): the 28 rows and 8 backlog items of
  the phase, the parity rows that test each one, what is missing, what has
  to be seen on the Mac, the ODF each needs, and the order of 19 small
  pull requests.
- *What reading the rows showed:* the table of contents in the sidebar
  (W041) was built in B1 with W011 and only has to be checked as live;
  four rows marked done (W047, W048, W073, W131) only need the check of
  the new target; 12 rows exist only in Pages 15 and have no parity row
  yet, so they are written from what Pages 15.4 does on the Mac.
- *Moved, by the owner's choice:* the review highlight (W059) to W2,
  which builds the comments it hangs on (B-039); bidirectional and
  vertical text (W071) and the right-to-left table (W139) to W6, where the
  roadmap already puts right-to-left and vertical text (B-040, B-041); the
  missing functions (W133) to T2, since `desk-formula` is shared and T2
  brings the functions of 15.4 (B-042). W1 keeps 24 rows.
- *How the Mac is used:* one session at the start, announced to the owner
  first, to see the features of the 15 and check the rows whose target
  changed, and one at the end. The layout items of the backlog (B-020,
  B-022 to B-025) are studied from the PDFs of Pages 15.4 already in the
  corpus, without the Mac; only the metrics of the Apple fonts (B-021) are
  measured there.

## Decisions, with their reasons and consequences

Ids `D-nnn` and `R-nnn` are those of [decisions.md](decisions.md).
Decisions older than that file have no id.

### Foundations (30 and 31 October)

**A shared base for the three apps, Writer first.**
*Why:* the cases the manuals describe across apps (a table or a chart of
Numbers inside Pages or Keynote) only work if the apps share the document
model, the text engine, tables, formulas and charts.
*Consequences:* the workspace is split into engine crates without GTK and
a thin interface layer. When Tables and Slides started on 15 November
they reused what Writer had; by the inventory of 16 November much of what
they lack reuses engines that already work in Writer.

**ODF as the native format, with documented extensions.** (Kept as D-009.)
*Why:* the owner wanted an open standard as the default format and noted
that ODF allows extensions for what it cannot hold.
*Consequences:* about a quarter of the Pages '09 features needed an
extension. The extensions live in one namespace, written files are
validated against the ODF schemas in the continuous integration, and
every gap review looked for features that were implemented but not saved.
On 16 November there were 43 extensions and 25 more features of the new
target that need one.

**The extension namespace is `https://brahm.com.br/ns/desk/1.0`.**
*Why:* a namespace URI must be unique, stable and versioned, and can never
change once documents exist. The owner first asked whether he could change
it later, since he needed time to set up a domain; the answer was that a
change is only free before the first public release, and that owning the
domain is enough, with no hosting needed. He then gave his own domain.
*Consequences:* the application ids follow the same domain
(`br.com.brahm.Desk.Writer`, `.Tables`, `.Slides`).

**No "GNOME" in the names.**
*Why:* the owner had not asked for permission to use the GNOME trademark.
*Consequences:* the suite is Desk, and the apps are Desk Writer, Desk
Tables and Desk Slides.

**GPL-3.0-or-later for the apps, MPL-2.0 for the format libraries.**
*Why:* the owner asked for the best free software licence for the project.
The repository had started with GPL-2.0, which, kept as "version 2 only",
is incompatible with the Apache-2.0 and GPL-3.0 libraries common in Rust.
GPL-3.0-or-later keeps the apps and their forks free. MPL-2.0, a
file-level copyleft, lets other programs link `desk-odf`, `desk-iwork`
and `desk-formula` to read and write these formats while changes to the
library files stay open.
*Consequences:* the format crates may not depend on any other Desk crate,
which became a layering rule of the architecture.

**A virtual machine with the real iWork as the reference.**
*Why:* parity of behaviour needs the original to compare with. The owner
holds the licences, none of his Macs can run a system old enough for
iWork '09, and he took responsibility for running macOS on a PC. He also
said the machine was temporary: needed only until Desk reached parity and
its ODF could save everything one to one.
*Consequences:* reference documents are made in Pages by script and
exported to PDF, and Desk's layout is compared with them line by line.
The first machine (Mojave) showed a blank document area and could not be
used to look at the interface; a first attempt at a Lion machine was
abandoned on 31 October; a patch made the Mojave one usable on 1 November;
and a working Lion machine was finally built on 13 November. The rule
that nothing of Apple's enters the repository (no screenshots of iWork,
no resources, no fonts) comes from this period and was made explicit in
D-011.

**The text engine builds on Parley, and Desk decides where lines break.**
*Why:* a proof of concept laid out a justified, hyphenated paragraph
wrapped around a circle in three ways. Parley's own line breaker left some
lines too narrow because it has no hyphenation; taking Parley's shaped
text and deciding the breaks in Desk gave exactly the lines of a
hand-written engine, with Parley still doing shaping, bidirectional text
and glyph placement.
*Consequences:* line breaking, hyphenation, wrapping around objects and
the line spacing rules of Pages are Desk's own code, which is what later
made it possible to match Pages line by line.

**Flatpak from the project's own repository, on the newest GNOME.**
(Kept as D-013.)
*Why:* the owner's decision on 31 October, replacing the Flathub release
the first plan assumed. On 13 November he added that Desk is officially
distributed only as a Flatpak on the GNOME 51 runtime, and that whoever
packages it for a distribution must meet the requirements or fork.
*Consequences:* a signed Flatpak repository on GitHub Pages with nightly
and beta channels; the freedom to require GTK 4.24 and Libadwaita 1.10;
building outside Flatpak needs Fedora 45, Ubuntu 26.10 or Arch Linux.

### Scope of compatibility (1 to 9 November)

**Only iWork '09 files; Pages '08 and older are out.**
*Why:* the target was iWork '09 alone, and the reader had no code that
existed only for the '08 format (pull request 18). The question came back
on 8 and 9 November for the '05, '06 and '08 formats and the answer was
the same.
*Consequences:* the matrix row for older versions is out of scope, and the
'08 copies of three reference documents survive only on a side branch.

**An import says what it left out.**
*Why:* a reader that silently drops content cannot be trusted with real
documents.
*Consequences:* a log of what an import leaves out since phase 2, later
shown in the window as a banner with a list of document warnings. The
fifth gap review found the same silence when opening ODT written by other
programs, and it was fixed in the closing of phase 6.

**Missing fonts are replaced by substitutes with the same metrics.**
*Why:* documents made on a Mac use Apple's fonts, which cannot be
distributed, and a substitute with other widths moves every line break.
*Consequences:* a table of substitutes; the measured metrics of each face
of Helvetica Neue; Desk Neue, a substitute built to those metrics, when
one line of one caption still differed; and substitutes for the 41 fonts
of the Pages templates, placing words within 0.6 pt of Pages.

**Desk follows Pages where Pages differs from the textbook.**
*Why:* parity of behaviour. Each case was confirmed on the reference
machine before being copied.
*Consequences:* justification stretches each space by at most half an em
and then spaces the letters (pull request 91); Delete Page deletes the
whole section; a long footnote was handled as the owner chose after seeing
that Pages '09 moves it whole to the next page. Under D-020 these cases
are checked again against iWork 15.4.

**Every supported GNOME language, and no app is translated before it is
complete.**
*Why:* the owner asked how many languages GNOME 51 officially supports.
The rule that followed: Desk supports each of them that has a spelling
dictionary it can use, and an app is translated only when it has all of
its features, so that translators do not chase moving text.
*Consequences:* dictionaries and hyphenation patterns for 37 languages in
the Flatpak (pull request 50) and the policy in `po/LANGUAGES.md`.
Brazilian Portuguese is the one translation kept during development.

### The habit of looking for what was left behind (2 to 15 November)

**A gap review before each new phase.**
*Why:* the owner's instruction of 2 November, repeated in almost the same
words at least eight times: check that nothing was left undone that is not
mapped to a later phase.
*Consequences:* each review found something. Among the finds: 23 items
after phase 4; phase 5 announced as closed while outline mode, thumbnails
and the beta channel were missing; the Flatpak still on GNOME 50; no
roadmap for Tables and Slides; no object compared with Pages; matrix rows
with no test carrying their id; and matrices that still asked for the
opposite of a decision already applied. The reviews are the reason tests
are named after matrix rows and the reason the parity report exists.

**Tell the owner whenever a task is not done or is moved.**
*Why:* his instruction of 15 November, after asking twice in one evening
whether anything had been left undone.
*Consequences:* a standing project instruction; every report since ends
with what was left undone and where it went; and, on 16 November, the
register `docs/backlog/deferred.tsv` with a check that refuses to close a
phase with open items (D-015).

### The interface (13 to 15 November)

**Parity is of features, not of interface.**
*Why:* the owner's words of 13 November: the aim is a functional copy of
iWork for GNOME, using Libadwaita and as faithful to the GNOME guidelines
as possible, without hurting the familiarity of someone who comes from
iWork. A few minutes later he sharpened it: the interface of iWork has to
be carried to GNOME, adaptations are necessary, familiarity must not
break even where that costs something against the guidelines, and the
answer is the middle way. On 14 November he added the tie-breaker: where
a middle way is needed, look at the modern Pages, Keynote and Numbers,
which are already close to Libadwaita applications.
*Consequences:* the parity matrices gained a column saying how each row is
adapted to GNOME; the customizable toolbar, the several floating
Inspector windows and the version browser left the plan; and, two days
later, the same reasoning taken one step further became the new target.

**The decisions of 13 November.** One Inspector in a sidebar and popovers
instead of floating windows; menus in the header bar; the GNOME dialog on
closing ("Save changes?" with Cancel, Discard, Save); sentence-case labels
with the GNOME glossary; manual saving with a recovery draft every two
minutes (R-002); one window per document (R-003).
*Why:* each resolved a conflict between Pages '09 and the guidelines.
Floating palettes cannot be positioned or kept above the document under
Wayland, which settled the Inspector.
*Consequences:* applied to Writer in pull request 100 and to the plans of
Tables and Slides. R-002 and R-003 lasted three days: the current iWork
saves automatically with versions and opens documents in tabs, so D-001
replaced them.

**The newest libraries.**
*Why:* three proposals of the interface plan needed widgets that the
versions then in use (Libadwaita 1.5, GTK 4.14) did not have. Since Desk
ships only as a Flatpak, the owner chose to move to the newest stable
versions and use them fully.
*Consequences:* GTK 4.24, Libadwaita 1.10, Rust 1.100, the GNOME 51
runtime, and an Ubuntu 26.10 container in the continuous integration.

**GNOME's shortcuts first.** (Kept as D-017; D-016 for Help.)
*Why:* the owner's rule of 13 November: native GNOME shortcuts have
priority, and the iWork ones are adapted so as not to collide.
*Consequences:* paragraph styles moved from F1 to F8 to Ctrl+1 to Ctrl+8,
since F1 is Help and F9 and F10 belong to the window; list levels left
Alt+Tab; F11 is full screen. On 16 November the second half of the rule
changed from "the iWork '09 shortcut" to "the iWork 15.4 shortcut".

**Interface B.** (R-004.)
*Why:* of three mockups (Classic, Balanced, Lean) drawn with real widgets,
B balanced the familiarity of iWork with the guidelines: a navigation
sidebar, a header bar with the insert buttons, a slim format bar that
changes with the selection, an Inspector in two levels and a bottom bar.
*Consequences:* seven steps of migration in Writer, and Tables and Slides
born in that window. It was replaced by M+ two days later (D-008). The
move from B to M+ is smaller than a new window: the format bar and the
bottom bar give way to the Format pane and overlays, and documents open in
tabs.

**A minimum screen of 1280 × 720.**
*Why:* the owner's decision of 14 November: Desk is for desktops, laptops
and tablets. Phones and smaller screens are not supported, but nothing is
blocked: the apps warn at start that the screen is below the minimum.
*Consequences:* the phone layout planned in step 1 was removed; every
mockup scene was measured against the minimum; a banner warns on smaller
screens; the README, the site and the metainfo say what Desk needs.

### The new target (16 November)

**How it came about.** The three comparisons showed that the current iWork
is not simply iWork '09 with features missing:

| | Features compared | Same | Different | Only in '09 | Only in 15 | 15 better | '09 better | 15 closer to the GNOME guidelines |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Pages | 165 | 49 | 74 | 10 | 32 | 57 | 15 | 51 |
| Keynote | 187 | - | - | - | - | 77 | 6 | 56 |
| Numbers | 187 | - | - | - | - | 54 | 7 | 63 |

A dash is a count the summary of that comparison did not give. In no
feature of Pages was iWork '09 closer to the guidelines than the current
version. iWork '09 wins where the owner had felt it: long documents in
Pages (outline mode, the styles drawer, the list of search results,
tracking of format changes, document statistics), Smart Builds in Keynote,
and the formula bar, the formula list, split cells and reusable tables in
Numbers. The check on a Mac changed no verdict and added facts: Numbers
15.4 has 355 functions, the 262 of '09 and 93 new ones of which 4 need the
network; Keynote 15 removed 7 transitions of '09.

**D-001 to D-003: the target is the current iWork, and nothing '09 did is
lost.** Shared features behave as in 15.4; features only '09 has are kept,
presented as the current iWork would present them, including features
without interface such as charts and tables that update when their source
document changes; what the current iWork lost inside a shared feature, or
does only with AI, is kept the same way (wrap direction, split cells, the
3D chart scene, tracking of format changes, Instant Alpha, the
Proofreader).
*Why:* it gives the owner what moved him in the first place, the features
Apple removed, in the form that is both the better one in most cases and
the one closest to GNOME. The first rule he tried that morning, "the
better version of each feature" (R-005), needed a judgement per feature;
this one does not.
*Consequences:* R-001, R-002, R-003 and R-005 were replaced. Of the work
done, 62 of 494 features in scope count as finished without a second
look, and 30 that were done or had a base need to be looked at again. The
old phases 8 to 11 of Writer and all but phase 2 of Tables and Slides
stopped, and a new roadmap replaced them. The estimate grew from about 50
working days (5 January) to about 70 (end of January, between 10 January
and 15 February).

**D-004 and D-005: out of scope, never erased.** Features that need AI, an
online service or a payment stay in the matrices with their mark and
reason, and the architecture must not rule them out. Features tied to
Apple platforms are "not applicable to GNOME" unless GNOME has an
equivalent.
*Why:* the owner asked for them to be marked out of scope "but not
discarded".
*Consequences:* 72 of the 566 inventory rows are out of the phases.
Taking one back is a new decision.

**D-007 and D-008: native widgets, and the M+ window.**
*Why:* the owner asked for two mockups: N, the look of the current iWork
with native Libadwaita widgets, and M, a mix that conflicts least with the
guidelines. N needed one widget of Desk's own (a toolbar item with a
caption) and conflicted with the guidelines in six points; M had neither.
Before choosing, he asked where in M the functions that N shows in the
toolbar would be, because he did not want a user to miss a function for
not knowing how to reach it. In M the six object buttons were one click
further, inside Insert. M+ puts them back in the header bar as icons.
*Consequences:* M+ is the window of the three apps. Where Libadwaita has
no widget, Desk builds one by the guidelines and marks it for review at
each new Libadwaita. The secondary scenes (popovers, menus, dialogs,
narrow window) were not drawn and are backlog item B-003.

**D-010 and D-011: five families of files, and a clean-room reader.**
*Why:* the owner listed what Desk must read: its own ODF, iWork '09,
current iWork, current Microsoft Office, and current LibreOffice and
OpenOffice. On the current iWork files he asked first that it be checked
against Apple's copyright and terms, and said that if it infringed, the
reader would exist only to check parity and be discarded at the end. The
assessment (technical, not legal advice) was that reading a user's own
document copies no code of Apple's, that the delicate point is that
public documentation of the format originates in Apple's executables,
which is a matter of the licence agreement more than of copyright, and
that the laws differ.
*Consequences:* four rules: nothing is extracted from Apple's executables;
the reader is written from public third-party documentation and from
documents made by the project; no template, theme, font or image of Apple
enters the repository; the product names are used only to say what Desk
is compatible with. The first record of the decision asked for a lawyer's
review of the second rule before version 1.0 (backlog item B-004). Later
the same day the owner decided that the project follows the practice of
LibreOffice, whose import filter for these files (libetonyek, under
MPL-2.0) was written from the file format and has been distributed for
years, and that no legal review is required; B-004 was dropped (pull
request 119). The readers of Numbers '09 and Keynote '09 became new work.

**D-019: four export targets.** ODF as LibreOffice and OpenOffice read
it, Microsoft Office, PDF and EPUB.
*Why:* the owner's decision when the plan assumed more.
*Consequences:* RTF, plain text, CSV and TSV, images, movie, animated GIF,
HTML and the older Office formats are out of scope and marked. CSV in
Tables was pointed out as the one worth reconsidering.

**D-012, then D-020: where behaviour is checked.** D-012, in the
replanning, made iWork 15.4 on a Mac the reference for everything
(replacing R-006).
*Why:* the target is the behaviour of the current versions, so the check
has to be against them.
*Consequences:* the reference cases made with Pages '09 are redone, and
where 15.4 breaks lines differently the text engine is adjusted. The old
cases remain as tests of the reader of iWork '09 files. The same
afternoon the owner refined it (D-020): validations, interface tests and
parity documents are made on the Mac with the current iWork, but features
that only iWork '09 has are tested, and their documents made and checked,
on the Lion virtual machine that shows the screen, built for that purpose.
The current iWork cannot show what it no longer has. So the Lion machine
stays in use for as long as those features are being built; the Mojave
machine, the first reference, is left with the fixtures of the iWork '09
readers.

**D-014 and D-018: documentation for contributors, all in English.**
*Why:* the owner wants outside contributors, human or using coding agents,
to be able to understand the project from the repository: architecture,
definitions, roadmap, diagrams, and source files commented to be
understood on their own. When the first version of the replanning came
with documentation in English and matrices in Portuguese, he ordered
everything kept in English, the mockups included.
*Consequences:* `docs/`, `CONTRIBUTING.md` and `AGENTS.md`; the older
material in Portuguese is translated in phase B1 (backlog item B-009,
done on 16 November: the matrices, `docs/interface` and the mockups);
bringing the comments of the existing code to the new standard is in
phase X1.

**D-015: nothing is dropped silently.** See "The habit of looking for what
was left behind".

**D-026: notes in the code cite an open backlog row (17 November).**
*Why:* the owner adopted rules written against the distraction of coding
agents: a `TODO` met while reading pulls an agent away from its task,
often after a bug that is already fixed, and costs steps and tokens.
Tying each note to a row of the register lets the agent check whether
the work is still pending, and closing the row removes the note.
*Consequences:* rules 12 and 13 of `AGENTS.md`, rules 5 and 6 of the
backlog; `ci/check-backlog.py` checks the notes, before each pull request
and in the CI. Problems noticed outside a task go to the register and to
the report, not into the change.

**A dashboard that is always current.**
*Why:* the owner asked to follow the progress on a page, kept up to date
after each set of tasks, with status, backlog and forecast, and published
on the website.
*Consequences:* a dashboard generated from the repository (pull request
118), and a standing instruction to update it. This log joined the list of
documents kept up to date on the same day.

## What changed direction, and what it cost

| Date | Change | What was redone or set aside |
| --- | --- | --- |
| 31 Oct | Flathub to the project's own Flatpak repository | The release steps of the plan |
| 1 Nov | Pages '08 dropped | Three reference documents |
| 13 Nov | Interface copied from Pages '09 to a middle way with the GNOME guidelines | Inspector windows, Fonts and Colors windows, Find & Replace window, style shortcuts |
| 14 Nov | First adaptation (pull request 100) to interface B | Seven migration steps in Writer |
| 14 Nov | Phone widths to a minimum of 1280 × 720 | The narrow layout of step 1 |
| 16 Nov | iWork '09 to iWork 15.4 as the target | Roadmap, matrices, the recovery draft, one window per document, 30 features to look at again, all reference cases |
| 16 Nov | Interface B to M+ | The format bar, the bottom bar, the sheets of Tables in the sidebar |
| 16 Nov | One reference for checking (iWork 15.4) to two (D-020) | Nothing redone: the cases of features only iWork '09 has stay on the Lion virtual machine |
| 16 Nov | Portuguese to English for everything in the repository | Matrices, `docs/interface`, mockup scripts |

What did not change in seventeen days: Rust, GTK and Libadwaita; ODF with
extensions as the native format; the shared engine under three apps; the
licences; Flatpak; tests tied to an inventory; and a real iWork as the
judge of behaviour.

## Where the project stands, and what comes next

On 16 November 2026 Desk Writer has its text engine, document structure,
writing tools, objects, tables and charts, and reads Pages '09 documents
with the layout of Pages. Desk Tables and Desk Slides have a window, and
engines for tables, formulas, charts and animation that the apps do not
use yet.

Phase B1 of the [roadmap](roadmap.md), the common base, ran from 16 to
17 November: the M+ window in the three apps, automatic saving with
versions, the reference cases redone on iWork 15.4, and the backlog
carried from the replanning. The owner closed it on 17 November and
chose W1, Writer to the behaviour of 15, as the next phase. The three
apps can advance side by side: six phases of Writer, five of Tables, five
of Slides, and a last phase across the apps before version 1.0.

Open questions that this log should close when they are answered:

- Whether the documents of `corpus/fixtures/pages` get PDFs of Pages
  15.4 as the cases did.
- Whether CSV export comes back into scope for Tables.
- How close the estimate was, phase by phase.

## Sources

- The repository history and its pull requests 1 to 121.
- [decisions.md](decisions.md), [goals.md](goals.md),
  [roadmap.md](roadmap.md) and [backlog](backlog/README.md).
- The project conversations between the owner and Claude from 30 October
  to 16 November 2026, from which the owner's reasons are taken. They are
  paraphrased from Portuguese.
- The reports kept with the project and not in the repository: the feature
  surveys of Pages, Numbers and Keynote '09; the comparison of Pages with
  the GNOME guidelines; the six gap reviews; the comparisons of each
  iWork '09 app with its 15.4 version; the check on the Mac; and the
  inventory and estimate of 16 November.

## Keeping this log

This log is one of the documents updated throughout the project, with the
inventory, the backlog register and the dashboard.

1. At the end of each set of tasks, add an entry to the timeline: the
   date, what was delivered with its pull requests, and what was left
   undone and where it went. Add the same changes, one short line each,
   to [HISTORY.md](../HISTORY.md), the summary for visitors of the
   repository.
2. When the owner takes a decision, or one is replaced, add it to
   [decisions.md](decisions.md) first, then here with its reason, in his
   words where they exist, and its consequences. Never rewrite an old
   entry to fit a new decision: add the new one and say what it replaced.
3. When a consequence only shows later (an estimate missed, a decision
   that cost rework), add it to the decision it belongs to and to "What
   changed direction".
4. Keep the numbers dated. A count without its date is wrong the next day.
