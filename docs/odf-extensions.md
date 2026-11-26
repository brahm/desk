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
  computed with that threshold, and `style:wrap-contour="true"`; a
  `draw:contour-path` with one subpath for each part when the opaque
  parts of the picture are apart (B-047).

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

#### `desk:text-box-paragraph-style`

- **Feature:** the appearance of new text boxes of a document, as Set as
  Default Text Box Appearance of Pages 15.4 sets it (PXV-10). The rest of
  it is standard ODF on the same default text box style: `fo:padding`,
  `draw:textarea-vertical-align`, `style:wrap` and `style:run-through`
  in its graphic properties, and the formatting of the text in its
  `style:paragraph-properties` and `style:text-properties`, which Desk
  writes only once the document has set it.
- **On:** the same element as `desk:object-style`, of the default text
  box style.
- **Value:** the name of the paragraph style of the text of new text
  boxes.
- **Fallback:** other programs give new text boxes their own text.

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

#### `desk:moved-from`

- **Feature:** where a floating object was before a tracked change
  moved or resized it, which rejecting the change puts it back to, as
  Pages '09 tracked it (W148, REV-01, D-003). ODF tracks changes of
  text only.
- **On:** a drawing object fixed on a page or anchored to a paragraph
  of the body, as `write_draw_object` writes it.
- **Value:** words apart: the `text:id` of the `text:changed-region` of
  the change, a `text:format-change` that no text marks, one for all
  the objects the change moved; `page` and the page from 1, or
  `paragraph` and the index of the paragraph in the body from 0, with
  the paragraphs that deletions took counted where they were; and the
  `x`, `y`, width and height of the frame there, as lengths, as Desk
  keeps the frame (from the left edge of the paper for an object of a
  paragraph).
- **Fallback:** the object where it is, as if the change were
  accepted; other programs read the region as a change that holds no
  text, and leave it out.

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

#### `desk:thread`

- **Feature:** a thread of text boxes of Pages with one box in it, which
  shows its number on the box and that other boxes can be added to
  (PXV-13, W091). Threads of several boxes are the standard links of
  `draw:chain-next-name`; Desk also writes the attribute on the first box
  of those, so that taking the others out keeps it in a thread.
- **On:** `draw:text-box`.
- **Value:** a boolean. Absent means the box is in a thread only when it
  is linked with others.
- **Fallback:** none needed; the box shows the same text, without the
  number of its thread.

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
  The title of a table (PXV-15), shown, centered and outlined, is among
  the options it keeps; other programs show the table without it. The
  lock of a table (PXV-16) is the standard `style:protect` of its frame.
  A cell that a tracked change changed (W148, REV-05, D-003) keeps, under
  `tracked`, what it held, its format and its style before the change,
  with `change`, the number in the `desk:change-name` `c<number>` of
  the `text:changed-region` of the change, a `text:format-change` that
  no text marks, one for each cell, and `others`, true when authors other
  than that of the change changed it after; other programs show the
  cell as it is now, as if the change were accepted, and leave the
  region out.
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

#### `desk:title-case`

- **Feature:** Title Case, which leaves the short words that join others
  in lower case, beside Start Case, which capitalizes every word (FMT-07,
  W054, PXV-05).
- **Value:** a boolean, written beside `fo:text-transform="capitalize"`:
  `true` for Title Case, `false` for Start Case.
- **Fallback:** `fo:text-transform="capitalize"`, which capitalizes every
  word. Without `desk:title-case`, Desk reads `capitalize` as Start Case,
  as other programs mean it; a document that an older Desk saved with
  Title Case reads as Start Case.

#### `desk:text-fill`

- **Feature:** a gradient or a picture that fills the letters in place of
  their color: the Gradient Fill and Image Fill of the text color of
  Pages (PXV-04, W052).
- **Value:** `gradient`, `bitmap` or `none`; `none` takes away a fill
  that the style of the text has.
- **Fallback:** `fo:color`, which Desk keeps beside it, so that other
  programs show the letters in that color.

#### `desk:text-fill-gradient-name`

