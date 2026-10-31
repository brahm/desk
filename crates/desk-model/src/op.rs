// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The operations that change a document.

use std::ops::Range;

use crate::document::{Metadata, PageSetup};
use crate::format::ParaAttrs;
use crate::id::NodeId;
use crate::style::{Style, StyleId};
use crate::text::{Paragraph, Run};

/// One small change to a document.
///
/// Every operation has an inverse, which [`Document::apply`] returns, and
/// applying the inverse brings the document back to what it was. Larger
/// edits, such as splitting a paragraph, are sequences of operations in a
/// [`Transaction`].
///
/// Operations on objects and table cells join this list in later phases,
/// so that undo and tracked changes cover them in the same way.
///
/// [`Document::apply`]: crate::Document::apply
/// [`Transaction`]: crate::Transaction
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
    /// Replaces a range of the text of a paragraph with runs. It inserts
    /// text when the range is empty, deletes text when there are no runs,
    /// and changes the formatting when the runs have the same text.
    ReplaceText {
        /// The paragraph.
        paragraph: NodeId,
        /// The range, in bytes of the paragraph text.
        range: Range<usize>,
        /// The runs that take the place of the range.
        runs: Vec<Run>,
    },
    /// Inserts a paragraph in the body.
    InsertParagraph {
        /// The index that the paragraph takes.
        index: usize,
        /// The paragraph.
        paragraph: Paragraph,
    },
    /// Removes a paragraph from the body.
    RemoveParagraph {
        /// The paragraph.
        paragraph: NodeId,
    },
    /// Sets the style of a paragraph and the attributes set on it.
    SetParagraphFormat {
        /// The paragraph.
        paragraph: NodeId,
        /// The paragraph style.
        style: StyleId,
        /// The attributes set on the paragraph itself.
        attrs: ParaAttrs,
    },
    /// Adds a style, or replaces the style with the same identifier.
    PutStyle {
        /// The style.
        style: Style,
    },
    /// Removes a style that nothing uses.
    RemoveStyle {
        /// The style.
        style: StyleId,
    },
    /// Sets the size and margins of the pages.
    SetPageSetup {
        /// The page setup.
        page: PageSetup,
    },
    /// Sets the information about the document.
    SetMetadata {
        /// The metadata.
        metadata: Metadata,
    },
}
