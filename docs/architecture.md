# Architecture

How Desk is put together: who uses it and what it talks to, the crates and
what each may depend on, the document model, the life of a document and
the path of an edit. Diagrams are Mermaid, so GitHub draws them and an
agent can read and change them as text.

Each section says whether it describes **today** (commit `0888aa9`,
2026-11-16) or the **target** of [goals.md](goals.md). Where they differ,
the roadmap phase that closes the gap is named.

## 1. Context

Today and target. Desk is three desktop apps and a command-line converter.
It has no server and no account: everything in scope works offline (D-004).

```mermaid
flowchart LR
    user([Person writing, calculating or presenting])
    subgraph desk [Desk]
        writer[Desk Writer]
        tables[Desk Tables]
        slides[Desk Slides]
        convert[desk-convert]
    end
    files[(Documents on disk<br>ODF with desk: extensions)]
    foreign[(Files of other suites<br>iWork '09, iWork 15,<br>Microsoft Office, LibreOffice)]
    out[(PDF, EPUB, Office files)]
    gnome[GNOME platform<br>portals, printing, fonts,<br>spelling, screen reader]
    user --> writer & tables & slides
    writer & tables & slides <--> files
    foreign -- import --> writer & tables & slides
    writer & tables & slides -- export --> out
    convert --> out
    writer & tables & slides <--> gnome
```

## 2. Use cases

Target. The use cases every app shares; each app adds its own (sections,
formulas, builds) in its matrix.

```mermaid
flowchart TB
    author([Author])
    reviewer([Reviewer])
    presenter([Presenter])
    subgraph Desk
        create(Create from a template)
        open(Open or import a file)
        edit(Edit text, tables, charts and objects)
        style(Format with styles and the Inspector)
        versions(Browse and restore versions)
        review(Comment and track changes)
        export(Export or print)
        present(Play a presentation)
    end
    author --> create & open & edit & style & versions & export
    reviewer --> open & review
    presenter --> open & present
    edit -. saved automatically .-> versions
```

## 3. Crates

Today. One Cargo workspace; an arrow means "depends on". The CI enforces
two rules (`ci/check-deps.sh`): only `desk-ui` and the three apps use GTK,
and the MPL-2.0 libraries (`desk-odf`, `desk-iwork`, `desk-formula`) use no
other Desk crate, so that other programs can reuse them.

```mermaid
flowchart TB
    subgraph apps [Apps: GTK]
        writer[desk-writer]
        tables[desk-tables]
        slides[desk-slides]
    end
    ui[desk-ui<br>windows, panes, editing]
    convert[desk-convert<br>CLI, no GTK]
    testkit[desk-testkit<br>corpus and parity report]
    subgraph engine [Engine: no GTK]
        render[desk-render<br>display list, Cairo]
        layout[desk-layout<br>pagination]
        text[desk-text<br>shaping, line breaking]
        model[desk-model<br>document, edits, undo]
        chart[desk-chart]
        math[desk-math]
        table[desk-table]
        anim[desk-anim]
        lang[desk-lang<br>spelling, hyphenation]
        contacts[desk-contacts]
    end
    subgraph formats [File formats]
        filters[desk-filters<br>model to and from files]
        odf[desk-odf · MPL]
        iwork[desk-iwork · MPL]
        formula[desk-formula · MPL]
    end
    writer & tables & slides --> ui & filters
    writer & tables --> contacts
    slides --> anim
    ui --> render & layout & text & model & lang
    convert --> filters & render & layout
    testkit --> filters & render & layout
    render --> layout --> text --> model
    render & layout --> math
    text --> lang
    model --> chart & table
    chart --> table --> formula
    anim --> model
    filters --> model & odf & iwork & anim & chart & table & formula
```

| Layer | Rule |
| --- | --- |
| File format libraries | Know XML, ZIP and the format. They do not know the model. |
| `desk-filters` | The only place that turns a format into the model and back. Every import returns a log of what it could not keep. |
| `desk-model` | Holds every change as an operation with an inverse. Nothing outside it mutates a document. |
| `desk-text`, `desk-layout`, `desk-render` | Pure functions of the model: the same document gives the same pages, on screen, in PDF and in tests. |
| `desk-ui` | The only library that uses GTK. Widgets read the model and send edits; they hold no document state of their own. |

What the target adds, each in the phase named in [roadmap.md](roadmap.md):

| Addition | Where it goes |
| --- | --- |
| Readers of current iWork files | A new MPL crate beside `desk-iwork`, under the rules of D-011 |
| Readers and writers of `.docx`, `.xlsx`, `.pptx` | A new MPL crate for OOXML packages, and filters in `desk-filters` |
| ODS and ODP | `desk-odf` and `desk-filters`, with the sheet and slide containers of the model |
| Automatic saving and versions | `desk-model` snapshots, a store in `desk-filters`, the interface in `desk-ui`; replaces `recovery.rs` |
| Document tabs, the M+ window | `desk-ui`; replaces `format_bar.rs`, `bottom_bar.rs` and the sheet list of the sidebar |
| Live links between documents | `desk-model` (a link node with a source and a range), refreshed by `desk-filters` |
| Transition and build engine | `desk-anim`, today a stub |