- **Feature:** the gradient that fills the letters (PXV-04).
- **Value:** the name of a `draw:gradient` of `office:styles`, the
  gradients that objects fill with, with the `desk:gradient-stop` of an
  advanced gradient.
- **Fallback:** as `desk:text-fill`.

#### `desk:text-fill-image-name`

- **Feature:** the picture that fills the letters (PXV-04).
- **Value:** the name of a `draw:fill-image` of `office:styles`.
- **Fallback:** as `desk:text-fill`.

#### `desk:text-fill-image-mode`

- **Feature:** how the picture that fills the letters is sized to them
  (PXV-04).
- **Value:** `scale-to-fill`, `scale-to-fit`, `stretch`, `original-size`
  or `tile`; `scale-to-fill` when absent.
- **Fallback:** as `desk:text-fill`.

#### `desk:text-fill-tint`

- **Feature:** a color laid over the picture that fills the letters
  (PXV-04).
- **Value:** an ODF color, `#rrggbb`.
- **Fallback:** as `desk:text-fill`.

#### `desk:text-fill-tint-opacity`

- **Feature:** the opacity of `desk:text-fill-tint` (PXV-04).
- **Value:** a percent from 0 to 100; 100 when absent.
- **Fallback:** as `desk:text-fill`.

#### `desk:text-fill-page`

- **Feature:** "Apply gradient to entire page" of Pages 15.4: the
  gradient of `desk:text-fill` runs across the whole page rather than
  the text that has it (W052).
- **Value:** `true` or `false`; `false` when absent.
- **Fallback:** as `desk:text-fill`.

### Text: drop caps (`style:drop-cap`)

The drop cap is the standard `style:drop-cap` of the paragraph
properties, with its `style:length`, `style:lines` (the whole height of
the letters, in lines) and `style:distance`. These attributes add the
options of Pages 15.4 (PXV-06), written by `paragraph_children` in
`write.rs` and read by `paragraph_properties`.

#### `desk:drop-cap-raised`

- **Feature:** a raised drop cap, whose letters rise above the first line
  of the text by some of their lines (PXV-06).
- **Value:** a whole number of lines, from 0 to one less than
  `style:lines`.
- **Fallback:** a drop cap as tall, beside the first lines.

#### `desk:drop-cap-wrap`

- **Feature:** whether the lines beside a drop cap follow the shape of
  its letters or the rectangle around them (PXV-06).
- **Value:** `shape` or `rectangle`. Without it, Desk reads `rectangle`,
  as other programs draw it.
- **Fallback:** the rectangle.

#### `desk:drop-cap-outdent`

- **Feature:** a drop cap that stands out into the margin, outside the
  indent, by a part of its width (PXV-06). At 100 % it ends at the
  indent, and the lines beside it start there.
- **Value:** a percent of the width of the letters, from 0 % to 200 %.
  Desk wrote `true` and `false` before 18 November 2026, which it reads
  as 100 % and 0 %.
- **Fallback:** the letters inside the indent.

#### `desk:drop-cap-fill`

- **Feature:** a shape behind the letters of a drop cap, a rectangle with
  rounded corners as tall as the letters would be, with the letters
  smaller in its middle and the lines beside it (PXV-06).
- **Value:** the color inside, as `#rrggbb`. The other attributes of the
  shape are read only with it.
- **Fallback:** the letters without a shape, at their full height.

#### `desk:drop-cap-border-width`

- **Feature:** the line around the shape of a drop cap (PXV-06).
- **Value:** a length; none when absent.
- **Fallback:** no line.

#### `desk:drop-cap-border-color`

- **Feature:** the color of the line around the shape of a drop cap
  (PXV-06).
- **Value:** `#rrggbb`; black when absent.
- **Fallback:** no line.

#### `desk:drop-cap-border-style`

- **Feature:** how the line around the shape of a drop cap is drawn
  (PXV-06).
- **Value:** `solid`, `dashed` or `dotted`, as `fo:border` names them;
  solid when absent.
- **Fallback:** no line.

#### `desk:drop-cap-scale`

- **Feature:** how tall the letters of a drop cap are in its shape
  (PXV-06).
