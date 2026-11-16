# The Desk extensions to OpenDocument

Desk saves OpenDocument (ODF 1.3) and keeps what standard ODF cannot hold
in its own namespace, `desk:` (`https://brahm.com.br/ns/desk/1.0`).
Decision D-009 asks for one documented extension for every implemented
feature that ODF has no place for. This page is the catalogue: the rules,
what exists today, and what the new target still needs.

## Rules

1. **Standard first.** A feature uses the ODF element or attribute when
   one exists, even an awkward one. An extension is the last resort.
2. **Extended conforming documents.** With every `desk:` element and
   attribute removed, the document validates against the ODF 1.3 schema
   (ODF part 3, section 3.17). `ci/validate-odf.sh` checks this with
   `ci/strip-extensions.py`.
3. **A reader that ignores the extensions still shows the document.** The
   standard part carries the closest thing ODF can say (a picture with its
   mask also stores the cropped fallback, a pivot table also stores its
   computed cells), so LibreOffice opens the file with a sensible result.
4. **Desk never drops what it does not understand.** Foreign elements of
   other producers are kept on a round trip where the model allows it and
   listed in the import warnings where it does not.
5. **Each extension is documented here before it is merged**, with the
   feature (inventory id), the element it sits on, its values and its
   fallback. The reader and the writer cite this page in their comments.
6. **The namespace is versioned.** A change that an older Desk would
   misread gets a new name, not a new meaning for an old one.

## Extensions that exist today

Read from `crates/desk-odf/src` on 2026-11-16, at the start of phase B1:
43 names. Each one is defined below by where it sits, what its value is,
what the standard part of the document says in its place (the fallback a
reader without Desk sees), and the rows of the parity matrices that it
serves. `ci/check-extensions.py` fails when a name used in
`crates/desk-odf/src` has no section here, or a section names one that no
longer exists.

Common forms of value:

- A **length** is an ODF length (`12pt`, `0.5cm`, `3mm`, `1in`); Desk
  writes points.
- A **percent** is a decimal number followed by `%`, as `35%` or `12.5%`.
- A **boolean** is `true` or `false`.
- **Ten-thousandths** are whole numbers where 10000 is the full width or
  height of the frame or picture they are measured in (the
  `POLYGON_SCALE` and `CONTOUR_SCALE` of the code).
- **Seconds** are a decimal number of seconds with at most three decimals
  and no trailing zeros, as `2.5`.
- A value that Desk cannot parse is ignored as if it were absent, and the
  standard part is read instead. Numbers are clamped to the ranges given
  here, against hostile files.

### Pictures and fills (`style:graphic-properties`)

These sit on the `style:graphic-properties` of the graphic style of a
`draw:frame` or shape, written by `write_graphic_properties` in
`draw.rs` and read by `graphic_properties`.

#### `desk:fill-image-mode`

- **Feature:** a picture that fills an object, sized as Pages sizes it
  (OBJ-42).
- **Value:** `scale-to-fill` (the picture covers the object and is cut to
  it, keeping its proportions) or `scale-to-fit` (the picture fits inside
  the object, keeping its proportions).
- **Fallback:** `style:repeat="stretch"`: the picture is stretched to the
  object. The other modes of Desk (original size, tile, stretch) are
  standard `style:repeat` values and need no extension.

#### `desk:fill-tint`

- **Feature:** a color laid over a picture fill (OBJ-42).
- **Value:** an ODF color, `#rrggbb`.
- **Fallback:** none; other programs show the picture without the tint.

#### `desk:fill-tint-opacity`

- **Feature:** the opacity of `desk:fill-tint` (OBJ-42).
- **Value:** a percent from 0 to 100; 100 when absent.
- **Fallback:** none.

#### `desk:mask`

- **Feature:** the mask of a picture, which can be moved and resized
  inside the frame without cutting the picture (OBJ-04).
- **Value:** four lengths separated by spaces, `x y width height`: where
  the whole picture lies, from the top left corner of the frame. `x` and
  `y` are zero or negative.
- **Fallback:** `fo:clip`, the cut that shows the same part of the
  picture; other programs show the masked picture but cannot move it in
  the mask.

#### `desk:mask-shape`

