// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Reading a text document from its XML streams.

use std::collections::HashMap;

use crate::error::OdfError;
use crate::ns::{CONFIG, DC, DESK, DRAW, FO, META, OFFICE, STYLE, SVG, TEXT, XLINK};
use crate::text::{
    Background, Border, BorderLine, Break, Column, Columns, Family, Field, HeaderFooter,
    HeaderFooterStyle, LINE_BREAK, LeaderStyle, Line, LineColor, LineHeight, ListLabel,
    ListLevelStyle, ListMember, ListStyle, MasterPage, Meta, Note, NotesConfiguration, PageLayout,
    Paragraph, ParagraphProperties, Properties, SectionStart, SectionStyle, Span, Style,
    StyleShortcut, TabStop, TabType, TextAlign, TextDocument, TextPosition, TextProperties,
    TextSettings, TextShadow, TextTransform, TocLevel,
};
use crate::units::{Color, Length, parse_percent};
use crate::xml::{self, Element, Node};

/// How many spaces one `text:s` may stand for.
const MAX_SPACES: usize = 1024;

/// Reads a text document from the content, styles and meta streams.
/// `styles` and `meta` may be missing.
pub fn read_streams(
    content: &str,
    styles: Option<&str>,
    meta: Option<&str>,
) -> Result<TextDocument, OdfError> {
    let content = xml::parse(content)?;
    if !content.name.is(OFFICE, "document-content") {
        return Err(OdfError::Malformed(
            "content.xml is not office:document-content".into(),
        ));
    }
    let styles = styles.map(xml::parse).transpose()?;
    let meta = meta.map(xml::parse).transpose()?;

    let mut fonts = HashMap::new();
    for root in styles.iter().chain([&content]) {
        if let Some(decls) = root.child(OFFICE, "font-face-decls") {
            for face in decls.children_named(STYLE, "font-face") {
                if let (Some(name), Some(family)) =
                    (face.attr(STYLE, "name"), face.attr(SVG, "font-family"))
                {
                    fonts.insert(name.to_owned(), unquote(family));
                }
            }
        }
    }
    let reader = Reader { fonts };

    let mut document = TextDocument::default();
    if let Some(styles) = &styles {
        if let Some(common) = styles.child(OFFICE, "styles") {
            for element in common.elements() {
                if element.name.is(STYLE, "default-style")
                    && element.attr(STYLE, "family") == Some("paragraph")
                {
                    document.default_style = reader.properties(element);
                } else if let Some(style) = reader.style(element) {
                    document.styles.push(style);
                } else if let Some(list) = reader.list_style(element) {
                    document.list_styles.push(list);
                } else if let Some(section) = section_style(element) {
                    document.section_styles.push(section);
                } else if let Some(gradient) = crate::draw::gradient(element) {
                    document.gradients.push(gradient);
                } else if let Some(marker) = crate::draw::marker(element) {
                    document.markers.push(marker);
                } else if element.name.is(TEXT, "notes-configuration") {
                    let attr = |ns, name| element.attr(ns, name).map(str::to_owned);
                    document.notes_configuration.push(NotesConfiguration {
                        class: attr(TEXT, "note-class").unwrap_or_else(|| "footnote".into()),
                        num_format: attr(STYLE, "num-format"),
                        start_numbering_at: attr(TEXT, "start-numbering-at"),
                        footnotes_position: attr(TEXT, "footnotes-position"),
                    });
                }
            }
        }
        document.page_layout = page_layout(styles).unwrap_or_default();
        if let Some(automatic) = styles.child(OFFICE, "automatic-styles") {
            document
                .master_styles
                .extend(automatic.elements().filter_map(|e| reader.style(e)));
        }
        if let Some(master) = master_page(styles) {
            document.master = read_master(master);
            let name = master.attr(STYLE, "name");
            if let Some(masters) = styles.child(OFFICE, "master-styles") {
                document.masters = masters
                    .children_named(STYLE, "master-page")
                    .filter(|m| m.attr(STYLE, "name") != name)
                    .filter_map(|m| Some((m.attr(STYLE, "name")?.to_owned(), read_master(m))))
                    .collect();
            }
        }
    }
    if let Some(automatic) = content.child(OFFICE, "automatic-styles") {
        document
            .automatic_styles
            .extend(automatic.elements().filter_map(|e| reader.style(e)));
        document
            .automatic_list_styles
            .extend(automatic.elements().filter_map(|e| reader.list_style(e)));
        document
            .section_styles
            .extend(automatic.elements().filter_map(section_style));
    }
    if let Some(text) = content
        .child(OFFICE, "body")
        .and_then(|body| body.child(OFFICE, "text"))
    {
        collect_paragraphs(
            text,
            None,
            None,
            &mut document.body,
            &mut document.page_objects,
        );
        document.toc_levels = toc_levels(text);
    }
    if let Some(meta) = meta.as_ref().and_then(|m| m.child(OFFICE, "meta")) {
        document.meta = read_meta(meta);
    }
    Ok(document)
}

