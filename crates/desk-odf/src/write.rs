// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Writing a text document as XML streams.

use std::collections::BTreeSet;

use crate::ns;
use crate::text::{
    Background, Border, BorderLine, Break, Family, LINE_BREAK, LeaderStyle, Line, LineColor,
    LineHeight, Meta, PageLayout, Paragraph, ParagraphProperties, Properties, Style, TabType,
    TextAlign, TextDocument, TextPosition, TextProperties, TextShadow, TextTransform,
};
use crate::units::{Length, decimal};
use crate::xml::Writer;

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
}

fn root(w: &mut Writer, name: &'static str) {
    w.start(name)
        .attr("xmlns:office", ns::OFFICE)
        .attr("xmlns:style", ns::STYLE)
        .attr("xmlns:text", ns::TEXT)
        .attr("xmlns:fo", ns::FO)
        .attr("xmlns:svg", ns::SVG)
        .attr("xmlns:meta", ns::META)
        .attr("xmlns:dc", ns::DC)
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
    w.end();
    w.start("office:body").start("office:text");
    for paragraph in &document.body {
        write_paragraph(&mut w, paragraph);
    }
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
    w.end();
    w.start("office:automatic-styles");
    write_page_layout(&mut w, &document.page_layout);
    w.end();
    w.start("office:master-styles")
        .start("style:master-page")
        .attr("style:name", "Standard")
        .attr("style:page-layout-name", "pm1")
        .end()
        .end();
    w.end();
    let styles = w.finish();

    Streams {
        content,
        styles,
        meta: write_meta(&document.meta),
    }
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
                Family::Text => "text",
            },
        )
        .attr_opt("style:parent-style-name", style.parent.as_deref())
        .attr_opt("style:next-style-name", style.next.as_deref());
    write_properties(w, &style.properties);
    w.end();
}

fn write_properties(w: &mut Writer, properties: &Properties) {
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
        let value = match border {
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
        };
        w.attr(&format!("fo:border-{which}"), &value);
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
    if t.underline.is_some_and(|l| l != Line::None) {
        w.attr("style:text-underline-color", "font-color");
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

fn write_page_layout(w: &mut Writer, page: &PageLayout) {
    w.start("style:page-layout")
        .attr("style:name", "pm1")
        .start("style:page-layout-properties");
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
    w.end().end();
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
    let mut text = TextWriter { after_space: true };
    for span in &paragraph.spans {
        match &span.style {
            Some(style) => {
                w.start("text:span").attr("text:style-name", style);
                text.write(w, &span.text);
                w.end();
            }
            None => text.write(w, &span.text),
        }
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
