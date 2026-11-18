// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The object styles of a document: named looks of shapes, lines, text
//! boxes and pictures, and the one of each kind that a new object takes
//! (MOD-04, MOD-07, VMI-02).
//!
//! Pages '09 keeps a default style for each kind of object, which
//! Format > Advanced > Define Default Style sets from the selection and
//! Reapply Defaults to Selection gives back. Desk keeps those defaults
//! among named styles, six of each kind in a new document, as the plan of
//! interface B draws them in the format bar and the Inspector. An object
//! remembers the style it took, so that redefining the style changes the
//! objects that still look as it did.

use std::sync::Arc;

use imbl::OrdMap;

use crate::error::ModelError;
use crate::format::{BorderStyle, CharAttrs, ParaAttrs};
use crate::id::NodeId;
use crate::object::{
    Fill, FrameStyle, GraphicStyle, Object, ObjectKind, ObjectShadow, PictureFrame, Stroke,
    VerticalAlignment, Wrap,
};
use crate::op::Op;
use crate::style::StyleId;
use crate::transaction::Edit;
use crate::units::{Length, Rgba};

/// The kind of object that an object style is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectStyleKind {
    /// A shape with an inside.
    Shape,
    /// A line or a connection line.
    Line,
    /// A text box.
    TextBox,
    /// A picture.
    Image,
}

impl ObjectStyleKind {
    /// Every kind, in the order of the menus.
    pub const ALL: [Self; 4] = [Self::Shape, Self::Line, Self::TextBox, Self::Image];

    /// The kind of styles that an object takes, or none for a movie, a
    /// sound or a group, which have none.
    pub fn of(kind: &ObjectKind) -> Option<Self> {
        match kind {
            ObjectKind::Shape(shape) if shape.is_line() => Some(Self::Line),
            ObjectKind::Shape(crate::Shape::Connection(_)) => Some(Self::Line),
            ObjectKind::Shape(_) => Some(Self::Shape),
            ObjectKind::TextBox(_) => Some(Self::TextBox),
            ObjectKind::Image(_) => Some(Self::Image),
            ObjectKind::Media(_)
            | ObjectKind::Group(_)
            | ObjectKind::Table(_)
            | ObjectKind::Chart(_) => None,
        }
    }

    /// Its name in files.
    pub fn name(self) -> &'static str {
        match self {
            Self::Shape => "shape",
            Self::Line => "line",
            Self::TextBox => "text-box",
            Self::Image => "image",
        }
    }

    /// The kind of a name in files.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// A named look for objects of a kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectStyle {
    /// The identifier.
    pub id: StyleId,
    /// The name shown to the user.
    pub name: Arc<str>,
    /// The kind of object it is for.
    pub kind: ObjectStyleKind,
    /// How it fills and outlines the object.
    pub graphic: GraphicStyle,
}

impl ObjectStyle {
    /// Whether an object looks as the style says. The ends of a line are
    /// the line's own, and do not count.
    pub fn matches(&self, graphic: &GraphicStyle) -> bool {
        self.applied_to(graphic) == *graphic
    }

    /// The look of an object that takes the style, from its look before:
    /// a line keeps its ends.
    pub fn applied_to(&self, before: &GraphicStyle) -> GraphicStyle {
        let mut graphic = self.graphic.clone();
        if self.kind == ObjectStyleKind::Line
            && let (Some(stroke), Some(old)) = (graphic.stroke.as_mut(), before.stroke)
        {
            stroke.start = old.start;
            stroke.end = old.end;
        }
        graphic
    }
}

/// What a new text box of a document takes besides the look of its
/// default style: the room around its text, where the text sits, how the
/// body goes around it and how its text is formatted, as Format >
/// Advanced > Set as Default Text Box Appearance of Pages 15.4 keeps them
/// (PXV-10). Its size is not part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBoxDefault {
    /// The room between the frame and the text.
    pub inset: Length,
    /// Where the text sits in the frame.
    pub vertical: VerticalAlignment,
    /// How the text of the body goes around the box.
    pub wrap: Wrap,
    /// The paragraph style of its text, or the default one of the
    /// document when none.
    pub paragraph_style: Option<StyleId>,
    /// The paragraph formatting of its text.
    pub paragraph: ParaAttrs,
    /// The character formatting of its text.
    pub character: CharAttrs,
}