/// The levels of the first table of contents of a body that lists
/// paragraph styles: each style it lists, with the template of its
/// entries at the same level.
fn toc_levels(body: &Element) -> Vec<TocLevel> {
    let mut source = None;
    visit(body, &mut |e| {
        if source.is_none() && e.name.is(TEXT, "table-of-content-source") {
            source = Some(e.clone());
        }
    });
    let Some(source) = source else {
        return Vec::new();
    };
    let level = |e: &Element| {
        e.attr(TEXT, "outline-level")
            .and_then(|l| l.trim().parse::<u8>().ok())
    };
    let mut out = Vec::new();
    for styles in source.children_named(TEXT, "index-source-styles") {
        let template = source
            .children_named(TEXT, "table-of-content-entry-template")
            .find(|t| level(t) == level(styles));
        for style in styles.children_named(TEXT, "index-source-style") {
            let Some(name) = style.attr(TEXT, "style-name") else {
                continue;
            };
            out.push(TocLevel {
                style: name.to_owned(),
                entry: template
                    .and_then(|t| t.attr(TEXT, "style-name"))
                    .unwrap_or_default()
                    .to_owned(),
                page_number: template
                    .is_some_and(|t| t.child(TEXT, "index-entry-page-number").is_some()),
                link: template.is_some_and(|t| t.child(TEXT, "index-entry-link-start").is_some()),
            });
        }
    }
    out
}

struct Reader {
    /// Font face names and their families.
    fonts: HashMap<String, String>,
}

impl Reader {
    fn style(&self, element: &Element) -> Option<Style> {
        if !element.name.is(STYLE, "style") {
            return None;
        }
        let family = match element.attr(STYLE, "family")? {
            "paragraph" => Family::Paragraph,
            "text" => Family::Text,
            "graphic" => Family::Graphic,
            _ => return None,
        };
        Some(Style {
            name: element.attr(STYLE, "name")?.to_owned(),
            display_name: element.attr(STYLE, "display-name").map(str::to_owned),
            family,
            parent: element.attr(STYLE, "parent-style-name").map(str::to_owned),
            next: element.attr(STYLE, "next-style-name").map(str::to_owned),
            list_style: element.attr(STYLE, "list-style-name").map(str::to_owned),
            master_page: element
                .attr(STYLE, "master-page-name")
                .filter(|n| !n.is_empty())
                .map(str::to_owned),
            properties: self.properties(element),
        })
    }

    /// A list style and its levels.
    fn list_style(&self, element: &Element) -> Option<ListStyle> {
        if !element.name.is(TEXT, "list-style") {
            return None;
        }
        let levels = element
            .elements()
            .filter_map(|e| {
                let level = e
                    .attr(TEXT, "level")
                    .and_then(|l| l.trim().parse::<u8>().ok())
                    .filter(|l| (1..=10).contains(l))?;
                let label = if e.name.is(TEXT, "list-level-style-bullet") {
                    ListLabel::Bullet(e.attr(TEXT, "bullet-char")?.chars().next()?)
                } else if e.name.is(TEXT, "list-level-style-number") {
                    let text =
                        |ns: &str, name: &str| e.attr(ns, name).unwrap_or_default().to_owned();
                    ListLabel::Number {
                        format: text(STYLE, "num-format"),
                        prefix: text(STYLE, "num-prefix"),
                        suffix: text(STYLE, "num-suffix"),
                        display_levels: e
                            .attr(TEXT, "display-levels")
                            .and_then(|l| l.trim().parse().ok())
                            .unwrap_or(1),
                        start: e
                            .attr(TEXT, "start-value")
                            .and_then(|l| l.trim().parse().ok())
                            .unwrap_or(1),
                    }
                } else if e.name.is(TEXT, "list-level-style-image") {
                    let properties = e.child(STYLE, "list-level-properties");
                    ListLabel::Image {
                        href: e.attr(XLINK, "href")?.to_owned(),
                        width: properties.and_then(|p| length(p, FO, "width")),
                        height: properties.and_then(|p| length(p, FO, "height")),
                        raise: length(e, DESK, "raise"),
                    }
                } else {
                    return None;
                };
                let properties = e.child(STYLE, "list-level-properties");
                let get = |name: &str| properties.and_then(|p| length(p, TEXT, name));
                let mut space_before = get("space-before");
                let mut min_label_width = get("min-label-width");
                // The label alignment mode of LibreOffice: the text at the
                // margin, the label at the margin plus the text indent.
                if let Some(alignment) =
                    properties.and_then(|p| p.child(STYLE, "list-level-label-alignment"))
                {
                    let margin = length(alignment, FO, "margin-left").unwrap_or(Length(0.0));
                    let indent = length(alignment, FO, "text-indent").unwrap_or(Length(0.0));
                    space_before = Some(Length(margin.0 + indent.0));
                    min_label_width = Some(Length(-indent.0));
                }
                Some(ListLevelStyle {
                    level,
                    label,
                    space_before,
                    min_label_width,
                    relative_size: e
                        .attr(TEXT, "bullet-relative-size")
                        .or_else(|| e.attr(DESK, "relative-size"))
                        .and_then(parse_percent),
                    text: e
                        .child(STYLE, "text-properties")
                        .map(|t| self.text_properties(t))
                        .unwrap_or_default(),
                })
            })
            .collect();
        Some(ListStyle {
            name: element.attr(STYLE, "name")?.to_owned(),
            display_name: element.attr(STYLE, "display-name").map(str::to_owned),
            levels,
        })
    }

    fn properties(&self, element: &Element) -> Properties {
        Properties {
            paragraph: element
                .child(STYLE, "paragraph-properties")
                .map(paragraph_properties)
                .unwrap_or_default(),
            text: element
                .child(STYLE, "text-properties")
                .map(|e| self.text_properties(e))
                .unwrap_or_default(),
            graphic: element
                .child(STYLE, "graphic-properties")
                .map(crate::draw::graphic_properties)
                .unwrap_or_default(),
        }
    }