- **Value:** a percent of their height without the shape, from 10 % to
  100 %; 80 % when absent.
- **Fallback:** the letters at their full height.

#### `desk:drop-cap-radius`

- **Feature:** how round the corners of the shape of a drop cap are
  (PXV-06).
- **Value:** a percent of half the shorter side, from 0 % (square
  corners) to 100 %; 20 % when absent.
- **Fallback:** no shape.

#### Styles of drop caps (settings, not an attribute)

The styles of drop caps of a document (PXV-06) are not part of its
content, so Desk keeps them where applications keep their own settings:
the `DropCapStyles` item of the `DeskTextSettings` set in `settings.xml`,
written only when they are not the six of a new document. Each line is
one style, in the order of the menu, as words of a name, `=` and a value:
`lines`, `characters`, `raised`, `wrap` (`shape` or `rectangle`),
`distance`, `outdent` (a percent), `weight` (100 to 900) and `color` of
the letters, and, for a style with a shape, `fill`, `border`,
`border-color`, `border-style`, `scale` and `radius`. Other programs
ignore the item; Desk reads a document without it with the six styles.

#### Chart styles (settings, not an attribute)

The chart styles of a document (W080) are not part of its content
either, so Desk keeps them in the `ChartStyles` item of the
`DeskTextSettings` set in `settings.xml`, written only when they are not
the six of a new document. Each line is one style, in the order of the
pictures, as a JSON object with `fills`, the fills of the series in
order, and `text`, the font, size, color, bold and italic of the chart,
written as `desk:chart` writes them. Other programs ignore the item; Desk
reads a document without it, or with no line it can read, with the six
styles.

#### Custom cell formats (settings, not an attribute)

The custom formats of the cells of a document (PXV-18, TAB-31), which
the Data Format menu lists by name, are kept in the `CustomFormats` item
of the `DeskTextSettings` set in `settings.xml`, written only when there
is one. Each line is one format, in the order of the menu, as a JSON
object with `name` and `pattern`, the pattern in the syntax of
spreadsheets that `desk:table` keeps for a cell. A cell keeps its own
copy of its format in `desk:table`, so a format renamed or deleted in the
menu stays on the cells that have it. Other programs ignore the item.

#### Author colours (settings, not an attribute)

The colour each author chose for their comments and highlights (W150),
View > Author Color in Desk Writer as in Pages 15.4, is kept in the
`AuthorColors` item of the `DeskTextSettings` set in `settings.xml`,
written only when an author chose one. Each line is one author, as a
JSON object with `author`, the name as comments carry it in `dc:creator`,
and `color`, one of `yellow`, `green`, `blue`, `pink`, `purple`, `teal`,
`orange`, `red`, `brown` and `gray-blue`. An author without a line takes
the colours no author chose, in the order of the menu: the user first,
which is yellow unless someone chose it, then the others by name, an
order that saving and opening again keeps.
Other programs ignore the item and colour comments their own way.

#### Bookmark suggestions (settings, not an attribute)

The paragraph styles whose paragraphs a link to a bookmark suggests
(W101), Manage Bookmark Suggestions of Pages 15.4, are kept in the
`BookmarkSuggestions` item of the `DeskTextSettings` set in
`settings.xml`, written once they are chosen: the name of each style, one
a line, as the style is named in `styles.xml`; an empty item when none
is. A document without the item suggests the paragraphs of Heading 2 and
Heading 3, as a new document of Pages does. Other programs ignore it.

#### Tracking paused (settings, not an attribute)

Whether the tracking of changes is paused (W148), as the Pause switch of
the change tracking bar leaves it in Desk Writer as in Pages 15.4, is
kept in the `TrackingPaused` item of the `DeskTextSettings` set in
`settings.xml`, written only when it is. ODF has only on and off:
`text:track-changes` of `text:tracked-changes` says `true` while changes
are tracked and `false` while tracking is paused, so that other programs
make changes as they are then, as Desk does, and keep those already
tracked. Other programs ignore the item, and a file they save tracks
changes again or not as their own setting says.

#### Markup view (settings, not an attribute)

