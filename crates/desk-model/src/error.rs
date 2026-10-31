// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors of the model.

use std::fmt;
use std::ops::Range;

use crate::id::NodeId;
use crate::style::{StyleFamily, StyleId};

/// Why a change to a document was refused. A refused transaction leaves
/// the document as it was.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelError {
    /// No paragraph has this identifier.
    UnknownParagraph(NodeId),
    /// No style has this identifier.
    UnknownStyle(StyleId),
    /// The style is not of the family the change needs.
    WrongStyleFamily {
        /// The style.
        style: StyleId,
        /// The family the change needs.
        expected: StyleFamily,
    },
    /// The change would make a style inherit from itself.
    StyleCycle(StyleId),
    /// The style cannot be removed while text or other styles use it.
    StyleInUse(StyleId),
    /// The default paragraph style cannot be removed.
    DefaultStyle,
    /// Another node already has this identifier.
    DuplicateId(NodeId),
    /// The range is outside the paragraph or splits a character.
    InvalidRange {
        /// The paragraph.
        paragraph: NodeId,
        /// The range, in bytes of the paragraph text.
        range: Range<usize>,
    },
    /// Text in a run cannot hold a paragraph separator.
    InvalidText,
    /// The index is past the end.
    IndexOutOfBounds {
        /// The index.
        index: usize,
        /// The number of items.
        len: usize,
    },
    /// A text flow keeps at least one paragraph.
    LastParagraph,
    /// The two positions are not in order.
    BackwardRange,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParagraph(id) => write!(f, "no paragraph has the identifier {id}"),
            Self::UnknownStyle(id) => write!(f, "no style has the identifier {id}"),
            Self::WrongStyleFamily { style, expected } => {
                write!(f, "style {style} is not a {expected:?} style")
            }
            Self::StyleCycle(id) => write!(f, "style {id} would inherit from itself"),
            Self::StyleInUse(id) => write!(f, "style {id} is in use"),
            Self::DefaultStyle => write!(f, "the default paragraph style cannot be removed"),
            Self::DuplicateId(id) => write!(f, "the identifier {id} is already in use"),
            Self::InvalidRange { paragraph, range } => write!(
                f,
                "the range {}..{} is not valid in paragraph {paragraph}",
                range.start, range.end
            ),
            Self::InvalidText => write!(f, "text in a run cannot hold a paragraph separator"),
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {index} is out of bounds for length {len}")
            }
            Self::LastParagraph => write!(f, "a text flow keeps at least one paragraph"),
            Self::BackwardRange => write!(f, "the start of the range is after its end"),
        }
    }
}

impl std::error::Error for ModelError {}