    fn text_properties(&self, e: &Element) -> TextProperties {
        let font_family = e
            .attr(STYLE, "font-name")
            .and_then(|name| {
                self.fonts
                    .get(name)
                    .cloned()
                    .or_else(|| Some(name.to_owned()))
            })
            .or_else(|| e.attr(FO, "font-family").map(unquote));
        let size = e.attr(FO, "font-size");
        let (position, position_scale) = e
            .attr(STYLE, "text-position")
            .map(text_position)
            .unwrap_or((None, None));
        TextProperties {
            font_family,
            font_size: size.and_then(Length::parse),
            font_size_percent: size.and_then(parse_percent),
            font_weight: e.attr(FO, "font-weight").and_then(|w| match w {
                "normal" => Some(400),
                "bold" => Some(700),
                _ => w.parse().ok(),
            }),
            italic: e.attr(FO, "font-style").map(|s| s != "normal"),
            underline: line(e, "underline"),
            line_through: line(e, "line-through"),
            underline_color: e.attr(STYLE, "text-underline-color").and_then(|c| {
                if c == "font-color" {
                    Some(LineColor::FontColor)
                } else {
                    Color::parse(c).map(LineColor::Color)
                }
            }),
            line_through_color: e.attr(STYLE, "text-line-through-color").and_then(|c| {
                if c == "font-color" {
                    Some(LineColor::FontColor)
                } else {
                    Color::parse(c).map(LineColor::Color)
                }
            }),
            color: e.attr(FO, "color").and_then(Color::parse),
            position,
            position_scale,
            transform: e.attr(FO, "text-transform").and_then(|t| match t {
                "none" => Some(TextTransform::None),
                "uppercase" => Some(TextTransform::Uppercase),
                "lowercase" => Some(TextTransform::Lowercase),
                "capitalize" => Some(TextTransform::Capitalize),
                _ => None,
            }),
            small_caps: e.attr(FO, "font-variant").map(|v| v == "small-caps"),
            language: e.attr(FO, "language").map(str::to_owned),
            country: e.attr(FO, "country").map(str::to_owned),
            script: e.attr(FO, "script").map(str::to_owned),
            letter_spacing: e.attr(FO, "letter-spacing").and_then(|v| {
                if v == "normal" {
                    Some(Length(0.0))
                } else {
                    Length::parse(v)
                }
            }),
            outline: e.attr(STYLE, "text-outline").map(|v| v == "true"),
            shadow: e.attr(FO, "text-shadow").and_then(text_shadow),
            background: e.attr(FO, "background-color").and_then(background),
            hyphenate: e.attr(FO, "hyphenate").map(|v| v == "true"),
            desk_hyphenate: e.attr(DESK, "hyphenate").map(|v| v == "true"),
            shadow_opacity: e
                .attr(DESK, "shadow-opacity")
                .and_then(parse_percent)
                .map(|p| p.clamp(0.0, 100.0)),
            tracking: e
                .attr(DESK, "tracking")
                .and_then(|v| v.trim().parse::<i32>().ok())
                .map(|t| t.clamp(-1000, 10_000)),
        }
    }
}

/// `fo:text-shadow`: `none`, or the first shadow of a CSS list, whose
/// color may come first or last.
fn text_shadow(value: &str) -> Option<TextShadow> {
    let value = value.trim();
    if value == "none" {
        return Some(TextShadow::None);
    }
    let first = value.split(',').next()?;
    let mut lengths = Vec::new();
    let mut color = None;
    for part in first.split_whitespace() {
        if let Some(c) = Color::parse(part) {
            color = Some(c);
        } else {
            lengths.push(Length::parse(part)?);
        }
    }
    match lengths[..] {
        [dx, dy] => Some(TextShadow::Shadow {
            dx,
            dy,
            blur: None,
            color,
        }),
        [dx, dy, blur, ..] => Some(TextShadow::Shadow {
            dx,
            dy,
            blur: Some(blur),
            color,
        }),
        _ => None,
    }
}

/// `fo:background-color`.
fn background(value: &str) -> Option<Background> {
    if value.trim() == "transparent" {
        Some(Background::Transparent)
    } else {
        Color::parse(value).map(Background::Color)
    }
}

/// A side of `fo:border`: a width, a style and a color, in any order, or
/// `none`.
fn border(value: &str) -> Option<Border> {
    let mut width = None;
    let mut style = None;
    let mut color = None;
    for part in value.split_whitespace() {
        match part {
            "none" | "hidden" => return Some(Border::None),
            "solid" | "double" | "groove" | "ridge" | "inset" | "outset" => {
                style = Some(BorderLine::Solid)
            }
            "dashed" => style = Some(BorderLine::Dashed),
            "dotted" => style = Some(BorderLine::Dotted),
            "thin" => width = Some(Length(0.75)),
            "medium" => width = Some(Length(1.5)),
            "thick" => width = Some(Length(3.0)),
            _ => {
                if let Some(c) = Color::parse(part) {
                    color = Some(c);
                } else if let Some(l) = Length::parse(part) {
                    width = Some(l);
                }
            }
        }
    }
    let style = style?;
    let width = width.unwrap_or(Length(1.5));
    if width.0 <= 0.0 {
        return Some(Border::None);
    }
    Some(Border::Line {
        width,
        style,
        color: color.unwrap_or(Color { r: 0, g: 0, b: 0 }),
    })
}