How tracked changes show (W148, REV-04), as View > Change Markup of
Desk Writer and the review toolbar of Pages 15.4 set it, is kept in the
`DeskMarkupView` item of the view settings in `settings.xml`, beside the
zoom and the outline: `without-deletions` hides the text deleted, and
`final` shows the text as if every change were accepted, without marks.
It is written only when it is not the markup, which is what a file
without it shows. Other programs ignore the item and show changes as
their own setting says.

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

### Text: headers and footers (`style:header`, `style:footer`)

#### `desk:header-field`

- **Feature:** the centre and right fields of a header or footer of
  Pages 15.4, each a third of the width of the text (W096); the left
  field alone is the free header of Pages '09.
- **On:** `text:p` and `text:h` of a `style:header`, `style:footer` or
  their first and left variants, after the paragraphs of the left field:
  those of the centre field, then those of the right one.
- **Value:** `center` or `right`.
- **Fallback:** the paragraphs themselves, below those of the left field,
  centred and aligned right by their own paragraph properties: the text
  stays, stacked instead of side by side.

### Text: tables of contents (`text:table-of-content`)

#### `desk:toc-scope`

- **Feature:** which paragraphs a table of contents lists, as Insert >
  Table of Contents and the Range of the Format pane of Pages 15.4
  (W100): those of its section, or those after it up to the next table,
  To Next Occurrence, which every table of Pages '09 lists.
- **On:** `text:table-of-content`.
- **Value:** `document`, `section` or `next`.
- **Fallback:** the entries themselves, which other programs show as
  written; when they update the table, it lists the whole document, as
  `text:index-scope` says by default. A table without the attribute is
  read as one of the whole document, unless `meta:generator` says Desk
  wrote it, before tables had a range: then it lists up to the next
  table, as it did.

### Page backgrounds (`style:drawing-page-properties`, `style:master-page`)

The background of the pages of a section (W098) is the `draw:fill` of the
`style:drawing-page-properties` of the drawing page style that its master
page names in `draw:style-name`, as ODF 1.3 gives it, with
`draw:background-size="full"`. Each master page with a background also
has a page layout of its own, with `fo:background-color` beside a color
for older readers; Desk reads a `draw:fill` of the page layout too, as
older programs wrote it. A picture that fills the pages takes
`desk:fill-image-mode`, `desk:fill-tint` and `desk:fill-tint-opacity` as
a graphic style does.

#### `desk:match-previous`

- **Feature:** a section with a background of its own and the headers,
  footers and master objects of the section before: "Match previous
  section" of Pages 15.4 (W098).
- **On:** `style:master-page`.
- **Value:** a boolean.
- **Fallback:** the master page holds copies of the headers, footers and
  master objects of the section before, so other programs show the same
  pages.

### Text: comments (`office:annotation`)

A comment is the standard `office:annotation` where the text it is on
starts, with its `office:name`, `dc:creator` and `dc:date`, and the
`office:annotation-end` of that name where the text ends; its text is a
`text:p` for each line, since comments take no styles in Pages 15.4.
A highlight is an annotation whose one paragraph is empty (PXV-19).

#### `desk:parent-name`

- **Feature:** a reply in the thread of a comment (W149, REV-08).
- **On:** `office:annotation`.
- **Value:** the `office:name` of the comment it answers. The reply is an
  annotation of its own on the same text, named, with its own
  `office:annotation-end`, written after its comment. Desk also reads
  `loext:parent-name`, which LibreOffice writes the same way.
- **Fallback:** the replies are comments of their own, on the same text,
  one after the other, as a reader without the extension shows them.
  It is also written on `desk:annotation`.

#### `desk:style-name`

- **Feature:** the formatting that text had before a tracked change of
  its formatting, which rejecting the change puts back (W148, REV-01).
- **On:** `text:format-change`, in a `text:changed-region` of
  `text:tracked-changes`.
- **Value:** the name of a text style of `content.xml`, common or
  automatic, whose properties are the formatting the text between the
  marks of the region had of its own, over its paragraph, with the
  common style it had as its parent; empty when that text had no
  formatting of its own. Text whose formatting before differs is
  written in regions of its own.
