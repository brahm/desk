// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

// Makes triangles.prc for the tests of desk-3d with Asymptote, which writes
// PRC as the 3D of PDF: asy -noV -f prc triangles.asy
// Two triangles with a normal and a color at each corner.

import three;
settings.prc=true;
size(4cm);
triple[] v={(0,0,0),(1,0,0),(1,1,0),(0,1,0)};
int[][] vi={{0,1,2},{0,2,3}};
triple[] n={(0,0,1)};
int[][] ni={{0,0,0},{0,0,0}};
pen[] p={red,green,blue,white};
int[][] pi={{0,1,2},{0,2,3}};
draw(v,vi,n,ni,p,pi);