/// The first family of a CSS font family list, as `svg:font-family` and
/// `fo:font-family` hold: a quoted string, with backslash escapes, or
/// the words up to the first comma.
fn unquote(families: &str) -> String {
    let families = families.trim_start();
    let mut chars = families.chars();
    match chars.next() {
        Some(quote @ ('\'' | '"')) => {
            let mut family = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => family.extend(chars.next()),
                    c if c == quote => break,
                    c => family.push(c),
                }
            }
            family
        }
        _ => families
            .split(',')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// A line under or through text, from its style and type attributes.
fn line(e: &Element, which: &str) -> Option<Line> {
    let style = e.attr(STYLE, &format!("text-{which}-style"));
    let kind = e.attr(STYLE, &format!("text-{which}-type"));
    if style.is_none() && kind.is_none() {
        return None;
    }
    Some(match (style, kind) {
        (Some("none"), _) | (_, Some("none")) => Line::None,
        (_, Some("double")) => Line::Double,
        _ => Line::Single,
    })
}

fn text_position(value: &str) -> (Option<TextPosition>, Option<f64>) {
    let mut parts = value.split_whitespace();
    let position = parts.next().and_then(|p| match p {
        "super" => Some(TextPosition::Super),
        "sub" => Some(TextPosition::Sub),
        _ => parse_percent(p).map(TextPosition::Percent),
    });
    // A scale means nothing without a position.
    let scale = position.and(parts.next().and_then(parse_percent));
    (position, scale)
}

fn length(e: &Element, ns: &str, name: &str) -> Option<Length> {
    e.attr(ns, name).and_then(Length::parse)
}

fn paragraph_properties(e: &Element) -> ParagraphProperties {
    let always = |name: &str| e.attr(FO, name).map(|v| v == "always");
    // A side of a property with sides, such as `fo:border-top`, which
    // takes the place of the property for all sides.
    let side = |name: &str, which: &str| {
        e.attr(FO, &format!("{name}-{which}"))
            .or_else(|| e.attr(FO, name))
    };
    ParagraphProperties {
        align: e.attr(FO, "text-align").and_then(|a| match a {
            "start" => Some(TextAlign::Start),
            "left" => Some(TextAlign::Left),
            "center" => Some(TextAlign::Center),
            "end" => Some(TextAlign::End),
            "right" => Some(TextAlign::Right),
            "justify" => Some(TextAlign::Justify),
            _ => None,
        }),
        line_height: e.attr(FO, "line-height").and_then(|h| {
            if h == "normal" {
                Some(LineHeight::Normal)
            } else if let Some(percent) = parse_percent(h) {
                Some(LineHeight::Percent(percent))
            } else {
                Length::parse(h).map(LineHeight::Exactly)
            }
        }),
        line_height_at_least: length(e, STYLE, "line-height-at-least"),
        line_spacing: length(e, STYLE, "line-spacing"),
        margin_top: length(e, FO, "margin-top"),
        margin_bottom: length(e, FO, "margin-bottom"),
        margin_left: length(e, FO, "margin-left"),
        margin_right: length(e, FO, "margin-right"),
        text_indent: length(e, FO, "text-indent"),
        break_before: e.attr(FO, "break-before").and_then(|b| match b {
            "auto" => Some(Break::Auto),
            "page" => Some(Break::Page),
            "column" => Some(Break::Column),
            _ => None,
        }),
        page_number: e.attr(STYLE, "page-number").and_then(|n| match n.trim() {
            "auto" => Some(None),
            n => n.parse().ok().map(Some),
        }),
        keep_with_next: always("keep-with-next"),
        keep_together: always("keep-together"),
        widows: e.attr(FO, "widows").and_then(|v| v.parse().ok()),
        orphans: e.attr(FO, "orphans").and_then(|v| v.parse().ok()),
        tab_stops: e.child(STYLE, "tab-stops").map(|stops| {
            stops
                .children_named(STYLE, "tab-stop")
                .filter_map(tab_stop)
                .collect()
        }),
        tab_stop_distance: length(e, STYLE, "tab-stop-distance"),
        background: e.attr(FO, "background-color").and_then(background),
        border_top: side("border", "top").and_then(border),
        border_bottom: side("border", "bottom").and_then(border),
        border_left: side("border", "left").and_then(border),
        border_right: side("border", "right").and_then(border),
        padding_top: side("padding", "top").and_then(Length::parse),
        padding_bottom: side("padding", "bottom").and_then(Length::parse),
        padding_left: side("padding", "left").and_then(Length::parse),
        padding_right: side("padding", "right").and_then(Length::parse),
    }
}

fn tab_stop(e: &Element) -> Option<TabStop> {
    let first_char = |name: &str| e.attr(STYLE, name).and_then(|v| v.chars().next());
    Some(TabStop {
        position: length(e, STYLE, "position")?,
        kind: match e.attr(STYLE, "type") {
            Some("center") => TabType::Center,
            Some("right") => TabType::Right,
            Some("char") => TabType::Char,
            _ => TabType::Left,
        },
        char: first_char("char"),
        leader_style: match e.attr(STYLE, "leader-style") {
            Some("solid") => LeaderStyle::Solid,
            Some("dotted") => LeaderStyle::Dotted,
            Some("dash" | "long-dash" | "dot-dash" | "dot-dot-dash" | "wave") => LeaderStyle::Dash,
            _ => LeaderStyle::None,
        },
        leader_text: first_char("leader-text").filter(|c| *c != ' '),
    })
}