## 4. Document model

Today for Writer; the sheet and slide containers are the target. One model
serves the three apps, so an object copied from one to another stays the
same object.

```mermaid
classDiagram
    class Document {
        Metadata metadata
        Styles styles
        Container container
    }
    class Container {
        <<enumeration>>
        Pages
        Sheets (target)
        Slides (target)
    }
    class PagedFlow {
        PageSetup setup
        TextFlow body
        Objects objects
    }
    class TextFlow
    class Paragraph {
        ParaAttrs attrs
    }
    class Run {
        String text
        CharAttrs attrs
    }
    class Styles {
        paragraph styles
        character styles
        list styles
        object styles
    }
    class Object {
        NodeId id
        geometry
        fill, stroke, effects
    }
    class Table
    class Chart
    class Editor {
        undo history
        edit(name, changes)
        undo()
        redo()
    }
    class Transaction {
        name
        Op[] ops
    }
    Document *-- Styles
    Document *-- Container
    Container <|-- PagedFlow
    PagedFlow *-- TextFlow
    PagedFlow *-- "0..*" Object
    TextFlow *-- "1..*" Paragraph
    Paragraph *-- "0..*" Run
    Object <|-- Table
    Object <|-- Chart
    Object *-- "0..1" TextFlow : text inside
    Chart ..> Table : reads data
    Editor o-- Document
    Editor *-- "0..*" Transaction
```

- Every node has a stable `NodeId`.
- Formatting is set as optional attributes on styles and on the text and
  resolved through the inheritance of the styles.
- Cloning a `Document` is cheap and gives a snapshot, because the content
  lives in persistent structures. Versions (target) are snapshots kept on
  disk.

## 5. Life of a document

Target (D-001: saving is automatic, with versions). Today the app saves on
command and keeps a recovery draft; the base phase of the roadmap replaces
that.

```mermaid
stateDiagram-v2
    [*] --> Untitled : new from a template
    [*] --> Imported : open a file of another suite
    [*] --> Saved : open an ODF document
    Untitled --> Saved : first save names the file
    Imported --> Saved : save as ODF
    Imported --> Imported : edit (kept in the app's own draft store)
    Saved --> Dirty : edit
    Dirty --> Saved : automatic save after a pause
    Saved --> Browsing : browse versions
    Browsing --> Saved : restore a version or leave
    Saved --> [*] : close
    Dirty --> Saved : close (saves first)
```

## 6. From a file to the screen, and an edit

Today.

```mermaid
sequenceDiagram
    participant UI as desk-ui
    participant F as desk-filters
    participant L as format library
    participant M as desk-model
    participant Y as desk-layout
    participant R as desk-render
    UI->>F: open(path)
    F->>L: read package
    L-->>F: parts of the file
    F-->>UI: Document and import log
    UI->>M: Editor::new(document)
    UI->>Y: lay out
    Y-->>UI: pages
    UI->>R: draw visible pages
    Note over UI,M: the person types
    UI->>M: editor.edit("Typing", ops)
    M-->>UI: transaction (changed nodes)
    UI->>Y: lay out again from the changed paragraph
    UI->>R: redraw
```

## 7. An edit, step by step

Today. Every change, from a key press to a paste from another app, takes
this path, which is what makes undo complete.

```mermaid
flowchart TD
    a[Input: key, pointer, menu, paste] --> b[desk-ui turns it into a named edit]
    b --> c{Valid for the selection?}
    c -- no --> z[Nothing changes]
    c -- yes --> d[Editor applies the operations and records their inverses]
    d --> e[Transaction pushed on the undo history]
    e --> f[Layout updates from the first changed node]
    f --> g[Widgets redraw, and the accessibility tree is updated]
    g --> h[Automatic save is scheduled · target]
```

## 8. Distribution

Today. Flatpak on the GNOME 51 runtime is the supported way to install
Desk (D-013).

```mermaid
flowchart LR
    main[main branch] --> ci[CI: ci/check-all.sh<br>format, Clippy, tests, UI tests,<br>ODF validation, licenses, REUSE]
    ci --> nightly[Flatpak nightly branch<br>built every night]
    ci --> beta[Flatpak beta branch]
    nightly & beta --> repo[(Flatpak repository<br>brahm.github.io/desk)]
    repo --> user([flatpak install])
    runtime[org.gnome.Platform 51<br>GTK 4.24, Libadwaita 1.10] --> user
```

## Where to read next

- What each feature must do: [`corpus/inventory`](../corpus/inventory/README.md)
  and the parity matrices in [`corpus`](../corpus/README.md).
- Where each control sits in the window: [`interface`](interface/README.md)
  and the chosen proposal in [`interface/proposals`](interface/proposals/README.md).
- How a feature is stored: [odf-extensions.md](odf-extensions.md).
- What comes when: [roadmap.md](roadmap.md).
