// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

// Makes tess.prc for the tests of desk-3d with Asymptote, which writes
// PRC as the 3D of PDF: asy -noV -f prc tess.asy
// A red cube, tessellated: the triangles of one face of a 3D tessellation.

import three;
settings.prc=true;
size(4cm);
draw(unitcube, red, render(tessellate=true));
