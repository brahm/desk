// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Writing a text document as XML streams.

use std::collections::BTreeSet;

use crate::ns;
use crate::text::{
    Background, Border, BorderLine, Break, Columns, Family, Field, HeaderFooter, HeaderFooterStyle,
    LINE_BREAK, LeaderStyle, Line, LineColor, LineHeight, ListLabel, ListMember, ListStyle, Meta,
    PageLayout, Paragraph, ParagraphProperties, Properties, SectionStart, SectionStyle, Style,
    TabType, TextAlign, TextDocument, TextPosition, TextProperties, TextShadow, TextTransform,
    TocLevel,
};
use crate::units::{Length, decimal};
use crate::xml::Writer;
use crate::{MasterPage, PageFill};

/// The ODF version written.
pub const VERSION: &str = "1.3";

/// The XML streams of a text document.
#[derive(Clone, Debug)]
pub struct Streams {
    /// `content.xml`.
    pub content: String,
    /// `styles.xml`.
    pub styles: String,
    /// `meta.xml`.
    pub meta: String,
    /// `settings.xml`, when there are settings to keep.
    pub settings: Option<String>,
}

fn root(w: &mut Writer, name: &'static str) {
    w.start(name)
        .attr("xmlns:office", ns::OFFICE)
        .attr("xmlns:style", ns::STYLE)
        .attr("xmlns:text", ns::TEXT)
        .attr("xmlns:fo", ns::FO)
        .attr("xmlns:svg", ns::SVG)
        .attr("xmlns:draw", ns::DRAW)
        .attr("xmlns:meta", ns::META)
        .attr("xmlns:dc", ns::DC)
        .attr("xmlns:xlink", ns::XLINK)
        .attr("xmlns:table", ns::TABLE)
        .attr("xmlns:chart", ns::CHART)
        .attr("xmlns:desk", ns::DESK)
        .attr("office:version", VERSION);
}

/// Writes the streams of a text document.
pub fn write_streams(document: &TextDocument) -> Streams {
    let fonts: BTreeSet<&str> = std::iter::once(&document.default_style)
        .chain(
            document
                .styles
                .iter()
                .chain(&document.automatic_styles)
                .map(|s| &s.properties),
        )
        .filter_map(|p| p.text.font_family.as_deref())
        .collect();

    let mut w = Writer::new();
    root(&mut w, "office:document-content");
    font_faces(&mut w, &fonts);
    w.start("office:automatic-styles");
    for style in &document.automatic_styles {
        write_style(&mut w, style);
    }
    for style in &document.automatic_list_styles {
        write_list_style(&mut w, style);
    }
    for style in &document.section_styles {
        write_section_style(&mut w, style);
    }
    w.end();
    w.start("office:body").start("office:text");
    // Sections and tables of contents are numbered through the text and
    // the paragraphs that deletions took, so that their names differ.
    let mut names = Names::default();
    write_tracked_changes(&mut w, document, &mut names);
    for object in &document.page_objects {
        crate::draw::write_draw_object(&mut w, object, &write_paragraph);
    }
    write_body(&mut w, &document.body, &document.toc_levels, &mut names);
    w.end().end().end();
    let content = w.finish();

    let mut w = Writer::new();
    root(&mut w, "office:document-styles");
    font_faces(&mut w, &fonts);
    w.start("office:styles");
    w.start("style:default-style")
        .attr("style:family", "paragraph");
    write_properties(&mut w, &document.default_style);
    w.end();
    for style in &document.styles {
        write_style(&mut w, style);
    }
    for style in &document.list_styles {
        write_list_style(&mut w, style);
    }
    for template in &document.table_templates {
        crate::table::write_table_template(&mut w, template);
    }
    for gradient in &document.gradients {
        crate::draw::write_gradient(&mut w, gradient);
    }
    for marker in &document.markers {
        crate::draw::write_marker(&mut w, marker);
    }
    for image in &document.fill_images {
        crate::draw::write_fill_image(&mut w, image);
    }
    for notes in &document.notes_configuration {
        w.start("text:notes-configuration")
            .attr("text:note-class", &notes.class);
        if let Some(format) = &notes.num_format {
            w.attr("style:num-format", format);
        }
        if let Some(at) = &notes.start_numbering_at {
            w.attr("text:start-numbering-at", at);
        }
        if let Some(position) = &notes.footnotes_position {
            w.attr("text:footnotes-position", position);
        }
        w.end();
    }
    w.end();
    w.start("office:automatic-styles");
    for style in &document.master_styles {
        write_style(&mut w, style);
    }
    let masters: Vec<(&str, &MasterPage)> = std::iter::once(("Standard", &document.master))
        .chain(document.masters.iter().map(|(n, m)| (n.as_str(), m)))
        .collect();
    // A master page with a background fills its pages with a drawing
    // page style, as ODF 1.3 does, and has a page layout of its own with
    // the color for older programs (W098).
    let mut layouts = Vec::new();
    write_page_layout(&mut w, "pm1", &document.page_layout, None);
    for (_, master) in &masters {
        let names = match &master.background {
            Some(fill) => {
                let n = layouts.len() + 2;
                let layout = format!("pm{n}");
                write_page_layout(&mut w, &layout, &document.page_layout, Some(fill));
                let page = format!("Mdp{n}");
                write_drawing_page_style(&mut w, &page, fill);
                (layout, Some(page))
            }
            None => ("pm1".to_owned(), None),
        };
        layouts.push(names);
    }
    w.end();
    w.start("office:master-styles");
    for ((name, master), (layout, page)) in masters.into_iter().zip(&layouts) {
        w.start("style:master-page")
            .attr("style:name", name)
            .attr("style:page-layout-name", layout);
        w.attr_opt("draw:style-name", page.as_deref());
        if master.match_previous {
            w.attr("desk:match-previous", "true");
        }
        for (name, part) in [
            ("style:header", &master.header),
            ("style:header-left", &master.header_left),
            ("style:header-first", &master.header_first),
            ("style:footer", &master.footer),
            ("style:footer-left", &master.footer_left),
            ("style:footer-first", &master.footer_first),
        ] {
            if let Some(part) = part {
                write_header_footer(&mut w, name, part);
            }
        }
        for object in &master.objects {
            crate::draw::write_draw_object(&mut w, object, &write_paragraph);
        }
        w.end();
    }
    w.end();
    w.end();
    let styles = w.finish();

    Streams {
        content,
        styles,
        meta: write_meta(&document.meta),
        settings: write_settings(document),
    }
}