- **Feature:** a mask with a shape other than a rectangle (OBJ-04).
- **Value:** one of
  - `ellipse`;
  - `rounded R`, a rectangle with rounded corners of radius `R` points
    (0 to 10000);
  - `points X Y X Y …`, a polygon of at least three points, in
    ten-thousandths of the frame (each clamped to -10000..20000);
  - `outline PATH`, a path in ten-thousandths of the frame, in the
    commands `M`, `L`, `C` and `Z` of `draw:enhanced-path`;
  - a value of [`desk:shape`](#deskshape) (`star`, `polygon`, `arrow`,
    `double-arrow`, `balloon`).
  Absent means a rectangle.
- **Fallback:** the rectangle of `fo:clip`.

#### `desk:image-adjust`

- **Feature:** Adjust Image (OBJ-07).
- **Value:** six whole numbers from -100 to 100 separated by spaces:
  brightness, contrast, saturation, exposure, temperature and tint. Zero is
  no change. Any other count of numbers is ignored.
- **Fallback:** `draw:luminance` and `draw:contrast` carry the brightness
  and the contrast; the other four are lost to other programs.

#### `desk:instant-alpha`

- **Feature:** Instant Alpha, the colors removed by clicking a picture
  (OBJ-06).
- **Value:** the clicks in order, separated by commas, each as
  `x y tolerance`: `x` and `y` in ten-thousandths of the width and height
  of the picture, `tolerance` from 0 to 100. Desk reads at most 256.
- **Fallback:** none; other programs show the whole picture.

#### `desk:alpha-kept`

- **Feature:** the part of a picture that Instant Alpha keeps, as Pages
  '09 saves it in its own files, so that a document opened from Pages
  keeps it without the clicks (OBJ-06).
- **Value:** polygons separated by semicolons, each a list of `x y` pairs
  in ten-thousandths of the picture; a point is kept when it is inside by
  the even-odd rule.
- **Fallback:** none.

#### `desk:picture-frame`

- **Feature:** a picture frame of the library of Desk around an object
  (OBJ-36).
- **Value:** the name of the frame: `photo`, `gallery`, `classic`,
  `metal` or `tape` (`FrameStyle::name` in `desk-model`). An unknown name
  is ignored.
- **Fallback:** the stroke of the object, as `svg:stroke-color` and
  `svg:stroke-width`, when it has one; otherwise none.

#### `desk:picture-frame-scale`

- **Feature:** the thickness of the picture frame (OBJ-36).
- **Value:** a percent of the frame's own thickness; 100 when absent.
- **Fallback:** none.

#### `desk:reflection`

- **Feature:** the reflection below an object (OBJ-38).
- **Value:** a percent from 0 to 100, how opaque the reflection is where
  it starts; it fades to nothing at its end.
- **Fallback:** none; other programs show no reflection.

#### `desk:wrap-threshold`

- **Feature:** tight wrap around the visible part of a picture, by the
  alpha threshold of Pages (OBJ-17).
- **Value:** a percent from 0 to 100: how opaque a pixel must be for the
  text to go around it.
- **Fallback:** `draw:contour-polygon` on the frame, the outline Desk
  computed with that threshold, and `style:wrap-contour="true"`.

### Lines and shadows (`style:graphic-properties`)

#### `desk:stroke-style`

- **Feature:** a dashed or dotted line in the two patterns of Desk
  (OBJ-35).
- **Value:** `dashed` or `dotted`. Absent means solid, or the dash of
  `draw:stroke` read as dashed.
- **Fallback:** `draw:stroke="dash"`, which other programs draw with their
  default dash.

#### `desk:shadow-blur`

- **Feature:** the blur of the shadow of an object (OBJ-37).
- **Value:** a length, the radius of the blur.
- **Fallback:** `draw:shadow`, `draw:shadow-offset-x`,
  `draw:shadow-offset-y`, `draw:shadow-color` and `draw:shadow-opacity`
  give a sharp shadow. Desk also reads `loext:shadow-blur`, which
  LibreOffice writes for the same thing.

### Object styles (`style:graphic-properties` of a named style)

#### `desk:object-style`

- **Feature:** object styles, the presets of shapes, lines, text boxes
  and pictures (MOD-04).
- **On:** the `style:graphic-properties` of a named graphic style in
  `office:styles`.
