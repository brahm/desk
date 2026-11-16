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

Read from `crates/desk-odf/src` on 2026-11-16: 43 names. The
description is the first sentence of the comment in the source file; the
full definition (values, fallback) is still to be written here for each
(backlog B-008).

| Name | Source file | What it holds |
| --- | --- | --- |
| `desk:alpha-kept` | `draw.rs` | See the source file |
| `desk:bend` | `draw.rs` | how far its middle is pulled aside, to the left of the way from start to end, as Desk bends it. |
| `desk:blend` | `draw.rs` | where it is half blended into the next color, from 0 to 1 of the way to it. |
| `desk:chart` | `draw.rs` | all that Desk keeps of a chart, which the object shows to other programs. |
| `desk:color` | `draw.rs` | See the source file |
| `desk:default-object-style` | `draw.rs` | See the source file |
| `desk:duration` | `draw.rs` | how long it lasts, in seconds, as Desk found. |
| `desk:fill-image-mode` | `draw.rs` | See the source file |
| `desk:fill-tint` | `draw.rs` | the color laid over a picture that fills the object (OBJ-42). |
| `desk:fill-tint-opacity` | `draw.rs` | See the source file |
| `desk:first-page-different` | `text.rs` | See the source file |
| `desk:gradient-stop` | `draw.rs` | children: the colors of an advanced gradient of Desk, between the start color and the end color (OBJ-41). |
| `desk:hyphenate` | `text.rs` | See the source file |
| `desk:image-adjust` | `draw.rs` | the changes of Adjust Image, as six numbers from -100 to 100: brightness, contrast, saturation, exposure, temperature and tint. |
| `desk:instant-alpha` | `draw.rs` | See the source file |
| `desk:left-right-different` | `text.rs` | See the source file |
| `desk:mask` | `draw.rs` | See the source file |
| `desk:mask-shape` | `draw.rs` | See the source file |
| `desk:master-pages` | `draw.rs` | See the source file |
| `desk:media-start` | `draw.rs` | See the source file |
| `desk:media-stop` | `draw.rs` | See the source file |
| `desk:new-page` | `text.rs` | whether the section starts on a new page. |
| `desk:object-style` | `draw.rs` | See the source file |
| `desk:opacity` | `draw.rs` | See the source file |
| `desk:origin` | `draw.rs` | what the copies of one object share, which Magic Move of Desk Slides matches (IOP-33). |
| `desk:pattern` | `text.rs` | keeps the pattern of the date in the symbols of Unicode. |
| `desk:picture-frame` | `draw.rs` | the picture frame of the library of Desk around the object, by its name. |
| `desk:picture-frame-scale` | `draw.rs` | how thick the picture frame is, in percent. |
| `desk:poster-time` | `draw.rs` | See the source file |
| `desk:raise` | `text.rs` | how far the picture is raised from where it stands, lowered when negative, which ODF has no attribute for. |
| `desk:reflection` | `draw.rs` | how opaque the reflection below the object is where it starts, in percent (OBJ-38). |
| `desk:relative-size` | `text.rs` | See the source file |
| `desk:repeat` | `draw.rs` | where it starts and stops and the poster frame was taken, in seconds, and how it repeats, as Desk plays it. |
| `desk:section-start` | `text.rs` | the page that a section of Pages starts on, when the section is one. |
| `desk:shadow-blur` | `draw.rs` | See the source file |
| `desk:shadow-opacity` | `text.rs` | See the source file |
| `desk:shape` | `draw.rs` | the kind and settings of a shape of Desk, which the path draws for other programs. |
| `desk:space-after` | `text.rs` | the space below the section. |
| `desk:space-before` | `text.rs` | the space above the section. |
| `desk:stroke-style` | `draw.rs` | See the source file |
| `desk:table` | `table.rs` | all that Desk keeps of the table, which the rest shows to other programs. |
| `desk:tracking` | `text.rs` | See the source file |
| `desk:wrap-threshold` | `draw.rs` | how opaque a pixel of a picture must be, in percent, for the text that wraps tightly to go around it. |

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