/// The name of the settings that Desk keeps the shortcuts of styles in.
pub(crate) const SHORTCUTS: &str = "DeskStyleShortcuts";

/// The name of the settings that Desk keeps the settings of the text in.
pub(crate) const TEXT_SETTINGS: &str = "DeskTextSettings";

fn config_item(w: &mut Writer, name: &str, kind: &str, value: &str) {
    w.start("config:config-item")
        .attr("config:name", name)
        .attr("config:type", kind)
        .text(value)
        .end();
}

/// Writes the settings: the shortcuts of styles, as one item per function
/// key holding the family and the name of the style, as in
/// `paragraph:Heading_20_1`, and the settings of the text.
fn write_settings(document: &TextDocument) -> Option<String> {
    let text = &document.text_settings;
    let has_text = *text != Default::default();
    let view = &document.view;
    if document.style_shortcuts.is_empty() && !has_text && view.is_empty() {
        return None;
    }
    let mut w = Writer::new();
    w.start("office:document-settings")
        .attr("xmlns:office", ns::OFFICE)
        .attr("xmlns:config", ns::CONFIG)
        .attr("office:version", VERSION);
    w.start("office:settings");
    if !view.is_empty() {
        // As LibreOffice writes them, which it reads back.
        w.start("config:config-item-set")
            .attr("config:name", "ooo:view-settings");
        w.start("config:config-item-map-indexed")
            .attr("config:name", "Views");
        w.start("config:config-item-map-entry");
        config_item(&mut w, "ViewId", "string", "view2");
        for (name, value) in [
            ("ZoomType", view.zoom_type),
            ("ZoomFactor", view.zoom_factor),
            ("ViewLayoutColumns", view.columns),
        ] {
            if let Some(value) = value {
                config_item(&mut w, name, "short", &value.to_string());
            }
        }
        if view.columns.is_some() {
            config_item(&mut w, "ViewLayoutBookMode", "boolean", "false");
        }
        for (name, value) in [
            ("DeskOutlineView", view.outline),
            ("DeskOutlineFirstLine", view.outline_first_line),
            ("DeskOutlineThumbnails", view.outline_thumbnails),
        ] {
            if let Some(value) = value {
                let value = if value { "true" } else { "false" };
                config_item(&mut w, name, "boolean", value);
            }
        }
        if let Some(levels) = view.outline_levels {
            config_item(&mut w, "DeskOutlineLevels", "short", &levels.to_string());
        }
        if let Some(markup) = &view.markup {
            config_item(&mut w, "DeskMarkupView", "string", markup);
        }
        w.end().end().end();
    }
    if has_text {
        w.start("config:config-item-set")
            .attr("config:name", TEXT_SETTINGS);
        let boolean = |b: bool| if b { "true" } else { "false" };
        if let Some(hyphenation) = text.hyphenation {
            config_item(&mut w, "Hyphenation", "boolean", boolean(hyphenation));
        }
        if let Some(ligatures) = text.ligatures {
            config_item(&mut w, "Ligatures", "boolean", boolean(ligatures));
        }
        if let Some(larger) = text.larger_spacing {
            config_item(&mut w, "LargerSpacing", "boolean", boolean(larger));
        }
        for (name, value) in [
            ("NoteKind", &text.note_kind),
            ("NoteFormat", &text.note_format),
            ("NoteNumbering", &text.note_numbering),
            ("NoteGap", &text.note_gap),
            ("HeaderMargin", &text.header_margin),
            ("FooterMargin", &text.footer_margin),
        ] {
            if let Some(value) = value {
                config_item(&mut w, name, "string", value);
            }
        }
        if let Some(inactive) = text.links_inactive {
            config_item(&mut w, "LinksInactive", "boolean", boolean(inactive));
        }
        if text.tracking_paused {
            config_item(&mut w, "TrackingPaused", "boolean", "true");
        }
        if let Some(pages) = text.layout_pages {
            config_item(&mut w, "PageLayoutPages", "int", &pages.to_string());
        }
        if !text.guides.is_empty() {
            config_item(&mut w, "Guides", "string", &text.guides.join("\n"));
        }
        if let Some(styles) = &text.drop_cap_styles {
            config_item(&mut w, "DropCapStyles", "string", &styles.join("\n"));
        }
        if let Some(styles) = &text.chart_styles {
            config_item(&mut w, "ChartStyles", "string", &styles.join("\n"));
        }
        if !text.custom_formats.is_empty() {
            config_item(
                &mut w,
                "CustomFormats",
                "string",
                &text.custom_formats.join("\n"),
            );
        }
        if !text.author_colors.is_empty() {
            config_item(
                &mut w,
                "AuthorColors",
                "string",
                &text.author_colors.join("\n"),
            );
        }
        if !text.never_hyphenate.is_empty() {
            config_item(
                &mut w,
                "NeverHyphenate",
                "string",
                &text.never_hyphenate.join("\n"),
            );
        }
        w.end();
    }
    if document.style_shortcuts.is_empty() {
        w.end().end();
        return Some(w.finish());
    }
    w.start("config:config-item-set")
        .attr("config:name", SHORTCUTS);
    for shortcut in &document.style_shortcuts {
        let family = match shortcut.family {
            Family::Paragraph => "paragraph",
            Family::Text => "text",
            Family::List => "list",
            Family::Graphic => "graphic",
            Family::Table => "table",
            Family::TableColumn => "table-column",
            Family::TableRow => "table-row",
            Family::TableCell => "table-cell",
        };
        w.start("config:config-item")
            .attr("config:name", &format!("F{}", shortcut.key))
            .attr("config:type", "string")
            .text(&format!("{family}:{}", shortcut.style))
            .end();
    }
    w.end().end().end();
    Some(w.finish())
}

