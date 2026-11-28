// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

// Makes brep.prc for the tests of desk-3d with Asymptote, which writes
// PRC as the 3D of PDF: asy -noV -f prc brep.asy
// A red cube of exact surfaces with no tessellation, which is not read.

import three;
size(4cm);
draw(unitcube, red);
