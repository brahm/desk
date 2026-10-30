// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Display list, effects and the Cairo back end.
//!
//! Everything the apps draw goes through one display list, which the Cairo
//! back end paints on screen, for printing and into PDF files. Shadows,
//! reflections and the other object effects are part of the display list.
