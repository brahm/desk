// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Formatted text pasted from other programs (TXT-04): the HTML that web
//! browsers, mail programs and word processors put on the clipboard, read
//! as paragraphs with their emphasis, sizes, colors, links and alignment.
//! What HTML says beyond that, as tables and pictures, gives its text
//! only.
//!
//! Text copied in Desk goes to other programs the same way: as HTML with
//! the look of its styles written out.

use std::sync::Arc;

use desk_model::{
    Alignment, Baseline, Capitalization, CharAttrs, CharFormat, Document, Editor, FIELD_CHARACTER,
    Field, FontWeight, Fragment, FragmentParagraph, LINE_BREAK, LIST_LEVELS, Length, LineSpacing,
    LineStyle, ListLabel, NO_BREAK_SPACE, NumberFormat, ParaAttrs, ParaFormat, Rgba,
};

use crate::odt::FilterError;

/// The media type of HTML on the clipboard.
pub const HTML_TYPE: &str = "text/html";

/// The most nested elements followed, against hostile text.
const MAX_DEPTH: usize = 256;

/// The text of `html` with its formatting, as a paste takes it.
pub fn read_html(html: &str) -> Result<Fragment, FilterError> {
    let paragraphs = parse(html);
    let mut editor = Editor::new(Document::new_word_processing());
    editor.edit("Paste", |edit| {
        let mut at = edit.document().start();
        for (i, paragraph) in paragraphs.iter().enumerate() {
            if i > 0 {
                at = edit.split_paragraph(at)?;
            }
            let start = at;
            // The text first, then its formatting, so that no run takes
            // the formatting of the one before it.
            let text: String = paragraph.runs.iter().map(|(t, _)| t.as_str()).collect();
            at = edit.insert_text(at, &text)?;
            let mut offset = start.offset;
            for (text, attrs) in &paragraph.runs {
                let from = desk_model::Position::new(start.paragraph, offset);
                offset += text.len();
                if *attrs != CharAttrs::default() {
                    let to = desk_model::Position::new(start.paragraph, offset);
                    edit.set_character_attrs(from, to, attrs)?;
                }
            }
            if let Some(alignment) = paragraph.alignment {
                let attrs = ParaAttrs {
                    alignment: Some(alignment),
                    ..ParaAttrs::default()
                };
                edit.set_paragraph_attrs(start, at, &attrs)?;
            }
        }
        Ok(())
    })?;
    let document = editor.document();
    Ok(document.copy_fragment(document.start(), document.end())?)
}

/// The HTML that puts text copied with its styles on the clipboard for
/// other programs (TXT-04): a paragraph or list item for each paragraph,
/// with the look its styles give it written out, since the styles stay
/// behind.
pub fn write_html(fragment: &Fragment) -> String {
    let mut out =
        String::from("<meta http-equiv=\"content-type\" content=\"text/html; charset=utf-8\">");
    // The lists open, from the outermost: their element, and whether an
    // item of theirs is open.
    let mut lists: Vec<(&'static str, bool)> = Vec::new();
    for paragraph in &fragment.paragraphs {
        let mut format = ParaFormat::default();
        paragraph.style.paragraph.apply_to(&mut format);
        paragraph.attrs.apply_to(&mut format);
        let level = usize::from(format.list_level).min(LIST_LEVELS - 1);
        let label = format
            .list_style
            .and_then(|id| fragment.lists.iter().find(|l| l.id == id))
            .map(|list| &list.levels)
            .filter(|levels| levels[level].label.is_some());
        let Some(levels) = label else {
            close_lists(&mut out, &mut lists, 0);
            out.push_str("<p style=\"");
            out.push_str(&paragraph_css(&format, false));
            out.push_str("\">");
            write_runs(&mut out, paragraph);
            out.push_str("</p>");
            continue;
        };
        let element = list_element(&levels[level].label);
        close_lists(&mut out, &mut lists, level + 1);
        match lists.get_mut(level) {
            // Another kind of list at the same level starts a list.
            Some((open, _)) if *open != element => close_lists(&mut out, &mut lists, level),
            Some((_, item)) if *item => {
                out.push_str("</li>");
                *item = false;
            }
            _ => {}
        }
        while lists.len() <= level {
            let label = &levels[lists.len()].label;
            let element = list_element(label);
            out.push('<');
            out.push_str(element);
            if let ListLabel::Number { format, .. } = label {
                out.push_str(match format {
                    NumberFormat::Decimal => " type=\"1\"",
                    NumberFormat::UpperRoman => " type=\"I\"",
                    NumberFormat::LowerRoman => " type=\"i\"",
                    NumberFormat::UpperAlpha => " type=\"A\"",
                    NumberFormat::LowerAlpha => " type=\"a\"",
                });
            }
            out.push('>');
            lists.push((element, false));
        }
        out.push_str("<li");
        if let Some(start) = format.list_start {
            out.push_str(&format!(" value=\"{start}\""));
        }
        out.push_str(" style=\"");
        out.push_str(&paragraph_css(&format, true));
        out.push_str("\">");
        write_runs(&mut out, paragraph);
        lists[level].1 = true;
    }
    close_lists(&mut out, &mut lists, 0);
    out
}

/// The element of the list of a label.
fn list_element(label: &ListLabel) -> &'static str {
    match label {
        ListLabel::Number { .. } => "ol",
        _ => "ul",
    }
}