/// Reads a boolean configuration item from the settings stream, if it is
/// there.
fn config_bool(settings: &str, name: &str) -> Option<bool> {
    let root = xml::parse(settings).ok()?;
    let mut found = None;
    visit(&root, &mut |e| {
        if e.name.is(CONFIG, "config-item") && e.attr(CONFIG, "name") == Some(name) {
            found = Some(e.text().trim() == "true");
        }
    });
    found
}

/// Reads `TabsRelativeToIndent` from the settings stream, if it is there.
pub(crate) fn tabs_relative_to_indent(settings: &str) -> Option<bool> {
    config_bool(settings, "TabsRelativeToIndent")
}

/// Reads the settings of the text that Desk keeps in the settings stream.
pub(crate) fn text_settings(settings: &str) -> TextSettings {
    let Ok(root) = xml::parse(settings) else {
        return TextSettings::default();
    };
    let mut out = TextSettings::default();
    visit(&root, &mut |e| {
        if !(e.name.is(CONFIG, "config-item-set")
            && e.attr(CONFIG, "name") == Some(crate::write::TEXT_SETTINGS))
        {
            return;
        }
        for item in e.children_named(CONFIG, "config-item") {
            let text = item.text();
            match item.attr(CONFIG, "name") {
                Some("Hyphenation") => out.hyphenation = Some(text.trim() == "true"),
                Some("Ligatures") => out.ligatures = Some(text.trim() == "true"),
                Some("LargerSpacing") => out.larger_spacing = Some(text.trim() == "true"),
                Some("NoteKind") => out.note_kind = Some(text.trim().to_owned()),
                Some("NoteFormat") => out.note_format = Some(text.trim().to_owned()),
                Some("NoteNumbering") => out.note_numbering = Some(text.trim().to_owned()),
                Some("NoteGap") => out.note_gap = Some(text.trim().to_owned()),
                Some("HeaderMargin") => out.header_margin = Some(text.trim().to_owned()),
                Some("FooterMargin") => out.footer_margin = Some(text.trim().to_owned()),
                Some("LinksInactive") => out.links_inactive = Some(text.trim() == "true"),
                Some("NeverHyphenate") => {
                    out.never_hyphenate = text
                        .lines()
                        .map(str::trim)
                        .filter(|w| !w.is_empty())
                        .take(100_000)
                        .map(str::to_owned)
                        .collect();
                }
                _ => {}
            }
        }
    });
    out
}

/// Reads the first view of the view settings in the settings stream.
pub(crate) fn view_settings(settings: &str) -> crate::text::ViewSettings {
    let mut out = crate::text::ViewSettings::default();
    let Ok(root) = xml::parse(settings) else {
        return out;
    };
    // The views are the entries of the map named Views, in the set of
    // the view settings.
    fn views(e: &Element) -> Option<&Element> {
        if e.name.is(CONFIG, "config-item-set")
            && e.attr(CONFIG, "name") == Some("ooo:view-settings")
        {
            return e
                .children_named(CONFIG, "config-item-map-indexed")
                .find(|m| m.attr(CONFIG, "name") == Some("Views"))?
                .children_named(CONFIG, "config-item-map-entry")
                .next();
        }
        e.elements().find_map(views)
    }
    let view = views(&root);
    let Some(view) = view else {
        return out;
    };
    for item in view.children_named(CONFIG, "config-item") {
        let text = item.text();
        let text = text.trim();
        match item.attr(CONFIG, "name") {
            Some("ZoomType") => out.zoom_type = text.parse().ok(),
            Some("ZoomFactor") => out.zoom_factor = text.parse().ok(),
            Some("ViewLayoutColumns") => out.columns = text.parse().ok(),
            Some("DeskOutlineView") => out.outline = Some(text == "true"),
            _ => {}
        }
    }
    out
}

/// Reads the shortcuts of styles from the settings stream.
pub(crate) fn style_shortcuts(settings: &str) -> Vec<StyleShortcut> {
    let Ok(root) = xml::parse(settings) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    visit(&root, &mut |e| {
        if !(e.name.is(CONFIG, "config-item-set")
            && e.attr(CONFIG, "name") == Some(crate::write::SHORTCUTS))
        {
            return;
        }
        for item in e.children_named(CONFIG, "config-item") {
            let key = item
                .attr(CONFIG, "name")
                .and_then(|n| n.strip_prefix('F'))
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|k| (1..=8).contains(k));
            let text = item.text();
            let (family, style) = match text.split_once(':') {
                Some(("paragraph", style)) => (Family::Paragraph, style),
                Some(("text", style)) => (Family::Text, style),
                Some(("list", style)) => (Family::List, style),
                _ => continue,
            };
            if let Some(key) = key
                && !style.is_empty()
            {
                found.push(StyleShortcut {
                    key,
                    family,
                    style: style.to_owned(),
                });
            }
        }
    });
    found
}

fn visit(e: &Element, f: &mut impl FnMut(&Element)) {
    f(e);
    for child in e.elements() {
        visit(child, f);
    }
}

/// The master page "Standard", or the first one.
fn master_page(styles: &Element) -> Option<&Element> {
    let masters = styles.child(OFFICE, "master-styles")?;
    masters
        .children_named(STYLE, "master-page")
        .find(|m| m.attr(STYLE, "name") == Some("Standard"))
        .or_else(|| masters.child(STYLE, "master-page"))
}