- **Fallback:** the change of formatting, as LibreOffice writes it,
  without what the text was before; rejecting it there keeps the
  formatting as it is.

#### `desk:paragraph-style-name`

- **Feature:** the style and the formatting that paragraphs had before
  a tracked change of their formatting, which rejecting the change puts
  back (W148, REV-01).
- **On:** `text:format-change`, in a `text:changed-region` of
  `text:tracked-changes`, whose marks are at the start and the end of
  the text of the paragraphs it changed. Those of a paragraph that a
  tracked deletion joins to another are around the text it shows, out
  of the `text:deletion`, and those of a paragraph the deletion took
  whole are in its paragraph in the `text:deletion` (B-069).
- **Value:** the name of a paragraph style of `content.xml`, common or
  automatic, whose properties are the formatting the paragraphs had of
  their own, with the common style they had as its parent, or empty when
  that is not known. Its presence makes the region a change of
  paragraphs rather than of their text.
- **Fallback:** a change of formatting of the text of the paragraphs, as
  other programs read it, without what the paragraphs were before.

#### Changes inside deleted text (no attribute of its own)

Text that a tracked deletion took from another change, an insertion or
a change of formatting of another author, keeps that change, so that
rejecting the deletion gives the text back to it (W148, B-070). Desk
writes the `text:change-start` and `text:change-end` marks of that
change inside the paragraphs of the `text:deletion` that hold the text,
with a `text:changed-region` of its own, and reads them back from
there. A change of formatting that goes on past a break the deletion
took goes on in the next of its paragraphs, as no break is in one. An
insertion goes on past the break only when the break was in it too,
which rejecting the deletion gives back to it (B-072): reading, an
insertion open at the end of one of those paragraphs holds its break,
and one that holds the text before the break and not the break ends
before it and starts again after it. A change of formatting starts again
where the formatting before it differs, as outside a deletion. ODF lets
a deletion keep any content of a paragraph; other
programs that do not look for marks there read the text as deleted
alone, and the region as a change that holds no text.

#### `desk:change-name`

- **Feature:** the regions of one tracked change (W148, B-073). ODF
  names one `text:changed-region` for each pair of marks, so the pieces
  of a change that another interrupts, as the text of an insertion
  before and after a deletion in it, are regions of their own.
- **On:** `text:insertion`, `text:deletion` and `text:format-change`.
  Desk writes the same name on the regions of a change that has more
  than one, and reads the regions that share a name and a kind as one
  change. The region of a change of cells (REV-05) always has it, as
  the cells in `desk:table` name their change by it.
- **Fallback:** a change of its own for each region, as other programs
  read them. Regions without the name, as other programs write them,
  are changes of their own even when their author and date are the
  same, as different changes can be.

#### `desk:annotation`

- **Feature:** a comment on an object itself, as Pages 15.4 puts one on
  a selected shape, picture, text box or table (W149, REV-08), and a
  reply to the comment of a table cell (W149, TAB-27).
- **On:** any drawing object written by `write_draw_object`, as its
  child, one for the comment and one after it for each reply. ODF has
  `office:annotation` only in paragraphs and table cells. In a
  `table:table-cell`, after its `office:annotation`, one for each reply
  to it, since a cell has one `office:annotation`.
- **Value:** what an `office:annotation` holds: `dc:creator`, `dc:date`
  and a `text:p` for each line, named in `desk:name`, with the reply
  naming its comment in `desk:parent-name`.
- **Fallback:** none on an object; other programs have no comments on
  objects, and show the object without it. In a cell, other programs
  show the comment, its `office:annotation`, without the replies.

#### `desk:name`

- **Feature:** the name of a comment on an object, which its replies
  name (W149), and the name of a page template (W095).
- **On:** `desk:annotation` and `desk:page-template`.
- **Value:** a name unique in the document for a comment, and among the
  templates for a template. Desk reads at most 256 characters of the
  name of a template, and the first template of a name.
- **Fallback:** none, as for `desk:annotation` and `desk:page-template`.

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

#### `desk:page-number-format`