/// Closes the lists open deeper than `depth`, with their items.
fn close_lists(out: &mut String, lists: &mut Vec<(&'static str, bool)>, depth: usize) {
    while lists.len() > depth {
        let Some((element, item)) = lists.pop() else {
            break;
        };
        if item {
            out.push_str("</li>");
        }
        out.push_str("</");
        out.push_str(element);
        out.push('>');
    }
}

/// A length in points, without the zeros at its end.
fn points(length: Length) -> String {
    let text = format!("{:.3}", length.to_pt());
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}pt")
}

/// A color, as `#rrggbb`.
fn css_color(color: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// The look of a paragraph in CSS; an item of a list leaves its indents
/// to the list.
fn paragraph_css(format: &ParaFormat, item: bool) -> String {
    let mut css = format!(
        "margin-top:{};margin-bottom:{};white-space:pre-wrap",
        points(format.space_before),
        points(format.space_after)
    );
    if !item {
        css.push_str(&format!(
            ";margin-left:{};margin-right:{};text-indent:{}",
            points(format.left_indent),
            points(format.right_indent),
            points(format.first_line_indent)
        ));
    }
    match format.alignment {
        Alignment::Left | Alignment::Auto => {}
        Alignment::Center => css.push_str(";text-align:center"),
        Alignment::Right => css.push_str(";text-align:right"),
        Alignment::Justify => css.push_str(";text-align:justify"),
    }
    match format.line_spacing {
        LineSpacing::Multiple { percent } if percent != 100 => {
            css.push_str(&format!(";line-height:{percent}%"));
        }
        LineSpacing::Exactly(height) => css.push_str(&format!(";line-height:{}", points(height))),
        _ => {}
    }
    if let Some(background) = format.background {
        css.push_str(&format!(";background-color:{}", css_color(background)));
    }
    css
}

/// The text of a paragraph, each run in the elements and the style of
/// its look.
fn write_runs(out: &mut String, paragraph: &FragmentParagraph) {
    let mut any = false;
    for (text, style, attrs) in &paragraph.runs {
        let mut format = CharFormat::default();
        paragraph.style.character.apply_to(&mut format);
        if let Some(style) = style {
            style.character.apply_to(&mut format);
        }
        attrs.apply_to(&mut format);
        // A field shows what it showed, where it knows; the marks of
        // notes and objects stay behind.
        let shown = match &format.field {
            Some(Field::DateTime { shown, .. }) => text.replace(FIELD_CHARACTER, shown),
            _ => text.replace(FIELD_CHARACTER, ""),
        };
        if shown.is_empty() {
            continue;
        }
        any = true;
        let mut close: Vec<&str> = Vec::new();
        if let Some(link) = &format.link {
            out.push_str(&format!("<a href=\"{}\">", escape(link, true)));
            close.push("</a>");
        }
        out.push_str(&format!("<span style=\"{}\">", character_css(&format)));
        close.push("</span>");
        let mut element = |on: bool, name: &'static str, end: &'static str| {
            if on {
                out.push_str(name);
                close.push(end);
            }
        };
        element(format.weight == FontWeight::BOLD, "<b>", "</b>");
        element(format.italic, "<i>", "</i>");
        element(format.underline != LineStyle::None, "<u>", "</u>");
        element(format.strikethrough != LineStyle::None, "<s>", "</s>");
        element(format.baseline == Baseline::Superscript, "<sup>", "</sup>");
        element(format.baseline == Baseline::Subscript, "<sub>", "</sub>");
        out.push_str(&escape(&shown, false));
        for end in close.iter().rev() {
            out.push_str(end);
        }
    }
    // An empty paragraph keeps its line.
    if !any {
        out.push_str("<br>");
    }
}

/// The look of text in CSS, but for what its elements say.
fn character_css(format: &CharFormat) -> String {
    let family: String = format
        .font_family
        .chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\' | '<' | '>' | '&' | ';'))
        .collect();
    let mut css = format!(
        "font-family:'{family}';font-size:{}",
        points(format.font_size)
    );
    if format.weight != FontWeight::NORMAL && format.weight != FontWeight::BOLD {
        css.push_str(&format!(";font-weight:{}", format.weight.value()));
    }
    if format.color != Rgba::BLACK {
        css.push_str(&format!(";color:{}", css_color(format.color)));
    }
    if let Some(background) = format.background {
        css.push_str(&format!(";background-color:{}", css_color(background)));
    }
    match format.capitalization {
        Capitalization::None => {}
        Capitalization::AllCaps => css.push_str(";text-transform:uppercase"),
        Capitalization::SmallCaps => css.push_str(";font-variant:small-caps"),
        Capitalization::Title => css.push_str(";text-transform:capitalize"),
    }
    css
}

/// Text as HTML holds it, in an element or in the value of an attribute.
fn escape(text: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            NO_BREAK_SPACE => out.push_str("&nbsp;"),
            LINE_BREAK if !attribute => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

/// A paragraph read from HTML.
#[derive(Debug, Default, PartialEq)]
struct Para {
    runs: Vec<(String, CharAttrs)>,
    alignment: Option<Alignment>,
}

/// An element open while reading: what it sets on its text.
#[derive(Clone, Debug)]
struct Open {
    name: String,
    attrs: CharAttrs,
    alignment: Option<Alignment>,
}

/// The paragraphs of `html`, with blank ones at the ends left out.
fn parse(html: &str) -> Vec<Para> {
    let mut out: Vec<Para> = vec![Para::default()];
    let mut stack: Vec<Open> = Vec::new();
    // Whether the text so far ends in a space, which collapses with the
    // next, as in HTML; a new paragraph starts as if after one.
    let mut space = true;
    let mut skip: Option<String> = None;
    let mut preformatted = 0usize;
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        if rest.starts_with('<')
            && let Some(end) = rest.find('>')
        {
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            let closing = tag.starts_with('/');
            let tag = tag.trim_start_matches('/');
            let name: String = tag
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            if let Some(skipped) = &skip {
                if closing && *skipped == name {
                    skip = None;
                }
                continue;
            }
            if matches!(name.as_str(), "head" | "style" | "script" | "title") && !closing {
                skip = Some(name);
                continue;
            }
            let block = is_block(&name);
            if closing {
                if let Some(at) = stack.iter().rposition(|o| o.name == name) {
                    stack.truncate(at);
                }
                if name == "pre" {
                    preformatted = preformatted.saturating_sub(1);
                }
                if block {
                    new_paragraph(&mut out, &stack, &mut space);
                }
                continue;
            }
            if name == "br" {
                push(&mut out, &stack, &LINE_BREAK.to_string());
                space = true;
                continue;
            }
            if block {
                new_paragraph(&mut out, &stack, &mut space);
            }
            if name == "pre" {
                preformatted += 1;
            }
            let void = tag.ends_with('/')
                || matches!(
                    name.as_str(),
                    "img" | "hr" | "meta" | "link" | "input" | "col" | "wbr" | "area" | "base"
                );
            if !void && stack.len() < MAX_DEPTH {
                let inherited = stack.last().map(|o| o.attrs.clone()).unwrap_or_default();
                let (attrs, alignment) = element(&name, tag, inherited);
                stack.push(Open {
                    name,
                    attrs,
                    alignment,
                });
                if block && let Some(last) = out.last_mut() {
                    last.alignment = alignment_of(&stack);
                }
            }
            continue;
        }
        // Text up to the next tag; a `<` that opens none is text.
        let first = rest.chars().next().map_or(0, char::len_utf8);
        let end = rest[first..].find('<').map_or(rest.len(), |e| e + first);
        let raw = &rest[..end];
        rest = &rest[end..];
        if skip.is_some() {
            continue;
        }
        let text = decode(raw);
        if preformatted > 0 {
            // Every character the model takes as the end of a paragraph
            // ends one here too, so that the runs keep their offsets.
            let text = text.replace("\r\n", "\n");
            let ends = ['\n', '\r', '\u{0b}', '\u{0c}', '\u{85}', '\u{2029}'];
            for (i, line) in text.split(ends).enumerate() {
                if i > 0 {
                    new_paragraph(&mut out, &stack, &mut space);
                }
                push(&mut out, &stack, line);
            }
            space = text.ends_with(char::is_whitespace);
            continue;
        }
        let mut collapsed = String::new();
        for c in text.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                if !space {
                    collapsed.push(' ');
                    space = true;
                }
            } else {
                collapsed.push(c);
                space = false;
            }
        }
        push(&mut out, &stack, &collapsed);
    }
    // A space at the end of a paragraph shows nothing.
    for paragraph in &mut out {
        if let Some((text, _)) = paragraph.runs.last_mut() {
            let trimmed = text.trim_end_matches(' ').len();
            text.truncate(trimmed);
        }
        paragraph.runs.retain(|(text, _)| !text.is_empty());
    }
    while out.len() > 1 && out.last().is_some_and(|p| p.runs.is_empty()) {
        out.pop();
    }
    while out.len() > 1 && out.first().is_some_and(|p| p.runs.is_empty()) {
        out.remove(0);
    }
    out
}