/// The headers and footers of a master page.
fn read_master(master: &Element) -> MasterPage {
    let part = |name: &str| {
        master.child(STYLE, name).map(|e| {
            let mut paragraphs = Vec::new();
            // Objects in headers and footers are not read yet.
            collect_paragraphs(e, None, None, &mut paragraphs, &mut Vec::new());
            HeaderFooter {
                display: e.attr(STYLE, "display") != Some("false"),
                paragraphs,
            }
        })
    };
    MasterPage {
        header: part("header"),
        header_first: part("header-first"),
        header_left: part("header-left"),
        footer: part("footer"),
        footer_first: part("footer-first"),
        footer_left: part("footer-left"),
    }
}

/// The size of the header or footer of a page layout.
fn header_footer_style(layout: &Element, name: &str, spacing: &str) -> Option<HeaderFooterStyle> {
    let p = layout
        .child(STYLE, name)?
        .child(STYLE, "header-footer-properties")?;
    Some(HeaderFooterStyle {
        min_height: length(p, FO, "min-height").or_else(|| length(p, SVG, "height")),
        spacing: length(p, FO, spacing),
    })
}

/// The page layout of the master page "Standard", or of the first one.
fn page_layout(styles: &Element) -> Option<PageLayout> {
    let master = master_page(styles)?;
    let name = master.attr(STYLE, "page-layout-name")?;
    let layout = styles
        .child(OFFICE, "automatic-styles")?
        .children_named(STYLE, "page-layout")
        .find(|l| l.attr(STYLE, "name") == Some(name))?;
    let p = layout.child(STYLE, "page-layout-properties")?;
    Some(PageLayout {
        width: length(p, FO, "page-width"),
        height: length(p, FO, "page-height"),
        margin_top: length(p, FO, "margin-top"),
        margin_bottom: length(p, FO, "margin-bottom"),
        margin_left: length(p, FO, "margin-left"),
        margin_right: length(p, FO, "margin-right"),
        mirrored: layout.attr(STYLE, "page-usage").map(|u| u == "mirrored"),
        columns: p.child(STYLE, "columns").and_then(columns),
        header: header_footer_style(layout, "header-style", "margin-bottom"),
        footer: header_footer_style(layout, "footer-style", "margin-top"),
    })
}

/// The columns of a section or of the pages, when there are more than
/// one.
fn columns(e: &Element) -> Option<Columns> {
    let count = e
        .attr(FO, "column-count")
        .and_then(|c| c.trim().parse::<u32>().ok())
        .unwrap_or(1)
        .min(u32::from(u8::MAX)) as u8;
    let columns: Vec<Column> = e
        .children_named(STYLE, "column")
        .filter_map(|c| {
            Some(Column {
                rel_width: c
                    .attr(STYLE, "rel-width")?
                    .trim()
                    .trim_end_matches('*')
                    .parse()
                    .ok()?,
                start_indent: length(c, FO, "start-indent").unwrap_or_default(),
                end_indent: length(c, FO, "end-indent").unwrap_or_default(),
            })
        })
        .collect();
    (count > 1).then(|| Columns {
        count,
        gap: length(e, FO, "column-gap"),
        columns,
    })
}

/// A section style.
fn section_style(e: &Element) -> Option<SectionStyle> {
    if !e.name.is(STYLE, "style") || e.attr(STYLE, "family") != Some("section") {
        return None;
    }
    let mut style = SectionStyle {
        name: e.attr(STYLE, "name")?.to_owned(),
        ..SectionStyle::default()
    };
    if let Some(p) = e.child(STYLE, "section-properties") {
        let flag = |name: &str| p.attr(DESK, name).map(|v| v == "true");
        style.margin_left = length(p, FO, "margin-left");
        style.margin_right = length(p, FO, "margin-right");
        style.columns = p.child(STYLE, "columns").and_then(columns);
        style.space_before = length(p, DESK, "space-before");
        style.space_after = length(p, DESK, "space-after");
        style.new_page = flag("new-page");
        style.section_start = p.attr(DESK, "section-start").and_then(|v| match v {
            "any" => Some(SectionStart::Any),
            "left" => Some(SectionStart::Left),
            "right" => Some(SectionStart::Right),
            _ => None,
        });
        style.first_page_different = flag("first-page-different");
        style.left_right_different = flag("left-right-different");
    }
    Some(style)
}

fn read_meta(meta: &Element) -> Meta {
    let text = |ns: &str, name: &str| meta.child(ns, name).map(Element::text);
    Meta {
        title: text(DC, "title"),
        subject: text(DC, "subject"),
        description: text(DC, "description"),
        keywords: meta
            .children_named(META, "keyword")
            .map(Element::text)
            .collect(),
        initial_creator: text(META, "initial-creator"),
        creator: text(DC, "creator"),
        generator: text(META, "generator"),
    }
}

/// Where the paragraphs being collected are in a list.
#[derive(Clone)]
struct InList {
    /// The list style, from the nearest list that names one.
    style: Option<String>,
    /// The level, from 0.
    level: u8,
    /// `text:start-value` of the item.
    start: Option<u32>,
    /// `text:style-override` of the item.
    style_override: Option<String>,
    /// `text:continue-numbering` of the outermost list.
    continues: bool,
    /// Whether no paragraph of the outermost list was met yet; shared by
    /// the nested lists.
    first: std::rc::Rc<std::cell::Cell<bool>>,
    /// Whether this is a list header, whose paragraphs have no label.
    header: bool,
}