- **Value:** the kind of object it is a style of: `shape`, `line`,
  `text-box` or `image`.
- **Fallback:** the style is an ordinary graphic style, which objects
  still use by `draw:style-name`.

#### `desk:default-object-style`

- **Feature:** the object style that new objects of a kind take (MOD-04).
- **On:** the same element as `desk:object-style`.
- **Value:** a boolean; `false` when absent.
- **Fallback:** none; other programs use their own defaults.

### Master pages (`style:graphic-properties`)

#### `desk:master-pages`

- **Feature:** an object of a section master that shows on the first page
  only, or on left pages only (EST-19, EST-21).
- **Value:** `first` or `left`. Absent means every page of the section.
- **Fallback:** the object is on the master page, so other programs show
  it on every page that uses it.

### Gradients (`draw:gradient`)

#### `desk:gradient-stop`

- **Feature:** advanced gradients with more than two colors (OBJ-41).
- **On:** an element, zero or more children of `draw:gradient` in
  `office:styles`, in order of `svg:offset`.
- **Value:** the element carries `svg:offset` (0 to 1, where the color
  is along the gradient) and the three attributes below.
- **Fallback:** `draw:start-color` and `draw:end-color` of the gradient
  are its first and last colors, so other programs draw a two-color
  gradient.

#### `desk:color`

- **On:** `desk:gradient-stop`.
- **Value:** an ODF color, `#rrggbb`.

#### `desk:opacity`

- **On:** `desk:gradient-stop`.
- **Value:** a percent from 0 to 100; 100 when absent.

#### `desk:blend`

- **On:** `desk:gradient-stop`.
- **Value:** a number from 0 to 1: where between this color and the next
  the two are blended half and half; 0.5 when absent.

### Drawing objects (`draw:` elements)

#### `desk:origin`

- **Feature:** what the copies of one object share, which Magic Move of
  Desk Slides matches from slide to slide (IOP-33).
- **On:** any drawing object (`draw:frame`, `draw:custom-shape`,
  `draw:connector` and the others written by `write_draw_object`).
- **Value:** sixteen hexadecimal digits, an identifier that a copy keeps.
- **Fallback:** none; other programs match by their own rules.

#### `desk:shape`

- **Feature:** the shapes of Desk with settings that the user changes
  with handles: stars, regular polygons, arrows, balloons and drawn paths
  (OBJ-08, OBJ-09, OBJ-11).
- **On:** `draw:enhanced-geometry` of a `draw:custom-shape` whose
  `draw:type` is `non-primitive`.
- **Value:** a kind followed by numbers, separated by spaces:
  - `star POINTS INNER`: 3 to the maximum number of sides; `INNER` the
    inner radius in ten-thousandths of the outer one;
  - `polygon SIDES`;
  - `arrow HEAD TAIL` and `double-arrow HEAD TAIL`: the length of the
    head and the thickness of the tail, in ten-thousandths;
  - `balloon RADIUS BODY TIP`: the corner radius in points, and the height
    of the body and the place of the tip in ten-thousandths;
  - `path closed` or `path open`: a shape drawn with the pen, whose
    points are those of the enhanced path.
- **Fallback:** `draw:enhanced-path` draws the same outline, so other
  programs show the shape, without its handles.

#### `desk:bend`

- **Feature:** a connection line bent by its middle handle (OBJ-31).
- **On:** `draw:connector`.
- **Value:** a length: how far the middle of the line is pulled aside, to
  the left of the way from the start to the end; negative to the right.
- **Fallback:** `svg:d` draws the bent line, so other programs show it as
  it is, without the handle.

#### `desk:chart`

- **Feature:** everything Desk keeps of a chart: its type, data, series,
  axes, labels and styles (the GRF rows).
- **On:** the `draw:object` of a frame that holds a chart.
- **Value:** the chart of `desk-model` (`desk_model::Chart`) as JSON,
  written with `serde_json`. Desk reads it back first; when it is absent
  or cannot be read, Desk reads the `chart:` object of the package.
- **Fallback:** the chart object of the package (`Object N/content.xml`,
  ODF chart), written with the same data, which other programs draw;
  types that ODF charts have no equivalent for are drawn as their nearest
  type.