/// Whether an element of HTML starts a paragraph of its own.
fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "dt"
            | "dd"
            | "tr"
            | "blockquote"
            | "pre"
            | "address"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "table"
            | "ul"
            | "ol"
            | "hr"
    )
}

/// Ends the paragraph being read, unless it has nothing yet.
fn new_paragraph(out: &mut Vec<Para>, stack: &[Open], space: &mut bool) {
    if out.last().is_some_and(|p| !p.runs.is_empty()) {
        out.push(Para {
            runs: Vec::new(),
            alignment: alignment_of(stack),
        });
    }
    *space = true;
}

/// Adds text to the paragraph being read, with the formatting of the
/// elements it is in.
fn push(out: &mut [Para], stack: &[Open], text: &str) {
    if text.is_empty() {
        return;
    }
    let attrs = stack.last().map(|o| o.attrs.clone()).unwrap_or_default();
    let Some(paragraph) = out.last_mut() else {
        return;
    };
    match paragraph.runs.last_mut() {
        Some((last, last_attrs)) if *last_attrs == attrs => last.push_str(text),
        _ => paragraph.runs.push((text.to_owned(), attrs)),
    }
}

/// The alignment that the innermost element that sets one gives.
fn alignment_of(stack: &[Open]) -> Option<Alignment> {
    stack.iter().rev().find_map(|o| o.alignment)
}