impl Default for TextBoxDefault {
    /// A new text box of Desk until a document says otherwise: 4 pt
    /// around the text, at the top, in front of the body, with the text
    /// of the default paragraph style.
    fn default() -> Self {
        Self {
            inset: Length::from_points(4),
            vertical: VerticalAlignment::default(),
            wrap: Wrap::Through,
            paragraph_style: None,
            paragraph: ParaAttrs::default(),
            character: CharAttrs::default(),
        }
    }
}

/// The object styles of a document, the default of each kind, and what a
/// new text box takes besides its look.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectStyles {
    styles: OrdMap<StyleId, ObjectStyle>,
    defaults: OrdMap<ObjectStyleKind, StyleId>,
    text_box: TextBoxDefault,
}

/// The look of a style of the library: a fill, a line, a shadow, a
/// reflection and a picture frame.
fn look(
    fill: Option<Rgba>,
    stroke: Option<(Rgba, f64, BorderStyle)>,
    shadow: bool,
    reflection: Option<u8>,
    frame: Option<FrameStyle>,
) -> GraphicStyle {
    GraphicStyle {
        fill: fill.map(Fill::Color),
        stroke: stroke.map(|(color, width, style)| Stroke {
            style,
            ..Stroke::new(color, Length::from_pt(width))
        }),
        shadow: shadow.then(ObjectShadow::default),
        reflection,
        frame: frame.map(PictureFrame::new),
        ..GraphicStyle::default()
    }
}