- **Versioning:** a field added to the model is read as its default from
  an older file. A field removed or changed in meaning gets a new
  attribute name (rule 6).

#### `desk:media-start`, `desk:media-stop`, `desk:poster-time`

- **Feature:** the trim and the poster frame of a movie or a sound
  (OBJ-19).
- **On:** `draw:plugin`.
- **Value:** seconds. `desk:media-start` is where it starts (0 when
  absent), `desk:media-stop` where it stops (the end when absent),
  `desk:poster-time` where the poster frame was taken (0 when absent).
- **Fallback:** the whole file plays from its start; the poster frame
  is the `draw:image` beside the plugin.

#### `desk:repeat`

- **Feature:** how a movie or a sound repeats (OBJ-19).
- **On:** `draw:plugin`.
- **Value:** `loop` or `back-and-forth`. Absent means it plays once.
- **Fallback:** `draw:param` `Loop` is `true` for both, which other
  programs play as a loop.

#### `desk:duration`

- **Feature:** the length of a movie or a sound, kept so that Desk can
  lay out the trim controls before it decodes the file (OBJ-18).
- **On:** `draw:plugin`.
- **Value:** seconds.
- **Fallback:** none needed; other programs read the length from the
  file.

### Tables (`table:table`)

#### `desk:table`

- **Feature:** everything Desk keeps of a table: cells with their values,
  formulas and formats, merged cells, styles, headers and footers, and
  options that ODF tables cannot hold (the TAB rows).
- **On:** `table:table`.
- **Value:** the table of `desk-model` (`desk_model::Table`) as JSON,
  written with `serde_json`. Desk reads it back first; when it is absent
  or cannot be read, Desk reads the standard table.
- **Fallback:** the standard table, with its columns, rows, cells, values
  and formulas in OpenFormula, which other programs show and compute.
- **Versioning:** as for `desk:chart`.

### Text: paragraphs and characters (`style:text-properties`)

#### `desk:tracking`

- **Feature:** the spacing of letters as Pages measures it (FMT-10).
- **Value:** a whole number, the spacing in thousandths of the font size;
  negative is tighter.
- **Fallback:** `fo:letter-spacing`, the same spacing as a length for the
  font size of the style. When `desk:tracking` is absent, Desk derives it
  from `fo:letter-spacing`.

#### `desk:shadow-opacity`

- **Feature:** the opacity of the shadow of text (FMT-03).
- **Value:** a percent from 0 to 100; 100 when absent.
- **Fallback:** `fo:text-shadow`, an opaque shadow.

#### `desk:hyphenate`

- **Feature:** a paragraph that allows hyphens, kept when the document
  does not hyphenate (TXT-09).
- **Value:** a boolean.
- **Fallback:** `fo:hyphenate`, which then says `false`, so that other
  programs do not hyphenate a document that Desk shows without hyphens.
  When the document hyphenates, Desk writes `fo:hyphenate` alone.

### Text: lists (`text:list-level-style-image`)

#### `desk:relative-size`

- **Feature:** the size of a picture bullet relative to the text (LST-03).
- **Value:** a percent: the height of the picture as a part of the size of
  the text.
- **Fallback:** `fo:width` and `fo:height` of
  `style:list-level-properties`, the size of the picture for the text of
  the style.

#### `desk:raise`

- **Feature:** how far a picture bullet is raised from the baseline
  (LST-03).
- **Value:** a length; negative lowers it.
- **Fallback:** none; other programs set the picture on the baseline.

### Text: fields

#### `desk:pattern`

- **Feature:** the format of a date and time field, as Pages offers it
  (EST-27).