/// What an element sets on its text, over what it is in: `tag` is the
/// whole of its opening tag, after the `<`.
fn element(name: &str, tag: &str, mut attrs: CharAttrs) -> (CharAttrs, Option<Alignment>) {
    match name {
        "b" | "strong" | "th" => attrs.weight = Some(FontWeight::BOLD),
        "i" | "em" | "cite" | "var" | "dfn" => attrs.italic = Some(true),
        "u" | "ins" => attrs.underline = Some(LineStyle::Single),
        "s" | "strike" | "del" => attrs.strikethrough = Some(LineStyle::Single),
        "sup" => attrs.baseline = Some(Baseline::Superscript),
        "sub" => attrs.baseline = Some(Baseline::Subscript),
        "code" | "tt" | "kbd" | "samp" | "pre" => attrs.font_family = Some("Courier".into()),
        "a" => {
            if let Some(href) = attribute(tag, "href").filter(|h| !h.is_empty()) {
                attrs.link = Some(Some(Arc::from(href.as_str())));
            }
        }
        _ => {}
    }
    // Headings bold, and larger the higher they are.
    let heading = match name {
        "h1" => Some(24),
        "h2" => Some(18),
        "h3" => Some(14),
        "h4" | "h5" | "h6" => Some(12),
        _ => None,
    };
    if let Some(size) = heading {
        attrs.weight = Some(FontWeight::BOLD);
        attrs.font_size = Some(Length::from_points(size));
    }
    if name == "font" {
        if let Some(face) = attribute(tag, "face") {
            attrs.font_family = first_family(&face);
        }
        if let Some(color) = attribute(tag, "color").and_then(|c| color(&c)) {
            attrs.color = Some(color);
        }
    }
    let mut alignment = attribute(tag, "align").and_then(|a| align(&a));
    if let Some(style) = attribute(tag, "style") {
        for declaration in style.split(';') {
            let Some((property, value)) = declaration.split_once(':') else {
                continue;
            };
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim().trim_end_matches("!important").trim();
            let lower = value.to_ascii_lowercase();
            match property.as_str() {
                "font-weight" => {
                    attrs.weight = Some(match lower.as_str() {
                        "bold" | "bolder" => FontWeight::BOLD,
                        "normal" | "lighter" => FontWeight::NORMAL,
                        n => match n.parse::<u16>() {
                            Ok(n) if (100..=900).contains(&n) => FontWeight::new(n),
                            _ => continue,
                        },
                    })
                }
                "font-style" => attrs.italic = Some(lower == "italic" || lower == "oblique"),
                "text-decoration" | "text-decoration-line" => {
                    if lower.contains("underline") {
                        attrs.underline = Some(LineStyle::Single);
                    }
                    if lower.contains("line-through") {
                        attrs.strikethrough = Some(LineStyle::Single);
                    }
                    if lower == "none" {
                        attrs.underline = Some(LineStyle::None);
                        attrs.strikethrough = Some(LineStyle::None);
                    }
                }
                "font-size" => {
                    if let Some(size) = size(&lower) {
                        attrs.font_size = Some(size);
                    }
                }
                "font-family" => attrs.font_family = first_family(value),
                "color" => {
                    if let Some(c) = color(&lower) {
                        attrs.color = Some(c);
                    }
                }
                "text-align" => alignment = align(&lower).or(alignment),
                "vertical-align" => match lower.as_str() {
                    "super" => attrs.baseline = Some(Baseline::Superscript),
                    "sub" => attrs.baseline = Some(Baseline::Subscript),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    (attrs, alignment)
}

/// The value of an attribute of an opening tag, decoded.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        // The whole name, after a space.
        if !lower[..at].ends_with(char::is_whitespace) {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let rest = &rest[1..];
                &rest[..rest.find(quote).unwrap_or(rest.len())]
            }
            _ => {
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                    .unwrap_or(rest.len());
                &rest[..end]
            }
        };
        return Some(decode(value));
    }
    None
}

/// The first family of a CSS list of font families, as Desk names fonts.
fn first_family(value: &str) -> Option<Arc<str>> {
    let first = value.split(',').next()?.trim().trim_matches(['"', '\'']);
    let generic = match first.to_ascii_lowercase().as_str() {
        "serif" => Some("Times New Roman"),
        "sans-serif" => Some("Helvetica"),
        "monospace" => Some("Courier"),
        "" | "inherit" | "initial" => return None,
        _ => None,
    };
    Some(generic.unwrap_or(first).into())
}

/// An alignment of HTML or CSS.
fn align(value: &str) -> Option<Alignment> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" | "start" => Some(Alignment::Left),
        "center" => Some(Alignment::Center),
        "right" | "end" => Some(Alignment::Right),
        "justify" => Some(Alignment::Justify),
        _ => None,
    }
}