/// Collects the paragraphs inside an element, looking into lists,
/// sections and tables for theirs. `section` is the style of the section
/// the element is in, or an empty name outside any.
fn collect_paragraphs(
    element: &Element,
    list: Option<&InList>,
    section: Option<&str>,
    out: &mut Vec<Paragraph>,
    objects: &mut Vec<crate::draw::DrawObject>,
) {
    // After a section inside this element, the next paragraph goes back
    // to the section of the element.
    let mut resume = false;
    for child in element.elements() {
        let name = &child.name;
        if name.is(TEXT, "section") {
            let style = child.attr(TEXT, "style-name").unwrap_or_default();
            let first = out.len();
            collect_paragraphs(child, list, Some(style), out, objects);
            // The innermost section that starts at a paragraph names it.
            if let Some(p) = out.get_mut(first)
                && p.section.is_none()
            {
                p.section = Some(style.to_owned());
            }
            resume |= out.len() > first;
            continue;
        }
        let first = out.len();
        if name.is(TEXT, "p") || name.is(TEXT, "h") {
            let mut paragraph = paragraph(child);
            if let Some(list) = list.filter(|l| !l.header)
                && let Some(style) = &list.style
            {
                paragraph.list = Some(ListMember {
                    style: style.clone(),
                    level: list.level,
                    start: list.start,
                    style_override: list.style_override.clone(),
                    first: list.first.replace(false),
                    continues: list.continues,
                });
            }
            out.push(paragraph);
        } else if name.is(TEXT, "list") {
            let style = child.attr(TEXT, "style-name").map(str::to_owned);
            let inner = match list {
                Some(outer) => InList {
                    style: style.or_else(|| outer.style.clone()),
                    level: (outer.level + 1).min(9),
                    start: None,
                    style_override: None,
                    ..outer.clone()
                },
                None => InList {
                    style,
                    level: 0,
                    start: None,
                    style_override: None,
                    continues: child.attr(TEXT, "continue-numbering") == Some("true")
                        || child.attr(TEXT, "continue-list").is_some(),
                    first: std::rc::Rc::new(std::cell::Cell::new(true)),
                    header: false,
                },
            };
            for item in child.elements() {
                let item_list = InList {
                    start: item
                        .attr(TEXT, "start-value")
                        .and_then(|v| v.trim().parse().ok()),
                    style_override: item.attr(TEXT, "style-override").map(str::to_owned),
                    header: item.name.is(TEXT, "list-header"),
                    ..inner.clone()
                };
                collect_paragraphs(item, Some(&item_list), section, out, objects);
            }
        } else if name.is(TEXT, "table-of-content") {
            // The entries, after the title of the table if it has one.
            if let Some(body) = child.child(TEXT, "index-body") {
                for part in body.elements() {
                    if part.name.is(TEXT, "p") || part.name.is(TEXT, "h") {
                        let mut paragraph = paragraph(part);
                        paragraph.toc = true;
                        out.push(paragraph);
                    } else {
                        collect_paragraphs(part, list, section, out, objects);
                    }
                }
            }
        } else if name.ns == DRAW {
            // Objects outside paragraphs, as those anchored to pages.
            objects.extend(crate::draw::draw_object(child, &paragraph));
        } else if name.is(OFFICE, "annotation") {
            // Comments are not part of the text flow.
        } else {
            collect_paragraphs(child, list, section, out, objects);
        }
        if resume && let Some(p) = out.get_mut(first) {
            if p.section.is_none() {
                p.section = Some(section.unwrap_or_default().to_owned());
            }
            resume = false;
        }
    }
}

/// Builds the spans of a paragraph, collapsing white space as ODF says:
/// runs of spaces, tabs and newlines in the XML are one space, and none at
/// the start of the paragraph.
struct SpanBuilder {
    spans: Vec<Span>,
    collapse: bool,
    /// The link and the bookmark that the text read now is in.
    link: Option<String>,
    bookmark: Option<String>,
    /// The bookmarks of one point met.
    marks: Vec<String>,
    /// The drawing objects met.
    frames: Vec<crate::draw::DrawObject>,
}

impl SpanBuilder {
    fn push(&mut self, style: Option<&str>, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.spans.last_mut() {
            Some(last)
                if last.style.as_deref() == style
                    && last.field.is_none()
                    && last.link == self.link
                    && last.bookmark == self.bookmark =>
            {
                last.text.push_str(text);
            }
            _ => self.spans.push(Span {
                style: style.map(str::to_owned),
                text: text.to_owned(),
                field: None,
                link: self.link.clone(),
                bookmark: self.bookmark.clone(),
            }),
        }
    }

    /// Adds a field with the text it shows.
    fn field(&mut self, style: Option<&str>, e: &Element, field: Field) {
        self.spans.push(Span {
            style: style.map(str::to_owned),
            text: e.text(),
            field: Some(field),
            link: self.link.clone(),
            bookmark: self.bookmark.clone(),
        });
        self.collapse = false;
    }