- **Feature:** how the numbers of the pages of a section are written, the
  Format of Page Numbering in the Section tab of Pages 15.4 (EST-18,
  W094): a page number field in digits shows it.
- **Value:** a `style:num-format`: `1`, `i`, `I`, `a` or `A`.
- **Fallback:** a section whose format differs from that of the master
  page before it gets a master page of its own, whose page layout has the
  same `style:num-format`, which page number fields show in other
  programs. Desk reads that too, from documents of other programs.

#### `desk:hide-first-page`

- **Feature:** a section whose first page shows no header and footer:
  Hide on first page of section of Pages 15.4 (EST-19, W094).
- **Value:** a boolean.
- **Fallback:** none; other programs show the headers and footers on the
  first page too.

### Captured pages (`Desk/Captures/`)

The pages captured to insert again (EST-20), Capture Pages of Pages '09
kept by D-003, are each a text document of their own in the package, at
`Desk/Captures/1.odt`, `Desk/Captures/2.odt` and on, listed in the
manifest with the media type of a text document. Each holds the styles
of the document, an empty first section and the sections captured after
it, which keep their headers and footers as sections of their own, and
its `dc:title` is the name they go by. A package keeps at most 64. Desk reads them in the order of their numbers, with the
styles of the document of the same names; other programs leave them
out, and a package whose captured pages are lost still opens.

### Page templates (`desk:page-templates`)

The page templates of a page layout document (W095, PXV-20), as Pages
15.4 has them, are in one `desk:page-templates` in `office:text`, after
the objects anchored to pages and before the first paragraph. ODF master
pages hold what each page shows behind its own objects, not a page to
add whose objects become those of the page, so they cannot hold one. The
objects of a page that came from a template are ordinary objects of its
page, which carry the attributes below.

#### `desk:page-templates`

- **Feature:** the page templates of a page layout document (W095).
- **On:** `office:text`, as its child; one at most.
- **Value:** a `desk:page-template` for each template, in the order of
  their list. Desk reads at most 1000.
- **Fallback:** none; other programs leave it out with the objects in
  it, and show the pages with the objects that came from templates as
  ordinary objects.

#### `desk:page-template`

- **Feature:** one page template (W095).
- **On:** `desk:page-templates`, as its child.
- **Value:** its name in `desk:name`, the pages that come from it in
  `desk:pages`, and its objects as drawing objects anchored to page 1,
  each with an `xml:id` that the objects copied from it name.
- **Fallback:** none, as for `desk:page-templates`.

#### `desk:pages`

- **Feature:** the pages whose template a page template is (W095).
- **On:** `desk:page-template`.
- **Value:** the numbers of the pages from 1, apart by spaces; numbers
  past the last page or past 10000 are ignored, and a page named by two
  templates takes the last.
- **Fallback:** none, as for `desk:page-templates`.

#### `desk:placeholder`

- **Feature:** a placeholder of a page template, Define as Placeholder
  Text and Define as Media Placeholder of Pages 15.4 (W095), whose
  content a page keeps when it takes the template again.
- **On:** any drawing object written by `write_draw_object`, of a
  template or of a page.
- **Value:** `text` or `media`.
- **Fallback:** the object, with its content, as an ordinary object.

#### `desk:placeholder-tag`

- **Feature:** the tag of a placeholder, which carries its content to the
  placeholder of the same tag of another template (W095).
- **On:** an object with `desk:placeholder`.
- **Value:** the tag; Desk reads at most 256 characters.
- **Fallback:** none; the tag has no meaning without the placeholder.

#### `desk:template-object`

- **Feature:** the object of the page template that an object of a page
  copies, which Reapply Page Template puts back (W095).
- **On:** a drawing object anchored to a page.
- **Value:** the `xml:id` of an object of a `desk:page-template`; one
  that names none is ignored.
- **Fallback:** the object, as an ordinary object of its page.

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
| Placeholder text and media placeholders | All | ODF has a placeholder field for text only | Ordinary content |
| Replies of comments in Tables and Slides | Tables, Slides | ODF 1.3 annotations have no reply relation; Writer uses `desk:parent-name` (above) | Flat annotations, in order |
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
