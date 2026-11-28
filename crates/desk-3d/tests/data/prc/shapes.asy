// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

// Makes shapes.prc for the tests of desk-3d with Asymptote, which writes
// PRC as the 3D of PDF: asy -noV -f prc shapes.asy
// Three cubes, each a product occurrence placed by its own matrix or an
// item placed by a coordinate system: red, blue half seen through, and
// green.

import three;
settings.prc=true;
size(4cm);
currentlight=nolight;
begingroup3("red");
draw(unitcube, red, render(tessellate=true));
endgroup3();
begingroup3("blue");
draw(shift(3,0,0)*unitcube, rgb(0,0,1)+opacity(0.5), render(tessellate=true));
endgroup3();
draw(shift(0,3,0)*unitcube, green, render(tessellate=true));
