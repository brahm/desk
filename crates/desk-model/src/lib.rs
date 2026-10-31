// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Document model shared by Desk Writer, Desk Tables and Desk Slides.
//!
//! The model holds the content and the styles of a document, the
//! transactions that change it, undo and redo, and snapshots. Text, sheets,
//! slides, master slides and floating objects are parts of one model, so an
//! object copied from one app to another stays the same object.
