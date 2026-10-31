// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable identifiers for the nodes of a document.

use std::fmt;
use std::num::NonZeroU64;

/// The stable identifier of a node: a paragraph, a style and, later, an
/// object, a table, a row or a cell.
///
/// An identifier never changes while the node exists, is never reused in
/// the same document, and is saved in the file (as `xml:id` in ODF), so
/// that comments, tracked changes, links and animations can point to the
/// node across edits, undo and reopening the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(NonZeroU64);

impl NodeId {
    /// Makes an identifier from its number, which cannot be zero.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// The number of the identifier.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n{}", self.0)
    }
}

/// Hands out new identifiers for one document.
///
/// The allocator only moves forward, also on undo, so an identifier that a
/// redo brings back is never given to another node in the meantime.
#[derive(Clone, Debug)]
pub(crate) struct IdAllocator {
    next: u64,
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl IdAllocator {
    /// Returns a new identifier.
    pub(crate) fn allocate(&mut self) -> NodeId {
        let id = NodeId::new(self.next).expect("identifiers start at 1");
        self.next += 1;
        id
    }

    /// Records an identifier that came from elsewhere, such as a file, so
    /// that it is never allocated again.
    pub(crate) fn observe(&mut self, id: NodeId) {
        self.next = self.next.max(id.get() + 1);
    }
}
