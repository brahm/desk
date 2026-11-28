# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: MPL-2.0

"""Makes the OBJ files of the tests of desk-3d, by the specification of
Wavefront OBJ and MTL, with no library besides Python's. Run it from this
folder: python3 make_obj.py"""

import os
import struct
import zlib


def png(width, height, pixels):
    """A PNG of RGBA pixels, by rows."""
    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))
    raw = b"".join(b"\0" + bytes(pixels[y * width * 4:(y + 1) * width * 4])
                   for y in range(height))
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


BOX = """# A box of side 2 around the origin, made by make_obj.py.
mtllib box materials.mtl
o Box
v -1 -1 -1
v 1 -1 -1
v 1 1 -1
v -1 1 -1
v -1 -1 1
v 1 -1 1
v 1 1 1
v -1 1 1
vt 0 0
vt 1 0
vt 1 1
vt 0 1
vn 0 0 1
vn 0 0 -1
vn 1 0 0
vn -1 0 0
vn 0 1 0
vn 0 -1 0
g sides
usemtl Red
f 2//3 3//3 7//3 6//3
f 1//4 5//4 8//4 4//4
f 1//6 2//6 6//6 5//6
f 1//2 4//2 3//2 2//2
usemtl Wood
# The top, by numbers back from the last.
f -1/-1/-2 -2/-2/-2 3/2/5 \\
  4/1/5
usemtl Glass
f 5/1/1 6/2/1 7/3/1 8/4/1
"""

MATERIALS = """# The materials of box.obj.
newmtl Red
Kd 0.8 0.1 0.1
Ks 0.5 0.5 0.5
illum 2

newmtl Wood
Kd 1 1 1
map_Kd -s 1 1 1 -bm 1 pictures/wood.png
# A picture outside the folder is never read.
map_Ks ../secret.png

newmtl Glass
Kd 0 0 1
Tr 0.5
"""

COLORS = """# A triangle with the colors of its corners, and no material.
v 0 0 0 1 0 0
v 1 0 0 0 1 0
v 0 1 0 0 0 1
f 1 2 3
"""


def main():
    os.makedirs("obj/pictures", exist_ok=True)
    with open("obj/box.obj", "w") as f:
        f.write(BOX)
    with open("obj/box materials.mtl", "w") as f:
        f.write(MATERIALS)
    with open("obj/colors.obj", "w") as f:
        f.write(COLORS)
    with open("obj/pictures/wood.png", "wb") as f:
        # Red and green above blue and white.
        f.write(png(2, 2, [255, 0, 0, 255, 0, 255, 0, 255,
                           0, 0, 255, 255, 255, 255, 255, 255]))


if __name__ == "__main__":
    main()