/// A font family as a quoted CSS string.
fn quote(family: &str) -> String {
    let mut out = String::from("'");
    for c in family.chars() {
        if matches!(c, '\'' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

fn font_faces(w: &mut Writer, fonts: &BTreeSet<&str>) {
    w.start("office:font-face-decls");
    for font in fonts {
        w.start("style:font-face")
            .attr("style:name", font)
            .attr("svg:font-family", &quote(font))
            .end();
    }
    w.end();
}

fn write_style(w: &mut Writer, style: &Style) {
    w.start("style:style")
        .attr("style:name", &style.name)
        .attr_opt("style:display-name", style.display_name.as_deref())
        .attr(
            "style:family",
            match style.family {
                Family::Paragraph => "paragraph",
                Family::Graphic => "graphic",
                Family::Table => "table",
                Family::TableColumn => "table-column",
                Family::TableRow => "table-row",
                Family::TableCell => "table-cell",
                // List styles are written as `text:list-style`.
                Family::Text | Family::List => "text",
            },
        )
        .attr_opt("style:parent-style-name", style.parent.as_deref())
        .attr_opt("style:next-style-name", style.next.as_deref())
        .attr_opt("style:list-style-name", style.list_style.as_deref())
        .attr_opt("style:master-page-name", style.master_page.as_deref());
    write_properties(w, &style.properties);
    w.end();
}

fn write_list_style(w: &mut Writer, style: &ListStyle) {
    w.start("text:list-style")
        .attr("style:name", &style.name)
        .attr_opt("style:display-name", style.display_name.as_deref());
    for level in &style.levels {
        match &level.label {
            ListLabel::Bullet(c) => {
                w.start("text:list-level-style-bullet")
                    .attr("text:level", &level.level.to_string())
                    .attr("text:bullet-char", &c.to_string());
                if let Some(size) = level.relative_size {
                    w.attr("text:bullet-relative-size", &format!("{}%", decimal(size)));
                }
            }
            ListLabel::Image { href, raise, .. } => {
                w.start("text:list-level-style-image")
                    .attr("text:level", &level.level.to_string())
                    .attr("xlink:type", "simple")
                    .attr("xlink:href", href)
                    .attr("xlink:show", "embed")
                    .attr("xlink:actuate", "onLoad");
                if let Some(size) = level.relative_size {
                    w.attr("desk:relative-size", &format!("{}%", decimal(size)));
                }
                len(w, "desk:raise", *raise);
            }
            ListLabel::Number {
                format,
                prefix,
                suffix,
                display_levels,
                start,
            } => {
                w.start("text:list-level-style-number")
                    .attr("text:level", &level.level.to_string());
                if !prefix.is_empty() {
                    w.attr("style:num-prefix", prefix);
                }
                if !suffix.is_empty() {
                    w.attr("style:num-suffix", suffix);
                }
                w.attr("style:num-format", format);
                if *display_levels > 1 {
                    w.attr("text:display-levels", &display_levels.to_string());
                }
                if *start != 1 {
                    w.attr("text:start-value", &start.to_string());
                }
            }
        }
        let image = matches!(level.label, ListLabel::Image { .. });
        w.start("style:list-level-properties");
        // A label followed by a space or nothing needs the label alignment
        // mode, which places the label and the text by a margin and a
        // text indent.
        let followed = level
            .followed_by
            .as_deref()
            .filter(|follow| *follow != "listtab");
        if followed.is_some() {
            w.attr("text:list-level-position-and-space-mode", "label-alignment");
        } else {
            len(w, "text:space-before", level.space_before);
            len(w, "text:min-label-width", level.min_label_width);
        }
        if let ListLabel::Image { width, height, .. } = &level.label {
            len(w, "fo:width", *width);
            len(w, "fo:height", *height);
        }
        if let Some(follow) = followed {
            let before = level.space_before.map_or(0.0, |l| l.0);
            let width = level.min_label_width.map_or(0.0, |l| l.0);
            w.start("style:list-level-label-alignment");
            w.attr("text:label-followed-by", follow);
            len(w, "fo:text-indent", Some(Length(-width)));
            len(w, "fo:margin-left", Some(Length(before + width)));
            w.end();
        }
        w.end();
        // A picture has no text properties.
        if !image && level.text != TextProperties::default() {
            w.start("style:text-properties");
            text_attrs(w, &level.text);
            w.end();
        }
        w.end();
    }
    w.end();
}

/// Writes the paragraphs of the body, with neighbors in the same list in
/// one `text:list` and deeper levels in lists nested in its items.
fn write_columns(w: &mut Writer, columns: &Columns) {
    w.start("style:columns")
        .attr("fo:column-count", &columns.count.to_string());
    len(w, "fo:column-gap", columns.gap);
    for column in &columns.columns {
        w.start("style:column")
            .attr("style:rel-width", &format!("{}*", column.rel_width))
            .attr("fo:start-indent", &column.start_indent.to_string())
            .attr("fo:end-indent", &column.end_indent.to_string())
            .end();
    }
    w.end();
}

fn write_section_style(w: &mut Writer, style: &SectionStyle) {
    w.start("style:style")
        .attr("style:name", &style.name)
        .attr("style:family", "section")
        .start("style:section-properties");
    len(w, "fo:margin-left", style.margin_left);
    len(w, "fo:margin-right", style.margin_right);
    len(w, "desk:space-before", style.space_before);
    len(w, "desk:space-after", style.space_after);
    let flag = |w: &mut Writer, name: &str, value: Option<bool>| {
        if let Some(value) = value {
            w.attr(name, if value { "true" } else { "false" });
        }
    };
    flag(w, "desk:new-page", style.new_page);
    flag(w, "text:dont-balance-text-columns", style.dont_balance);
    if let Some(start) = style.section_start {
        w.attr(
            "desk:section-start",
            match start {
                SectionStart::Any => "any",
                SectionStart::Left => "left",
                SectionStart::Right => "right",
            },
        );
    }
    flag(w, "desk:first-page-different", style.first_page_different);
    flag(w, "desk:left-right-different", style.left_right_different);
    if let Some(columns) = &style.columns {
        write_columns(w, columns);
    }
    w.end().end();
}

/// Opens a table of contents, with what it lists.
fn start_toc(w: &mut Writer, number: usize, levels: &[TocLevel]) {
    w.start("text:table-of-content")
        .attr("text:name", &format!("Table of Contents{number}"));
    w.start("text:table-of-content-source")
        .attr("text:outline-level", &levels.len().clamp(1, 10).to_string())
        .attr("text:use-outline-level", "false")
        .attr("text:use-index-source-styles", "true");
    for (i, level) in levels.iter().take(10).enumerate() {
        let number = (i + 1).to_string();
        w.start("text:table-of-content-entry-template")
            .attr("text:outline-level", &number)
            .attr("text:style-name", &level.entry);
        w.start("text:index-entry-text").end();
        if level.page_number {
            w.start("text:index-entry-tab-stop")
                .attr("style:type", "right")
                .end();
            if level.link {
                w.start("text:index-entry-link-start").end();
            }
            w.start("text:index-entry-page-number").end();
            if level.link {
                w.start("text:index-entry-link-end").end();
            }
        }
        w.end();
    }
    for (i, level) in levels.iter().take(10).enumerate() {
        w.start("text:index-source-styles")
            .attr("text:outline-level", &(i + 1).to_string());
        w.start("text:index-source-style")
            .attr("text:style-name", &level.style)
            .end();
        w.end();
    }
    w.end();
    w.start("text:index-body");
}

/// How many sections and tables of contents are written so far.
#[derive(Default)]
struct Names {
    sections: usize,
    tocs: usize,
}

fn write_body(w: &mut Writer, body: &[Paragraph], toc_levels: &[TocLevel], names: &mut Names) {
    // For each open list, from the outermost, whether an item is open.
    let mut open: Vec<bool> = Vec::new();
    let mut style: Option<&str> = None;
    let close_to = |w: &mut Writer, open: &mut Vec<bool>, depth: usize| {
        while open.len() > depth {
            if open.pop() == Some(true) {
                w.end();
            }
            w.end();
        }
    };
    // Whether a section is open.
    let mut in_section = false;
    // Whether a table of contents is open.
    let mut in_toc = false;
    for paragraph in body {
        if in_toc && (!paragraph.toc || paragraph.section.is_some()) {
            w.end().end();
            in_toc = false;
        }
        if let Some(name) = &paragraph.section {
            close_to(w, &mut open, 0);
            if in_section {
                w.end();
                in_section = false;
            }
            if !name.is_empty() {
                names.sections += 1;
                w.start("text:section")
                    .attr("text:style-name", name)
                    .attr("text:name", &format!("Section{}", names.sections));
                in_section = true;
            }
        }
        if paragraph.toc {
            close_to(w, &mut open, 0);
            if !in_toc {
                names.tocs += 1;
                start_toc(w, names.tocs, toc_levels);
                in_toc = true;
            }
            write_paragraph(w, paragraph);
            continue;
        }
        let Some(member) = &paragraph.list else {
            close_to(w, &mut open, 0);
            write_paragraph(w, paragraph);
            continue;
        };
        if open.is_empty() || member.first || style != Some(member.style.as_str()) {
            close_to(w, &mut open, 0);
            w.start("text:list").attr("text:style-name", &member.style);
            if member.continues {
                w.attr("text:continue-numbering", "true");
            }
            open.push(false);
            style = Some(&member.style);
        }
        let depth = usize::from(member.level) + 1;
        close_to(w, &mut open, depth);
        let start_item = |w: &mut Writer, item: Option<&ListMember>| {
            w.start("text:list-item");
            if let Some(start) = item.and_then(|m| m.start) {
                w.attr("text:start-value", &start.to_string());
            }
            if let Some(style) = item.and_then(|m| m.style_override.as_deref()) {
                w.attr("text:style-override", style);
            }
        };
        // `open` holds, for each list open, whether it has an item open.
        if open.len() == depth
            && let Some(item) = open.last_mut()
        {
            if *item {
                w.end();
            }
            start_item(w, Some(member));
            *item = true;
        }
        while open.len() < depth {
            if let Some(item) = open.last_mut()
                && !*item
            {
                start_item(w, None);
                *item = true;
            }
            w.start("text:list");
            open.push(false);
            if open.len() == depth
                && let Some(item) = open.last_mut()
            {
                start_item(w, Some(member));
                *item = true;
            }
        }
        write_paragraph(w, paragraph);
    }
    close_to(w, &mut open, 0);
    if in_toc {
        w.end().end();
    }
    if in_section {
        w.end();
    }
}

fn write_properties(w: &mut Writer, properties: &Properties) {
    crate::table::write_table_properties(w, &properties.table, &border_value);
    // ODF orders the graphic properties before those of the paragraph and
    // the text, in a graphic style.
    crate::draw::write_graphic_properties(w, &properties.graphic);
    if properties.paragraph != ParagraphProperties::default() {
        w.start("style:paragraph-properties");
        paragraph_attrs(w, &properties.paragraph);
        paragraph_children(w, &properties.paragraph);
        w.end();
    }
    if properties.text != TextProperties::default() {
        w.start("style:text-properties");
        text_attrs(w, &properties.text);
        w.end();
    }
}

/// The value of a side of `fo:border`.
fn border_value(border: Border) -> String {
    match border {
        Border::None => "none".to_owned(),
        Border::Line {
            width,
            style,
            color,
        } => {
            let style = match style {
                BorderLine::Solid => "solid",
                BorderLine::Dashed => "dashed",
                BorderLine::Dotted => "dotted",
            };
            format!("{width} {style} {color}")
        }
    }
}

fn len(w: &mut Writer, name: &str, value: Option<Length>) {
    if let Some(value) = value {
        w.attr(name, &value.to_string());
    }
}

fn paragraph_attrs(w: &mut Writer, p: &ParagraphProperties) {
    if let Some(align) = p.align {
        w.attr(
            "fo:text-align",
            match align {
                TextAlign::Start => "start",
                TextAlign::Left => "left",
                TextAlign::Center => "center",
                TextAlign::End => "end",
                TextAlign::Right => "right",
                TextAlign::Justify => "justify",
            },
        );
    }
    match p.line_height {
        Some(LineHeight::Normal) => {
            w.attr("fo:line-height", "normal");
        }
        Some(LineHeight::Percent(percent)) => {
            w.attr("fo:line-height", &format!("{}%", decimal(percent)));
        }
        Some(LineHeight::Exactly(length)) => {
            w.attr("fo:line-height", &length.to_string());
        }
        None => {}
    }
    len(w, "style:line-height-at-least", p.line_height_at_least);
    len(w, "style:line-spacing", p.line_spacing);
    len(w, "fo:margin-top", p.margin_top);
    len(w, "fo:margin-bottom", p.margin_bottom);
    len(w, "fo:margin-left", p.margin_left);
    len(w, "fo:margin-right", p.margin_right);
    len(w, "fo:text-indent", p.text_indent);
    if let Some(b) = p.break_before {
        w.attr(
            "fo:break-before",
            match b {
                Break::Auto => "auto",
                Break::Page => "page",
                Break::Column => "column",
            },
        );
    }
    match p.page_number {
        Some(Some(n)) => {
            w.attr("style:page-number", &n.to_string());
        }
        Some(None) => {
            w.attr("style:page-number", "auto");
        }
        None => {}
    }
    let always = |v: bool| if v { "always" } else { "auto" };
    if let Some(v) = p.keep_with_next {
        w.attr("fo:keep-with-next", always(v));
    }
    if let Some(v) = p.keep_together {
        w.attr("fo:keep-together", always(v));
    }
    if let Some(v) = p.widows {
        w.attr("fo:widows", &v.to_string());
    }
    if let Some(v) = p.orphans {
        w.attr("fo:orphans", &v.to_string());
    }
    len(w, "style:tab-stop-distance", p.tab_stop_distance);
    write_background(w, p.background);
    for (which, border) in [
        ("top", p.border_top),
        ("bottom", p.border_bottom),
        ("left", p.border_left),
        ("right", p.border_right),
    ] {
        let Some(border) = border else { continue };
        w.attr(&format!("fo:border-{which}"), &border_value(border));
    }
    len(w, "fo:padding-top", p.padding_top);
    len(w, "fo:padding-bottom", p.padding_bottom);
    len(w, "fo:padding-left", p.padding_left);
    len(w, "fo:padding-right", p.padding_right);
}

fn write_background(w: &mut Writer, background: Option<Background>) {
    match background {
        Some(Background::Transparent) => {
            w.attr("fo:background-color", "transparent");
        }
        Some(Background::Color(color)) => {
            w.attr("fo:background-color", &color.to_string());
        }
        None => {}
    }
}

/// The children of `style:paragraph-properties`, after its attributes.
fn paragraph_children(w: &mut Writer, p: &ParagraphProperties) {
    tab_stops(w, p);
    if let Some(cap) = p.drop_cap {
        w.start("style:drop-cap");
        if cap.length == 0 {
            w.attr("style:length", "word");
        } else {
            w.attr("style:length", &cap.length.to_string());
        }
        w.attr("style:lines", &cap.lines.to_string());
        len(w, "style:distance", cap.distance);
        // The extensions of the drop cap (docs/odf-extensions.md).
        if let Some(raised) = cap.raised {
            w.attr("desk:drop-cap-raised", &raised.to_string());
        }
        if let Some(shape) = cap.follow_shape {
            w.attr(
                "desk:drop-cap-wrap",
                if shape { "shape" } else { "rectangle" },
            );
        }
        if let Some(outdent) = cap.outdent {
            w.attr("desk:drop-cap-outdent", &format!("{outdent}%"));
        }
        if let Some(shape) = cap.background {
            w.attr("desk:drop-cap-fill", &shape.fill.to_string());
            len(w, "desk:drop-cap-border-width", shape.border_width);
            if let Some(color) = shape.border_color {
                w.attr("desk:drop-cap-border-color", &color.to_string());
            }
            if let Some(style) = shape.border_style {
                let name = match style {
                    BorderLine::Solid => "solid",
                    BorderLine::Dashed => "dashed",
                    BorderLine::Dotted => "dotted",
                };
                w.attr("desk:drop-cap-border-style", name);
            }
            if let Some(scale) = shape.scale {
                w.attr("desk:drop-cap-scale", &format!("{scale}%"));
            }
            if let Some(radius) = shape.radius {
                w.attr("desk:drop-cap-radius", &format!("{radius}%"));
            }
        }
        w.end();
    }
}

/// `style:tab-stops`, the first child of `style:paragraph-properties`.
fn tab_stops(w: &mut Writer, p: &ParagraphProperties) {
    let Some(stops) = &p.tab_stops else { return };
    w.start("style:tab-stops");
    for stop in stops {
        w.start("style:tab-stop")
            .attr("style:position", &stop.position.to_string());
        match stop.kind {
            TabType::Left => {}
            TabType::Center => {
                w.attr("style:type", "center");
            }
            TabType::Right => {
                w.attr("style:type", "right");
            }
            TabType::Char => {
                w.attr("style:type", "char")
                    .attr("style:char", &stop.char.unwrap_or('.').to_string());
            }
        }
        let style = match stop.leader_style {
            LeaderStyle::None => None,
            LeaderStyle::Solid => Some("solid"),
            LeaderStyle::Dotted => Some("dotted"),
            LeaderStyle::Dash => Some("dash"),
        };
        if let Some(style) = style {
            w.attr("style:leader-style", style);
        }
        if let Some(text) = stop.leader_text {
            w.attr("style:leader-text", &text.to_string());
        }
        w.end();
    }
    w.end();
}

fn write_line(w: &mut Writer, which: &str, line: Option<Line>) {
    let Some(line) = line else { return };
    let (style, kind) = match line {
        Line::None => ("none", "none"),
        Line::Single => ("solid", "single"),
        Line::Double => ("solid", "double"),
    };
    w.attr(&format!("style:text-{which}-style"), style)
        .attr(&format!("style:text-{which}-type"), kind);
    if line != Line::None {
        w.attr(&format!("style:text-{which}-width"), "auto");
    }
}

fn text_attrs(w: &mut Writer, t: &TextProperties) {
    w.attr_opt("style:font-name", t.font_family.as_deref());
    len(w, "fo:font-size", t.font_size);
    if t.font_size.is_none()
        && let Some(percent) = t.font_size_percent
    {
        w.attr("fo:font-size", &format!("{}%", decimal(percent)));
    }
    if let Some(weight) = t.font_weight {
        let text = match weight {
            400 => "normal".to_owned(),
            700 => "bold".to_owned(),
            _ => weight.to_string(),
        };
        w.attr("fo:font-weight", &text);
    }
    if let Some(italic) = t.italic {
        w.attr("fo:font-style", if italic { "italic" } else { "normal" });
    }
    write_line(w, "underline", t.underline);
    match t.underline_color {
        Some(LineColor::FontColor) => {
            w.attr("style:text-underline-color", "font-color");
        }
        Some(LineColor::Color(color)) => {
            w.attr("style:text-underline-color", &color.to_string());
        }
        None => {}
    }
    write_line(w, "line-through", t.line_through);
    match t.line_through_color {
        Some(LineColor::FontColor) => {
            w.attr("style:text-line-through-color", "font-color");
        }
        Some(LineColor::Color(color)) => {
            w.attr("style:text-line-through-color", &color.to_string());
        }
        None => {}
    }
    if let Some(color) = t.color {
        w.attr("fo:color", &color.to_string());
    }
    if let Some(position) = t.position {
        let first = match position {
            TextPosition::Super => "super".to_owned(),
            TextPosition::Sub => "sub".to_owned(),
            TextPosition::Percent(p) => format!("{}%", decimal(p)),
        };
        let value = match t.position_scale {
            Some(scale) => format!("{first} {}%", decimal(scale)),
            None => first,
        };
        w.attr("style:text-position", &value);
    }
    if let Some(transform) = t.transform {
        w.attr(
            "fo:text-transform",
            match transform {
                TextTransform::None => "none",
                TextTransform::Uppercase => "uppercase",
                TextTransform::Lowercase => "lowercase",
                TextTransform::Capitalize => "capitalize",
            },
        );
    }
    if let Some(small) = t.small_caps {
        w.attr(
            "fo:font-variant",
            if small { "small-caps" } else { "normal" },
        );
    }
    if let Some(hyphenate) = t.hyphenate {
        w.attr("fo:hyphenate", if hyphenate { "true" } else { "false" });
    }
    if let Some(hyphenate) = t.desk_hyphenate {
        w.attr("desk:hyphenate", if hyphenate { "true" } else { "false" });
    }
    if let Some(tracking) = t.tracking {
        w.attr("desk:tracking", &tracking.to_string());
    }
    if let Some(title) = t.title_case {
        w.attr("desk:title-case", if title { "true" } else { "false" });
    }
    if let Some(opacity) = t.shadow_opacity {
        w.attr("desk:shadow-opacity", &format!("{}%", decimal(opacity)));
    }
    w.attr_opt("desk:text-fill", t.fill.as_deref())
        .attr_opt(
            "desk:text-fill-gradient-name",
            t.fill_gradient_name.as_deref(),
        )
        .attr_opt("desk:text-fill-image-name", t.fill_image_name.as_deref())
        .attr_opt("desk:text-fill-image-mode", t.fill_image_mode.as_deref());
    if let Some(tint) = t.fill_tint {
        w.attr("desk:text-fill-tint", &tint.to_string());
    }
    if let Some(opacity) = t.fill_tint_opacity {
        w.attr(
            "desk:text-fill-tint-opacity",
            &format!("{}%", decimal(opacity)),
        );
    }
    if let Some(page) = t.fill_page {
        w.attr("desk:text-fill-page", if page { "true" } else { "false" });
    }
    w.attr_opt("fo:language", t.language.as_deref())
        .attr_opt("fo:country", t.country.as_deref())
        .attr_opt("fo:script", t.script.as_deref());
    if let Some(spacing) = t.letter_spacing {
        if spacing.0 == 0.0 {
            w.attr("fo:letter-spacing", "normal");
        } else {
            w.attr("fo:letter-spacing", &spacing.to_string());
        }
    }
    if let Some(outline) = t.outline {
        w.attr("style:text-outline", if outline { "true" } else { "false" });
    }
    match t.shadow {
        Some(TextShadow::None) => {
            w.attr("fo:text-shadow", "none");
        }
        Some(TextShadow::Shadow {
            dx,
            dy,
            blur,
            color,
        }) => {
            let mut value = format!("{dx} {dy}");
            if let Some(blur) = blur {
                value.push_str(&format!(" {blur}"));
            }
            if let Some(color) = color {
                value.push_str(&format!(" {color}"));
            }
            w.attr("fo:text-shadow", &value);
        }
        None => {}
    }
    write_background(w, t.background);
}

fn write_header_footer(w: &mut Writer, name: &'static str, part: &HeaderFooter) {
    w.start(name);
    if !part.display {
        w.attr("style:display", "false");
    }
    for paragraph in &part.paragraphs {
        write_paragraph(w, paragraph);
    }
    // The centre and right fields (W096).
    for (field, paragraphs) in [("center", &part.centre), ("right", &part.right)] {
        for paragraph in paragraphs {
            write_paragraph_in(w, paragraph, Some(field));
        }
    }
    w.end();
}

fn write_header_footer_style(
    w: &mut Writer,
    name: &'static str,
    spacing: &str,
    style: &HeaderFooterStyle,
) {
    w.start(name).start("style:header-footer-properties");
    len(w, "fo:min-height", style.min_height);
    len(w, spacing, style.spacing);
    w.end().end();
}

/// The drawing page style that fills the pages of a master page (W098).
fn write_drawing_page_style(w: &mut Writer, name: &str, fill: &PageFill) {
    w.start("style:style")
        .attr("style:name", name)
        .attr("style:family", "drawing-page");
    w.start("style:drawing-page-properties")
        .attr("draw:fill", &fill.fill);
    if let Some(color) = fill.color {
        w.attr("draw:fill-color", &color.to_string());
    }
    w.attr_opt("draw:fill-gradient-name", fill.gradient_name.as_deref());
    w.attr_opt("draw:fill-image-name", fill.image_name.as_deref());
    w.attr_opt("style:repeat", fill.repeat.as_deref());
    // The whole page, not only inside its margins.
    w.attr("draw:background-size", "full");
    w.attr_opt("desk:fill-image-mode", fill.image_mode.as_deref());
    if let Some(tint) = fill.tint {
        w.attr("desk:fill-tint", &tint.to_string());
    }
    if let Some(opacity) = fill.tint_opacity {
        w.attr("desk:fill-tint-opacity", &format!("{}%", decimal(opacity)));
    }
    w.end().end();
}

fn write_page_layout(w: &mut Writer, name: &str, page: &PageLayout, fill: Option<&PageFill>) {
    w.start("style:page-layout").attr("style:name", name);
    if page.mirrored == Some(true) {
        w.attr("style:page-usage", "mirrored");
    }
    w.start("style:page-layout-properties");
    len(w, "fo:page-width", page.width);
    len(w, "fo:page-height", page.height);
    if let (Some(width), Some(height)) = (page.width, page.height) {
        w.attr(
            "style:print-orientation",
            if width > height {
                "landscape"
            } else {
                "portrait"
            },
        );
    }
    len(w, "fo:margin-top", page.margin_top);
    len(w, "fo:margin-bottom", page.margin_bottom);
    len(w, "fo:margin-left", page.margin_left);
    len(w, "fo:margin-right", page.margin_right);
    // The color of a background, for programs that do not read the
    // drawing page style.
    if let Some(color) = fill.and_then(|f| f.color) {
        w.attr("fo:background-color", &color.to_string());
    }
    if let Some(columns) = &page.columns {
        write_columns(w, columns);
    }
    w.end();
    if let Some(header) = &page.header {
        write_header_footer_style(w, "style:header-style", "fo:margin-bottom", header);
    }
    if let Some(footer) = &page.footer {
        write_header_footer_style(w, "style:footer-style", "fo:margin-top", footer);
    }
    w.end();
}

/// Writes text so that a reader collapsing white space gets it back:
/// a space after another one, or at the start, becomes `text:s`.
struct TextWriter {
    /// Whether the last thing written was a literal space, which a reader
    /// would collapse a following one into. True at the start.
    after_space: bool,
}

impl TextWriter {
    fn write(&mut self, w: &mut Writer, text: &str) {
        let mut plain = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                ' ' => {
                    let mut count = 1;
                    while chars.peek() == Some(&' ') {
                        chars.next();
                        count += 1;
                    }
                    if !self.after_space {
                        plain.push(' ');
                        count -= 1;
                        self.after_space = true;
                    }
                    if count > 0 {
                        flush(w, &mut plain);
                        w.start("text:s");
                        if count > 1 {
                            w.attr("text:c", &count.to_string());
                        }
                        w.end();
                        self.after_space = false;
                    }
                }
                '\t' | LINE_BREAK | '\n' => {
                    flush(w, &mut plain);
                    w.start(if c == '\t' {
                        "text:tab"
                    } else {
                        "text:line-break"
                    })
                    .end();
                    self.after_space = false;
                }
                _ => {
                    plain.push(c);
                    self.after_space = false;
                }
            }
        }
        flush(w, &mut plain);
    }
}

fn flush(w: &mut Writer, plain: &mut String) {
    if !plain.is_empty() {
        w.text(plain);
        plain.clear();
    }
}

fn write_paragraph(w: &mut Writer, paragraph: &Paragraph) {
    write_paragraph_in(w, paragraph, None);
}

/// Writes a paragraph, of a field of a header or footer when it is in one
/// other than the left (W096).
fn write_paragraph_in(w: &mut Writer, paragraph: &Paragraph, header_field: Option<&str>) {
    match paragraph.outline_level {
        Some(level) => {
            w.start("text:h")
                .attr("text:outline-level", &level.to_string());
        }
        None => {
            w.start("text:p");
        }
    }
    w.attr_opt("text:style-name", paragraph.style.as_deref());
    w.attr_opt("desk:header-field", header_field);
    for mark in &paragraph.marks {
        w.start("text:bookmark").attr("text:name", mark).end();
    }
    for object in &paragraph.frames {
        crate::draw::write_draw_object(w, object, &write_paragraph);
    }
    let mut text = TextWriter { after_space: true };
    let mut link: Option<&str> = None;
    let mut bookmark: Option<&str> = None;
    for span in &paragraph.spans {
        // Bookmarks start and end outside links.
        if span.bookmark.as_deref() != bookmark {
            if link.take().is_some() {
                w.end();
            }
            if let Some(name) = bookmark.take() {
                w.start("text:bookmark-end").attr("text:name", name).end();
            }
            if let Some(name) = span.bookmark.as_deref() {
                w.start("text:bookmark-start").attr("text:name", name).end();
                bookmark = Some(name);
            }
        }
        if span.link.as_deref() != link {
            if link.take().is_some() {
                w.end();
            }
            if let Some(href) = span.link.as_deref() {
                w.start("text:a")
                    .attr("xlink:type", "simple")
                    .attr("xlink:href", href);
                link = Some(href);
            }
        }
        if let Some(style) = &span.style {
            w.start("text:span").attr("text:style-name", style);
        }
        match &span.field {
            Some(field) => write_field(w, field, &span.text),
            None => text.write(w, &span.text),
        }
        if span.style.is_some() {
            w.end();
        }
        // A comment and its end leave the white space around them alone.
        if span.field.is_some()
            && !matches!(
                span.field,
                Some(
                    Field::Annotation(_)
                        | Field::AnnotationEnd { .. }
                        | Field::ChangeStart { .. }
                        | Field::ChangeEnd { .. }
                        | Field::Change { .. }
                )
            )
        {
            text.after_space = false;
        }
    }
    if link.is_some() {
        w.end();
    }
    if let Some(name) = bookmark {
        w.start("text:bookmark-end").attr("text:name", name).end();
    }
    w.end();
}

/// The tracked changes of the body (`text:tracked-changes`), with the
/// paragraphs that each deletion took (W148).
fn write_tracked_changes(w: &mut Writer, document: &TextDocument, names: &mut Names) {
    if document.tracked_changes.is_empty() && !document.track_changes {
        return;
    }
    w.start("text:tracked-changes").attr(
        "text:track-changes",
        if document.track_changes {
            "true"
        } else {
            "false"
        },
    );
    for region in &document.tracked_changes {
        w.start("text:changed-region")
            .attr("xml:id", &region.id)
            .attr("text:id", &region.id);
        w.start(match region.kind {
            crate::ChangeKind::Insertion => "text:insertion",
            crate::ChangeKind::Deletion => "text:deletion",
            crate::ChangeKind::FormatChange => "text:format-change",
        });
        w.attr_opt("desk:style-name", region.previous_style.as_deref());
        w.attr_opt(
            "desk:paragraph-style-name",
            region.previous_paragraph_style.as_deref(),
        );
        w.attr_opt("desk:change-name", region.change_name.as_deref());
        w.start("office:change-info");
        w.text_element("dc:creator", region.creator.as_deref().unwrap_or_default());
        // ODF needs a date: an unknown one is written as the epoch.
        w.text_element(
            "dc:date",
            region.date.as_deref().unwrap_or(crate::UNKNOWN_DATE),
        );
        w.end();
        // The paragraphs it took, with their lists (B-067), the sections
        // they start and the tables of contents they are entries of
        // (B-068).
        write_body(w, &region.deleted, &document.toc_levels, names);
        w.end().end();
    }
    w.end();
}

/// A comment: an `office:annotation` in the text, or the
/// `desk:annotation` of a drawing object (`desk`), with its name in
/// `desk:name`. A reply names its comment in `desk:parent-name`
/// (odf-extensions.md).
pub(crate) fn write_annotation(w: &mut Writer, desk: bool, annotation: &crate::Annotation) {
    let (element, name) = if desk {
        ("desk:annotation", "desk:name")
    } else {
        ("office:annotation", "office:name")
    };
    w.start(element);
    w.attr_opt(name, annotation.name.as_deref());
    w.attr_opt("desk:parent-name", annotation.parent.as_deref());
    if let Some(creator) = &annotation.creator {
        w.text_element("dc:creator", creator);
    }
    if let Some(date) = &annotation.date {
        w.text_element("dc:date", date);
    }
    for line in annotation.text.split('\n') {
        w.start("text:p");
        TextWriter { after_space: true }.write(w, line);
        w.end();
    }
    w.end();
}

fn write_field(w: &mut Writer, field: &Field, shown: &str) {
    match field {
        Field::Object(object) => {
            crate::draw::write_draw_object(w, object, &write_paragraph);
            return;
        }
        Field::Annotation(annotation) => {
            write_annotation(w, false, annotation);
            return;
        }
        Field::AnnotationEnd { name } => {
            w.start("office:annotation-end")
                .attr("office:name", name)
                .end();
            return;
        }
        Field::ChangeStart { id } | Field::ChangeEnd { id } | Field::Change { id } => {
            let element = match field {
                Field::ChangeStart { .. } => "text:change-start",
                Field::ChangeEnd { .. } => "text:change-end",
                _ => "text:change",
            };
            w.start(element).attr("text:change-id", id).end();
            return;
        }
        Field::PageNumber { num_format } => {
            w.start("text:page-number")
                .attr("text:select-page", "current");
            w.attr_opt("style:num-format", num_format.as_deref());
        }
        Field::PageCount { num_format } => {
            w.start("text:page-count");
            w.attr_opt("style:num-format", num_format.as_deref());
        }
        Field::Date { pattern, fixed } => {
            w.start("text:date")
                .attr("text:fixed", if *fixed { "true" } else { "false" });
            w.attr_opt("desk:pattern", pattern.as_deref());
        }
        Field::FileName { display } => {
            w.start("text:file-name")
                .attr("text:display", display)
                .attr("text:fixed", "true");
        }
        Field::PageOf { name } => {
            w.start("text:bookmark-ref")
                .attr("text:reference-format", "page")
                .attr("text:ref-name", name);
        }
        Field::Note(note) => {
            w.start("text:note");
            w.attr_opt("text:id", note.id.as_deref());
            w.attr(
                "text:note-class",
                if note.endnote { "endnote" } else { "footnote" },
            );
            w.start("text:note-citation");
            w.attr_opt("text:label", note.label.as_deref());
            if !shown.is_empty() {
                w.text(shown);
            }
            w.end();
            w.start("text:note-body");
            for paragraph in &note.body {
                write_paragraph(w, paragraph);
            }
            w.end().end();
            return;
        }
    }
    if !shown.is_empty() {
        w.text(shown);
    }
    w.end();
}

fn write_meta(meta: &Meta) -> String {
    let mut w = Writer::new();
    w.start("office:document-meta")
        .attr("xmlns:office", ns::OFFICE)
        .attr("xmlns:meta", ns::META)
        .attr("xmlns:dc", ns::DC)
        .attr("office:version", VERSION)
        .start("office:meta");
    if let Some(v) = &meta.generator {
        w.text_element("meta:generator", v);
    }
    if let Some(v) = &meta.title {
        w.text_element("dc:title", v);
    }
    if let Some(v) = &meta.subject {
        w.text_element("dc:subject", v);
    }
    if let Some(v) = &meta.description {
        w.text_element("dc:description", v);
    }
    for keyword in &meta.keywords {
        w.text_element("meta:keyword", keyword);
    }
    if let Some(v) = &meta.initial_creator {
        w.text_element("meta:initial-creator", v);
    }
    if let Some(v) = &meta.creator {
        w.text_element("dc:creator", v);
    }
    w.end().end();
    w.finish()
}