- **On:** `text:date`.
- **Value:** the pattern in the date symbols of Unicode (UTS #35), as
  `EEEE, d MMMM y`.
- **Fallback:** the text of the field, the date as it was formatted when
  the document was saved; `text:fixed` says whether it updates.

### Sections (`style:section-properties`)

Desk writes a section of Pages, which starts on a page of its own and
numbers its pages, as an ODF section with these attributes; the first
paragraph of the section also breaks the page and sets its number, so that
other programs paginate it the same way (`sections.rs` in
`desk-filters`).

#### `desk:space-before`, `desk:space-after`

- **Feature:** the space above and below a section of a text layout
  document (EST-16).
- **Value:** a length.
- **Fallback:** the space before the first paragraph and after the last.

#### `desk:new-page`

- **Feature:** a section that starts on a new page (EST-16).
- **Value:** a boolean.
- **Fallback:** `fo:break-before="page"` on the first paragraph.

#### `desk:section-start`

- **Feature:** the page a section starts on (EST-16).
- **Value:** `any` (the next page), `left` (the next left page) or
  `right` (the next right page).
- **Fallback:** a page break on the first paragraph, which starts the next
  page whatever its side.

#### `desk:first-page-different`, `desk:left-right-different`

- **Feature:** a section whose first page, or whose left and right pages,
  have their own headers, footers and master objects (EST-19).
- **Value:** a boolean.
- **Fallback:** the master page of the section, whose
  `style:header-first` and `style:header-left` hold the headers of
  first and left pages where it has them.

## Extensions the new target needs

A first pass over [the inventory](../corpus/inventory/README.md): the
features of the target that ODF 1.3 has no standard place for. Each one is
specified here, by the rules above, in the phase that implements it.

| Feature | Apps | Why ODF is not enough | Fallback in the standard part |
| --- | --- | --- | --- |
| Versions of the document (browse and restore) | All | ODF has no version history that other producers read | The current version is the document; older ones are extra streams of the package |
| Live link of a chart or table to another document | All | ODF links an object to a file, without a table and range that update | The last data, embedded |
| Object, table and chart styles as named presets of the template | All | ODF styles have no "preset gallery" order or thumbnails | Ordinary automatic styles |
| Shape library entries, combined shapes, editable freehand drawings | All | ODF keeps the resulting path, not the operation | The resulting path |
| Image gallery (several pictures in one frame, with captions) | All | No standard element | The first picture, with its caption |
| Captions and titles tied to an object | All | ODF captions are free text frames | A text frame grouped with the object |
| 3D objects (models with a view angle) | All | No standard element for a model with a camera | A rendered picture |
| Equations entered as LaTeX | All | ODF stores MathML only | MathML, with the LaTeX source as the extension |
| Text fill with gradient or image, text outline options | All | ODF text properties have a colour only | The main colour |
| Drop caps beyond the ODF model, automatic fractions, phonetic guide details | Writer | ODF covers part of each | The ODF drop cap, the literal characters, ODF ruby |
| Page templates (pages of the template to add), section masters | Writer | ODF master pages have no "page to insert" with placeholders | Ordinary master pages |
| Placeholder text and media placeholders | All | ODF has a placeholder field for text only | Ordinary content |
| Threaded text boxes with an order shown | Writer | ODF chains frames, without the display data | The ODF chain |
| Comment threads with replies and highlights | All | ODF 1.3 annotations have no reply relation | Flat annotations, in order |
| Tracking of format and table-cell changes | Writer | ODF tracks insertions, deletions and some format changes only | The changes ODF can express |
| Several free tables on a sheet, each with its name, headers and footers | Tables | An ODF sheet is one grid | Each table as a range of the sheet, as today in `desk:table` |
| Categories (grouped rows with summaries) | Tables | ODF has outline groups and subtotals, not categories | Outline groups and subtotal rows |
| Pivot tables as iWork models them | Tables | ODF data pilot tables differ in options | An ODF data pilot table, plus the computed cells |
| Cell controls: checkbox, slider, stepper, pop-up, star rating | Tables | ODF has form controls, not cell formats of this kind | The value, with a validation list where one applies |
| Interactive charts, reference lines, summary labels, rounded corners | All | ODF chart has no such properties | A static chart |
| Slide layouts with tagged placeholders, object defaults per master | Slides | ODF presentation layouts are simpler | ODF master pages and presentation classes |
| Builds and transitions that SMIL in ODF cannot name (Magic Move by word and letter, the '09 effects kept by D-003) | Slides | ODF animation names a fixed set | The nearest standard effect |
| Live video objects, animated backgrounds | Slides | No standard element | A still picture |
| Recorded presentation timing with narration | Slides | ODF holds slide timings, not a synced recording | Slide timings and the sound file |
| Presentation exit password and kiosk options | Slides | No standard place | None |

Features that are out of scope by D-004 (AI, online, paid) get no
extension now; their rows stay in the inventory.