    /// Adds a note, with its citation as the text it shows.
    fn note(&mut self, style: Option<&str>, e: &Element) {
        let citation = e.child(TEXT, "note-citation");
        let mut body = Vec::new();
        if let Some(b) = e.child(TEXT, "note-body") {
            for child in b.elements() {
                if child.name.is(TEXT, "p") || child.name.is(TEXT, "h") {
                    body.push(paragraph(child));
                }
            }
        }
        let note = Note {
            id: e.attr(TEXT, "id").map(str::to_owned),
            endnote: e.attr(TEXT, "note-class") == Some("endnote"),
            label: citation
                .and_then(|c| c.attr(TEXT, "label"))
                .map(str::to_owned),
            body,
        };
        self.spans.push(Span {
            style: style.map(str::to_owned),
            text: citation.map(Element::text).unwrap_or_default(),
            field: Some(Field::Note(Box::new(note))),
            link: self.link.clone(),
            bookmark: self.bookmark.clone(),
        });
        self.collapse = false;
    }

    fn xml_text(&mut self, style: Option<&str>, text: &str) {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                if !self.collapse {
                    out.push(' ');
                    self.collapse = true;
                }
            } else {
                out.push(c);
                self.collapse = false;
            }
        }
        self.push(style, &out);
    }

    fn literal(&mut self, style: Option<&str>, text: &str) {
        self.push(style, text);
        self.collapse = false;
    }

    fn content(&mut self, element: &Element, style: Option<&str>) {
        for node in &element.children {
            match node {
                Node::Text(text) => self.xml_text(style, text),
                Node::Element(child) => self.inline(child, style),
            }
        }
    }

    fn inline(&mut self, e: &Element, style: Option<&str>) {
        let name = &e.name;
        if name.ns == TEXT {
            match name.local.as_str() {
                "span" => return self.content(e, e.attr(TEXT, "style-name").or(style)),
                "s" => {
                    let count = e
                        .attr(TEXT, "c")
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(1usize)
                        .clamp(1, MAX_SPACES);
                    return self.literal(style, &" ".repeat(count));
                }
                "tab" => return self.literal(style, "\t"),
                "line-break" => return self.literal(style, &LINE_BREAK.to_string()),
                // The page number of another page is only its text.
                "page-number" if e.attr(TEXT, "select-page").is_none_or(|p| p == "current") => {
                    let num_format = e.attr(STYLE, "num-format").map(str::to_owned);
                    return self.field(style, e, Field::PageNumber { num_format });
                }
                "page-count" => {
                    let num_format = e.attr(STYLE, "num-format").map(str::to_owned);
                    return self.field(style, e, Field::PageCount { num_format });
                }
                "date" => {
                    let pattern = e.attr(DESK, "pattern").map(str::to_owned);
                    let fixed = e.attr(TEXT, "fixed") == Some("true");
                    return self.field(style, e, Field::Date { pattern, fixed });
                }
                "file-name" => {
                    let display = e.attr(TEXT, "display").unwrap_or("full").to_owned();
                    return self.field(style, e, Field::FileName { display });
                }
                "note" => return self.note(style, e),
                "a" => {
                    let outer = self.link.take();
                    self.link = e.attr(XLINK, "href").map(str::to_owned);
                    self.content(e, style);
                    self.link = outer;
                    return;
                }
                "bookmark" => {
                    if let Some(name) = e.attr(TEXT, "name") {
                        self.marks.push(name.to_owned());
                    }
                    return;
                }
                "bookmark-start" => {
                    self.bookmark = e.attr(TEXT, "name").map(str::to_owned);
                    return;
                }
                "bookmark-end" => {
                    if self.bookmark.as_deref() == e.attr(TEXT, "name") {
                        self.bookmark = None;
                    }
                    return;
                }
                "bookmark-ref" if e.attr(TEXT, "reference-format") == Some("page") => {
                    let name = e.attr(TEXT, "ref-name").unwrap_or_default().to_owned();
                    return self.field(style, e, Field::PageOf { name });
                }
                // Reference marks and the like add no text of their own.
                "reference-mark"
                | "reference-mark-start"
                | "reference-mark-end"
                | "soft-page-break"
                | "change"
                | "change-start"
                | "change-end"
                | "tracked-changes" => return,
                _ => {}
            }
        }
        if name.ns == DRAW {
            match crate::draw::draw_object(e, &paragraph) {
                Some(object) if object.anchor.as_deref() == Some("as-char") => {
                    // In the line of the text, where it is.
                    self.spans.push(Span {
                        style: style.map(str::to_owned),
                        text: String::new(),
                        field: Some(Field::Object(Box::new(object))),
                        link: self.link.clone(),
                        bookmark: self.bookmark.clone(),
                    });
                    self.collapse = false;
                }
                Some(object) => self.frames.push(object),
                None => {}
            }
            return;
        }
        if name.is(OFFICE, "annotation") || name.is(OFFICE, "annotation-end") {
            return;
        }
        // Links, fields and anything else: their text.
        self.content(e, style);
    }
}

fn paragraph(e: &Element) -> Paragraph {
    let mut builder = SpanBuilder {
        spans: Vec::new(),
        collapse: true,
        link: None,
        bookmark: None,
        marks: Vec::new(),
        frames: Vec::new(),
    };
    builder.content(e, None);
    Paragraph {
        style: e.attr(TEXT, "style-name").map(str::to_owned),
        outline_level: e.name.is(TEXT, "h").then(|| {
            e.attr(TEXT, "outline-level")
                .and_then(|l| l.parse().ok())
                .unwrap_or(1)
        }),
        list: None,
        section: None,
        marks: builder.marks,
        toc: false,
        spans: builder.spans,
        frames: builder.frames,
    }
}