/// The six styles of each kind in a new document, by their names, the
/// first being the default. The first shape, line and text box are the
/// look that Desk always gave new objects.
pub fn library(kind: ObjectStyleKind) -> [(&'static str, GraphicStyle); 6] {
    use BorderStyle::{Dashed, Dotted, Solid};
    let rgb = Rgba::rgb;
    let black = Rgba::BLACK;
    match kind {
        ObjectStyleKind::Shape => [
            (
                "Basic",
                look(
                    Some(rgb(0x6c, 0x9f, 0xd8)),
                    Some((black, 1.0, Solid)),
                    false,
                    None,
                    None,
                ),
            ),
            (
                "Blue",
                look(Some(rgb(0x35, 0x84, 0xe4)), None, true, None, None),
            ),
            (
                "Green",
                look(Some(rgb(0x2e, 0xc2, 0x7e)), None, true, None, None),
            ),
            (
                "Orange",
                look(Some(rgb(0xff, 0x78, 0x00)), None, true, None, None),
            ),
            (
                "Red Outline",
                look(
                    None,
                    Some((rgb(0xe0, 0x1b, 0x24), 3.0, Solid)),
                    false,
                    None,
                    None,
                ),
            ),
            (
                "Gray",
                look(
                    Some(rgb(0xde, 0xdd, 0xda)),
                    Some((rgb(0x77, 0x76, 0x7b), 1.0, Solid)),
                    false,
                    None,
                    None,
                ),
            ),
        ],
        ObjectStyleKind::Line => [
            (
                "Basic",
                look(None, Some((black, 1.0, Solid)), false, None, None),
            ),
            (
                "Thick",
                look(None, Some((black, 3.0, Solid)), false, None, None),
            ),
            (
                "Dashed",
                look(
                    None,
                    Some((rgb(0x5e, 0x5c, 0x64), 1.0, Dashed)),
                    false,
                    None,
                    None,
                ),
            ),
            (
                "Dotted",
                look(
                    None,
                    Some((rgb(0x5e, 0x5c, 0x64), 2.0, Dotted)),
                    false,
                    None,
                    None,
                ),
            ),
            (
                "Blue",
                look(
                    None,
                    Some((rgb(0x35, 0x84, 0xe4), 2.0, Solid)),
                    false,
                    None,
                    None,
                ),
            ),
            (
                "Red",
                look(
                    None,
                    Some((rgb(0xe0, 0x1b, 0x24), 2.0, Solid)),
                    false,
                    None,
                    None,
                ),
            ),
        ],
        ObjectStyleKind::TextBox => [
            ("Plain", look(None, None, false, None, None)),
            (
                "Outline",
                look(None, Some((black, 1.0, Solid)), false, None, None),
            ),
            (
                "Gray",
                look(Some(rgb(0xf6, 0xf5, 0xf4)), None, false, None, None),
            ),
            (
                "Yellow",
                look(Some(rgb(0xf9, 0xf0, 0x6b)), None, false, None, None),
            ),
            (
                "Light Blue",
                look(Some(rgb(0x99, 0xc1, 0xf1)), None, false, None, None),
            ),
            (
                "Card",
                look(
                    Some(Rgba::WHITE),
                    Some((rgb(0xc0, 0xbf, 0xbc), 1.0, Solid)),
                    true,
                    None,
                    None,
                ),
            ),
        ],
        ObjectStyleKind::Image => [
            ("Plain", look(None, None, false, None, None)),
            (
                "Thin Border",
                look(None, Some((black, 1.0, Solid)), false, None, None),
            ),
            ("Shadow", look(None, None, true, None, None)),
            (
                "Reflection",
                look(None, None, false, Some(GraphicStyle::REFLECTION), None),
            ),
            (
                "Photo",
                look(None, None, false, None, Some(FrameStyle::Photo)),
            ),
            (
                "Gallery",
                look(None, None, false, None, Some(FrameStyle::Gallery)),
            ),
        ],
    }
}

impl ObjectStyles {
    /// What a new text box takes besides its look (PXV-10).
    pub fn text_box_default(&self) -> &TextBoxDefault {
        &self.text_box
    }

    /// Sets what a new text box takes besides its look (PXV-10).
    pub fn set_text_box_default(&mut self, default: TextBoxDefault) {
        self.text_box = default;
    }

    /// The styles of the library, with identifiers from `new_id`: six
    /// of each kind, the first of each the default.
    pub fn library(mut new_id: impl FnMut() -> NodeId) -> Self {
        let mut styles = Self::default();
        for kind in ObjectStyleKind::ALL {
            for (index, (name, graphic)) in library(kind).into_iter().enumerate() {
                let id = new_id();
                styles.styles.insert(
                    id,
                    ObjectStyle {
                        id,
                        name: name.into(),
                        kind,
                        graphic,
                    },
                );
                if index == 0 {
                    styles.defaults.insert(kind, id);
                }
            }
        }
        styles
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

    /// A style by identifier.
    pub fn get(&self, id: StyleId) -> Option<&ObjectStyle> {
        self.styles.get(&id)
    }

    /// Every style, in the order they were made.
    pub fn iter(&self) -> impl Iterator<Item = &ObjectStyle> {
        self.styles.values()
    }

    /// The styles of a kind, in the order they were made.
    pub fn of_kind(&self, kind: ObjectStyleKind) -> impl Iterator<Item = &ObjectStyle> {
        self.iter().filter(move |s| s.kind == kind)
    }

    /// The style of a kind that new objects take, if it has any.
    pub fn default_for(&self, kind: ObjectStyleKind) -> Option<&ObjectStyle> {
        self.defaults
            .get(&kind)
            .and_then(|id| self.get(*id))
            .or_else(|| self.of_kind(kind).next())
    }

    /// Whether a style is the default of its kind.
    pub fn is_default(&self, id: StyleId) -> bool {
        self.get(id)
            .and_then(|s| self.default_for(s.kind))
            .is_some_and(|d| d.id == id)
    }

    /// A style by its kind and name.
    pub fn by_name(&self, kind: ObjectStyleKind, name: &str) -> Option<&ObjectStyle> {
        self.of_kind(kind).find(|s| &*s.name == name)
    }

    /// A name that no style of a kind has: `name`, or `name` and a
    /// number.
    pub fn unique_name(&self, kind: ObjectStyleKind, name: &str) -> String {
        if self.by_name(kind, name).is_none() {
            return name.to_owned();
        }
        (2..)
            .map(|n| format!("{name} {n}"))
            .find(|n| self.by_name(kind, n).is_none())
            .unwrap_or_else(|| name.to_owned())
    }

    /// Puts a style, in place of the one with its identifier; a kind
    /// with no default gets it as its default.
    pub fn insert(&mut self, style: ObjectStyle) {
        self.defaults.entry(style.kind).or_insert(style.id);
        self.styles.insert(style.id, style);
    }

    /// Makes a style the default of its kind.
    pub fn set_default(&mut self, id: StyleId) {
        if let Some(style) = self.get(id) {
            self.defaults.insert(style.kind, id);
        }
    }

    /// Takes a style out; the default of its kind goes to the first one
    /// left.
    pub fn remove(&mut self, id: StyleId) -> Option<ObjectStyle> {
        let style = self.styles.remove(&id)?;
        if self.defaults.get(&style.kind) == Some(&id) {
            self.defaults.remove(&style.kind);
            let first = self.of_kind(style.kind).next().map(|s| s.id);
            if let Some(first) = first {
                self.defaults.insert(style.kind, first);
            }
        }
        Some(style)
    }
}

/// Gives an object the look of a style of its kind and remembers the
/// style.
pub fn apply_object_style(object: &mut Object, style: &ObjectStyle) {
    object.style = style.applied_to(&object.style);
    object.object_style = Some(style.id);
}

impl crate::Document {
    /// The object styles.
    pub fn object_styles(&self) -> &ObjectStyles {
        &self.object_styles
    }

    /// Gives an object the default style of its kind, when the document
    /// has one, as a new object takes it (MOD-04, MOD-07). False when it
    /// has none.
    pub fn give_default_style(&self, object: &mut Object) -> bool {
        let Some(style) =
            ObjectStyleKind::of(&object.kind).and_then(|kind| self.object_styles.default_for(kind))
        else {
            return false;
        };
        apply_object_style(object, style);
        true
    }

    /// The style that an object took, and whether it still looks as the
    /// style says.
    pub fn object_style_of(&self, object: &Object) -> Option<(&ObjectStyle, bool)> {
        let style = self.object_styles.get(object.object_style?)?;
        Some((style, style.matches(&object.style)))
    }
}

impl Edit {
    /// Sets every object style at once.
    pub fn set_object_styles(&mut self, styles: ObjectStyles) -> Result<(), ModelError> {
        if *self.document().object_styles() == styles {
            return Ok(());
        }
        self.apply(Op::SetObjectStyles { styles }).map(drop)
    }

    /// Changes the object styles.
    fn change_object_styles(
        &mut self,
        change: impl FnOnce(&mut ObjectStyles) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        let mut styles = self.document().object_styles().clone();
        change(&mut styles)?;
        self.set_object_styles(styles)
    }

    /// Gives objects the look of a style, which they remember. Objects of
    /// another kind than the style's, or locked, stay as they are.
    pub fn apply_object_style(&mut self, ids: &[NodeId], style: StyleId) -> Result<(), ModelError> {
        let style = self
            .document()
            .object_styles()
            .get(style)
            .cloned()
            .ok_or(ModelError::UnknownStyle(style))?;
        for id in ids {
            let Some(mut object) = self.document().objects().get(*id).cloned() else {
                continue;
            };
            if object.locked || ObjectStyleKind::of(&object.kind) != Some(style.kind) {
                continue;
            }
            apply_object_style(&mut object, &style);
            self.replace_object(object)?;
        }
        Ok(())
    }

    /// Makes a new style of a kind with a look, named from `name`, and
    /// gives back its identifier.
    pub fn add_object_style(
        &mut self,
        kind: ObjectStyleKind,
        name: &str,
        graphic: GraphicStyle,
    ) -> Result<StyleId, ModelError> {
        let id = self.new_id();
        let name = self.document().object_styles().unique_name(kind, name);
        self.change_object_styles(|styles| {
            styles.insert(ObjectStyle {
                id,
                name: name.into(),
                kind,
                graphic,
            });
            Ok(())
        })?;
        Ok(id)
    }

    /// Gives a style a new look, as Update Style takes it from the
    /// selection. The objects that took the style and still looked as it
    /// did take the new look; those changed since keep theirs.
    pub fn redefine_object_style(
        &mut self,
        id: StyleId,
        graphic: GraphicStyle,
    ) -> Result<(), ModelError> {
        let old = self
            .document()
            .object_styles()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownStyle(id))?;
        let new = ObjectStyle {
            graphic,
            ..old.clone()
        };
        let followers: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| o.object_style == Some(id) && old.matches(&o.style))
            .cloned()
            .collect();
        self.change_object_styles(|styles| {
            styles.insert(new.clone());
            Ok(())
        })?;
        for mut object in followers {
            object.style = new.applied_to(&object.style);
            self.replace_object(object)?;
        }
        Ok(())
    }

    /// Renames a style. Two styles of a kind cannot have one name.
    pub fn rename_object_style(&mut self, id: StyleId, name: &str) -> Result<(), ModelError> {
        let name = name.trim();
        self.change_object_styles(|styles| {
            let mut style = styles
                .get(id)
                .cloned()
                .ok_or(ModelError::UnknownStyle(id))?;
            if name.is_empty() {
                return Err(ModelError::DuplicateStyleName(name.to_owned()));
            }
            if styles.by_name(style.kind, name).is_some_and(|s| s.id != id) {
                return Err(ModelError::DuplicateStyleName(name.to_owned()));
            }
            style.name = name.into();
            styles.insert(style);
            Ok(())
        })
    }

    /// Makes a style the one that new objects of its kind take.
    pub fn set_default_object_style(&mut self, id: StyleId) -> Result<(), ModelError> {
        self.change_object_styles(|styles| {
            if styles.get(id).is_none() {
                return Err(ModelError::UnknownStyle(id));
            }
            styles.set_default(id);
            Ok(())
        })
    }

    /// Makes a text box the appearance of the new text boxes of the
    /// document, as Set as Default Text Box Appearance of Pages 15.4
    /// (PXV-10): its look becomes the default text box style, its own
    /// style when it still looks as it or a new one named `name`, and its
    /// inset, the place of its text, its wrap and the formatting of the
    /// start of its text are kept for the text of new boxes.
    pub fn set_default_text_box(&mut self, id: NodeId, name: &str) -> Result<(), ModelError> {
        let object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        let ObjectKind::TextBox(text) = &object.kind else {
            return Ok(());
        };
        let style = match self
            .document()
            .object_style_of(&object)
            .filter(|(_, same)| *same)
        {
            Some((style, _)) => style.id,
            None => {
                let style =
                    self.add_object_style(ObjectStyleKind::TextBox, name, object.style.clone())?;
                self.apply_object_style(&[id], style)?;
                style
            }
        };
        let first = text.paragraphs.first();
        let character = first
            .and_then(|p| p.runs().next())
            .map(|run| CharAttrs {
                field: None,
                link: None,
                ..run.attrs.clone()
            })
            .unwrap_or_default();
        let default = TextBoxDefault {
            inset: text.inset,
            vertical: text.vertical,
            wrap: object.wrap,
            paragraph_style: first.map(crate::Paragraph::style),
            paragraph: first.map(|p| p.attrs().clone()).unwrap_or_default(),
            character,
        };
        self.change_object_styles(|styles| {
            styles.set_default(style);
            styles.set_text_box_default(default);
            Ok(())
        })
    }

    /// Deletes a style. The objects that took it keep their look and
    /// forget it. The last style of a kind cannot go, as new objects need
    /// a default.
    pub fn remove_object_style(&mut self, id: StyleId) -> Result<(), ModelError> {
        let styles = self.document().object_styles();
        let style = styles.get(id).ok_or(ModelError::UnknownStyle(id))?;
        if styles.of_kind(style.kind).count() < 2 {
            return Err(ModelError::StyleInUse(id));
        }
        let users: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| o.object_style == Some(id))
            .cloned()
            .collect();
        for mut object in users {
            object.object_style = None;
            self.replace_object(object)?;
        }
        self.change_object_styles(|styles| {
            styles.remove(id);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Frame, Shape};

    fn document_with(kind: ObjectKind) -> (Document, NodeId) {
        let mut document = Document::new_word_processing();
        let id = document
            .transact("Insert", |edit| {
                let id = edit.new_id();
                let mut object = Object::new(id, kind, Frame::from_pt(0.0, 0.0, 100.0, 100.0));
                edit.document().give_default_style(&mut object);
                edit.add_object(object)?;
                Ok(id)
            })
            .unwrap()
            .0;
        (document, id)
    }

    #[test]
    fn mod_04_a_new_document_has_six_styles_of_each_kind() {
        let document = Document::new_word_processing();
        let styles = document.object_styles();
        for kind in ObjectStyleKind::ALL {
            assert_eq!(styles.of_kind(kind).count(), 6, "{kind:?}");
            assert_eq!(
                styles.default_for(kind).map(|s| s.kind),
                Some(kind),
                "{kind:?}"
            );
        }
        // The first shape is the look Desk always gave new shapes.
        let basic = styles.default_for(ObjectStyleKind::Shape).unwrap();
        assert_eq!(
            basic.graphic.fill,
            Some(Fill::Color(Rgba::rgb(0x6c, 0x9f, 0xd8)))
        );
    }

    #[test]
    fn pxv_10_a_text_box_sets_the_appearance_of_new_ones() {
        use crate::{FontWeight, Paragraph, Run, TextBox};
        let mut document = Document::new_word_processing();
        let body = document.styles().default_paragraph();
        let bold = CharAttrs {
            weight: Some(FontWeight::BOLD),
            ..CharAttrs::default()
        };
        let paragraph = Paragraph::from_runs(
            document.new_id(),
            body,
            ParaAttrs::default(),
            [Run::with_attrs("Note", None, bold.clone())],
        )
        .unwrap();
        let mut text = TextBox::new(std::sync::Arc::from([paragraph]), Length::from_points(9));
        text.vertical = VerticalAlignment::Middle;
        let (mut document, id) = {
            let id = document.new_id();
            let mut object = Object::new(
                id,
                ObjectKind::TextBox(text),
                Frame::from_pt(0.0, 0.0, 100.0, 100.0),
            );
            document.give_default_style(&mut object);
            object.style.opacity = 50;
            object.wrap = Wrap::AboveAndBelow;
            document
                .transact("Insert", |edit| edit.add_object(object))
                .unwrap();
            (document, id)
        };
        document
            .transact("Set as Default Text Box Appearance", |edit| {
                edit.set_default_text_box(id, "Default")
            })
            .unwrap();
        let styles = document.object_styles();
        let style = styles.default_for(ObjectStyleKind::TextBox).unwrap();
        assert_eq!(style.graphic.opacity, 50);
        let default = styles.text_box_default();
        assert_eq!(default.inset, Length::from_points(9));
        assert_eq!(default.vertical, VerticalAlignment::Middle);
        assert_eq!(default.wrap, Wrap::AboveAndBelow);
        assert_eq!(default.paragraph_style, Some(body));
        assert_eq!(default.character, bold);
        assert_eq!(
            TextBoxDefault::default().inset,
            Length::from_points(4),
            "the default of Desk"
        );
    }

    #[test]
    fn mod_04_a_new_object_takes_the_default_and_remembers_it() {
        let (document, id) = document_with(Shape::Rectangle.into());
        let object = document.objects().get(id).unwrap();
        let (style, same) = document.object_style_of(object).unwrap();
        assert_eq!(style.name.as_ref(), "Basic");
        assert!(same);
    }

    #[test]
    fn mod_04_redefining_a_style_changes_the_objects_that_still_follow_it() {
        let (mut document, first) = document_with(Shape::Rectangle.into());
        let second = document
            .transact("Copy", |edit| {
                let mut copy = edit.document().objects().get(first).unwrap().clone();
                copy.id = edit.new_id();
                let id = copy.id;
                edit.add_object(copy)?;
                Ok(id)
            })
            .unwrap()
            .0;
        let style = document.objects().get(first).unwrap().object_style.unwrap();
        // The second is changed by hand: it no longer follows the style.
        document
            .transact("Opacity", |edit| {
                let mut object = edit.document().objects().get(second).unwrap().clone();
                object.style.opacity = 50;
                edit.replace_object(object)
            })
            .unwrap();
        let red = GraphicStyle {
            fill: Some(Fill::Color(Rgba::rgb(255, 0, 0))),
            ..GraphicStyle::default()
        };
        document
            .transact("Update Style", |edit| {
                edit.redefine_object_style(style, red.clone())
            })
            .unwrap();
        assert_eq!(document.objects().get(first).unwrap().style, red);
        assert_eq!(document.objects().get(second).unwrap().style.opacity, 50);
        assert_eq!(document.object_styles().get(style).unwrap().graphic, red);
    }

    #[test]
    fn vmi_02_applying_a_style_keeps_the_ends_of_a_line() {
        let (mut document, id) = document_with(Shape::Line { rising: false }.into());
        document
            .transact("Arrow", |edit| {
                let mut object = edit.document().objects().get(id).unwrap().clone();
                object.style.stroke.as_mut().unwrap().end = crate::LineEnd::Arrow;
                edit.replace_object(object)
            })
            .unwrap();
        let thick = document
            .object_styles()
            .by_name(ObjectStyleKind::Line, "Thick")
            .unwrap()
            .id;
        document
            .transact("Style", |edit| edit.apply_object_style(&[id], thick))
            .unwrap();
        let object = document.objects().get(id).unwrap();
        let stroke = object.style.stroke.unwrap();
        assert_eq!(stroke.width, Length::from_points(3));
        assert_eq!(stroke.end, crate::LineEnd::Arrow);
        assert!(document.object_style_of(object).unwrap().1);
    }

    #[test]
    fn mod_04_styles_are_renamed_made_default_and_removed() {
        let text = crate::TextBox::new(std::sync::Arc::from([]), Length::from_points(4));
        let (mut document, id) = document_with(ObjectKind::TextBox(text));
        let styles = document.object_styles().clone();
        let plain = styles.default_for(ObjectStyleKind::TextBox).unwrap().id;
        let photo = styles.by_name(ObjectStyleKind::TextBox, "Card").unwrap().id;
        document
            .transact("Default", |edit| {
                edit.set_default_object_style(photo)?;
                edit.rename_object_style(photo, "Snapshot")
            })
            .unwrap();
        let styles = document.object_styles();
        assert_eq!(
            styles.default_for(ObjectStyleKind::TextBox).unwrap().id,
            photo
        );
        assert_eq!(styles.get(photo).unwrap().name.as_ref(), "Snapshot");
        let taken = document.transact("Rename", |edit| edit.rename_object_style(photo, "Gray"));
        assert!(matches!(taken, Err(ModelError::DuplicateStyleName(_))));
        // Removing the style of an object leaves its look alone.
        let look = document.objects().get(id).unwrap().style.clone();
        document
            .transact("Delete", |edit| edit.remove_object_style(plain))
            .unwrap();
        let object = document.objects().get(id).unwrap();
        assert_eq!(object.object_style, None);
        assert_eq!(object.style, look);
        assert_eq!(
            document
                .object_styles()
                .of_kind(ObjectStyleKind::TextBox)
                .count(),
            5
        );
        assert!(document.object_styles().get(plain).is_none());
    }

    #[test]
    fn mod_04_names_of_new_styles_are_unique() {
        let mut document = Document::new_word_processing();
        let (a, b) = document
            .transact("New", |edit| {
                let a =
                    edit.add_object_style(ObjectStyleKind::Shape, "Blue", GraphicStyle::default())?;
                let b =
                    edit.add_object_style(ObjectStyleKind::Shape, "Mine", GraphicStyle::default())?;
                Ok((a, b))
            })
            .unwrap()
            .0;
        let styles = document.object_styles();
        assert_eq!(styles.get(a).unwrap().name.as_ref(), "Blue 2");
        assert_eq!(styles.get(b).unwrap().name.as_ref(), "Mine");
    }
}