/// A size of CSS in points: in points, pixels, ems of 12 points or the
/// named sizes.
fn size(value: &str) -> Option<Length> {
    let named = match value {
        "xx-small" => Some(7.0),
        "x-small" => Some(7.5),
        "small" => Some(10.0),
        "medium" => Some(12.0),
        "large" => Some(13.5),
        "x-large" => Some(18.0),
        "xx-large" => Some(24.0),
        _ => None,
    };
    let points = named.or_else(|| {
        let number = |suffix: &str| value.strip_suffix(suffix)?.trim().parse::<f64>().ok();
        number("pt")
            .or_else(|| number("px").map(|px| px * 0.75))
            .or_else(|| number("em").map(|em| em * 12.0))
            .or_else(|| number("rem").map(|em| em * 12.0))
            .or_else(|| number("%").map(|p| p * 0.12))
    })?;
    (points.is_finite() && points > 0.0).then(|| Length::from_pt(points.min(1638.0)))
}

/// A color of HTML or CSS: `#rgb`, `#rrggbb`, `rgb(r, g, b)` or a few
/// names.
fn color(value: &str) -> Option<Rgba> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        let digit = |i: usize, len: usize| u8::from_str_radix(hex.get(i..i + len)?, 16).ok();
        return match hex.len() {
            3 => Some(Rgba::rgb(
                digit(0, 1)? * 17,
                digit(1, 1)? * 17,
                digit(2, 1)? * 17,
            )),
            6 | 8 => Some(Rgba::rgb(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?)),
            _ => None,
        };
    }
    if let Some(inside) = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))
    {
        let parts: Vec<u8> = inside
            .trim_end_matches(')')
            .split(',')
            .take(3)
            .map(|p| {
                p.trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| v.clamp(0.0, 255.0) as u8)
            })
            .collect::<Option<_>>()?;
        let &[r, g, b] = parts.as_slice() else {
            return None;
        };
        return Some(Rgba::rgb(r, g, b));
    }
    match value {
        "black" => Some(Rgba::BLACK),
        "white" => Some(Rgba::rgb(255, 255, 255)),
        "red" => Some(Rgba::rgb(255, 0, 0)),
        "green" => Some(Rgba::rgb(0, 128, 0)),
        "blue" => Some(Rgba::rgb(0, 0, 255)),
        "gray" | "grey" => Some(Rgba::rgb(128, 128, 128)),
        _ => None,
    }
}

/// Text of HTML with its character references replaced.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find(';').filter(|&e| e <= 10);
        let decoded = end.and_then(|end| {
            let name = &rest[1..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                "ndash" => Some('\u{2013}'),
                "mdash" => Some('\u{2014}'),
                "hellip" => Some('\u{2026}'),
                "lsquo" => Some('\u{2018}'),
                "rsquo" => Some('\u{2019}'),
                "ldquo" => Some('\u{201c}'),
                "rdquo" => Some('\u{201d}'),
                "copy" => Some('\u{a9}'),
                _ => {
                    let number = name.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)
                }
            }?;
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(fragment: &Fragment) -> Vec<Vec<(String, CharAttrs)>> {
        fragment
            .paragraphs
            .iter()
            .map(|p| {
                p.runs
                    .iter()
                    .map(|(t, _, a)| (t.clone(), a.clone()))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn txt_04_formatted_text_of_other_programs_keeps_its_emphasis() {
        let html = "<html><head><style>p { color: red }</style></head><body>\
            <!--StartFragment--><h1>Title</h1>\
            <p style=\"text-align: center\">Some <b>bold</b> and <i>italic</i> \
            text, <span style=\"font-size: 16px; color: #0000ff\">blue</span>.</p>\
            <ul><li>One&nbsp;&amp; <a href=\"https://example.com\">a link</a></li></ul>\
            <!--EndFragment--></body></html>";
        let fragment = read_html(html).unwrap();
        assert_eq!(
            fragment.text(),
            "Title\nSome bold and italic text, blue.\nOne\u{a0}& a link"
        );
        let runs = runs(&fragment);
        assert_eq!(runs[0][0].1.weight, Some(FontWeight::BOLD));
        assert_eq!(runs[0][0].1.font_size, Some(Length::from_points(24)));
        assert_eq!(
            runs[1][1],
            (
                "bold".into(),
                CharAttrs {
                    weight: Some(FontWeight::BOLD),
                    ..CharAttrs::default()
                }
            )
        );
        assert_eq!(runs[1][3].1.italic, Some(true));
        let blue = &runs[1][5].1;
        assert_eq!(blue.font_size, Some(Length::from_pt(12.0)));
        assert_eq!(blue.color, Some(Rgba::rgb(0, 0, 255)));
        assert_eq!(
            fragment.paragraphs[1].attrs.alignment,
            Some(Alignment::Center)
        );
        let link = &runs[2][1];
        assert_eq!(link.0, "a link");
        assert_eq!(link.1.link, Some(Some(Arc::from("https://example.com"))));
    }

    #[test]
    fn txt_04_text_copied_is_html_with_its_look() {
        let html = "<h1 align=center>A &amp; B</h1>\
            <p>one <i>two</i> <a href=\"https://example.org/?a=1&amp;b=2\">three</a> \
            <span style=\"color:#cc0000;font-family:Georgia;font-size:16pt\">four</span><br>five</p>";
        let written = write_html(&read_html(html).unwrap());
        assert!(written.starts_with("<meta http-equiv=\"content-type\""));
        assert!(written.contains("text-align:center"));
        assert!(written.contains("font-size:24pt\"><b>A &amp; B</b></span></p>"));
        assert!(written.contains("<i>two</i>"));
        assert!(written.contains("<a href=\"https://example.org/?a=1&amp;b=2\">"));
        assert!(
            written.contains("font-family:'Georgia';font-size:16pt;color:#cc0000\">four</span>")
        );
        assert!(written.contains("<br>five</span></p>"));
        // And Desk reads its own HTML as it was, with the font and the
        // size of every run said.
        let look = |fragment: &Fragment| -> Vec<_> {
            runs(fragment)
                .into_iter()
                .flatten()
                .map(|(text, a)| (text, a.weight, a.italic, a.color, a.link))
                .collect()
        };
        let again = read_html(&written).unwrap();
        assert_eq!(look(&again), look(&read_html(html).unwrap()));
        assert_eq!(
            runs(&again)[1][0].1.font_size,
            Some(Length::from_points(12))
        );
    }

    #[test]
    fn txt_04_lists_copied_are_lists_in_html() {
        let mut editor = Editor::new(Document::new_word_processing());
        let (bullets, numbers) = editor
            .edit("Setup", |edit| Ok((edit.new_id(), edit.new_id())))
            .unwrap();
        let body = desk_model::ParagraphStyle::new(bullets, "Body");
        let item = |text: &str, list: Option<desk_model::StyleId>, level: u8| FragmentParagraph {
            style: body.clone(),
            attrs: ParaAttrs {
                list_style: Some(list),
                list_level: Some(level),
                ..ParaAttrs::default()
            },
            runs: vec![(text.to_owned(), None, CharAttrs::default())],
        };
        let fragment = Fragment {
            paragraphs: vec![
                item("before", None, 0),
                item("a", Some(bullets), 0),
                item("b", Some(bullets), 1),
                item("c", Some(bullets), 0),
                item("one", Some(numbers), 0),
                item("after", None, 0),
            ],
            whole_start: true,
            lists: vec![
                desk_model::ListStyle::bullet(bullets, "Bullet"),
                desk_model::ListStyle::harvard(numbers, "Harvard"),
            ],
            notes: Vec::new(),
            objects: Vec::new(),
        };
        let written = write_html(&fragment);
        // The elements alone, without their styles.
        let mut shape = String::new();
        let mut rest = written.as_str();
        while let Some(at) = rest.find('<') {
            shape.push_str(&rest[..at]);
            rest = &rest[at..];
            let end = rest.find('>').unwrap();
            let name: String = rest[1..end]
                .chars()
                .take_while(|c| *c == '/' || c.is_ascii_alphanumeric())
                .collect();
            if !matches!(name.as_str(), "span" | "/span" | "meta") {
                shape.push_str(&format!("<{name}>"));
            }
            rest = &rest[end + 1..];
        }
        assert_eq!(
            shape,
            "<p>before</p><ul><li>a<ul><li>b</li></ul></li><li>c</li></ul>\
             <ol><li>one</li></ol><p>after</p>"
        );
        assert!(written.contains("<ol type=\"I\">"));
        assert_eq!(
            read_html(&written).unwrap().text(),
            "before\na\nb\nc\none\nafter"
        );
    }

    #[test]
    fn txt_04_line_breaks_and_preformatted_text_are_kept() {
        let fragment = read_html("one<br>two<pre>a  b\nc</pre>é").unwrap();
        assert_eq!(fragment.text(), format!("one{LINE_BREAK}two\na  b\nc\né"));
    }

    #[test]
    fn txt_04_hostile_html_reads_as_text() {
        let deep = "<b>".repeat(10_000) + "x" + "&#99999999;&;<";
        let fragment = read_html(&deep).unwrap();
        assert!(fragment.text().starts_with('x'));
    }

    #[test]
    fn txt_04_what_the_fuzzer_found_pastes() {
        let fragment = read_html("<pre>a\u{b}b\r\nc\u{2029}d</pre>").unwrap();
        assert_eq!(fragment.text(), "a\nb\nc\nd");
        let html = include_str!("../../../fuzz/seeds/html_paste/crash-vertical-tab.html");
        let fragment = read_html(html).unwrap();
        let mut editor = desk_model::Editor::new(desk_model::Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Paste", |edit| {
                edit.paste_fragment(start, &fragment).map(drop)
            })
            .unwrap();
    }
}
