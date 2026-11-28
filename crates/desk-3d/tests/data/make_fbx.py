# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: MPL-2.0

"""Makes the FBX files of the tests of desk-3d, binary and text, by the
layout of FBX 7 as it is documented in the open (the nodes, their values
and the connections of objects), with no library besides Python's. Run
it from this folder: python3 make_fbx.py"""

import os
import struct
import zlib

from make_obj import png

TICKS = 46186158000

# A box of side 2 around its center, by its eight corners and six faces,
# the last corner of each face written as its complement.
CORNERS = [-1, -1, -1, 1, -1, -1, 1, 1, -1, -1, 1, -1,
           -1, -1, 1, 1, -1, 1, 1, 1, 1, -1, 1, 1]
FACES = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4],
         [2, 3, 7, 6], [1, 2, 6, 5], [0, 4, 7, 3]]
NORMALS = [(0, 0, -1), (0, 0, 1), (0, -1, 0), (0, 1, 0), (1, 0, 0), (-1, 0, 0)]


def index():
    out = []
    for face in FACES:
        out += face[:-1] + [~face[-1]]
    return out


def p70(*entries):
    """A Properties70 node of (name, type, values)."""
    return ("Properties70", [], [("P", [name, kind, "", "A"] + list(values), [])
                                 for name, kind, values in entries])


def document(picture):
    """The tree of a model: a parent box, red but for its top, which has
    a picture inside the file, moved 6 in X in a second; and a child box
    above it, turned a quarter about Y, that goes with it."""
    normals = [v for n in NORMALS for _ in range(4) for v in n]
    uv = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]
    geometry = ("Geometry", [100, "Box\0\1Geometry", "Mesh"], [
        ("Vertices", [("d", [float(v) for v in CORNERS])], []),
        ("PolygonVertexIndex", [("i", index())], []),
        ("LayerElementNormal", [0], [
            ("MappingInformationType", ["ByPolygonVertex"], []),
            ("ReferenceInformationType", ["Direct"], []),
            ("Normals", [("d", [float(v) for v in normals])], []),
        ]),
        ("LayerElementUV", [0], [
            ("MappingInformationType", ["ByPolygonVertex"], []),
            ("ReferenceInformationType", ["IndexToDirect"], []),
            ("UV", [("d", uv)], []),
            ("UVIndex", [("i", [0, 1, 2, 3] * 6)], []),
        ]),
        ("LayerElementMaterial", [0], [
            ("MappingInformationType", ["ByPolygon"], []),
            ("ReferenceInformationType", ["IndexToDirect"], []),
            ("Materials", [("i", [0, 0, 0, 1, 0, 0])], []),
        ]),
    ])
    objects = [
        geometry,
        ("Model", [200, "Parent\0\1Model", "Mesh"], [
            p70(("Lcl Translation", "Lcl Translation", [0.0, 0.0, 0.0])),
        ]),
        ("Model", [201, "Child\0\1Model", "Mesh"], [
            p70(("Lcl Translation", "Lcl Translation", [0.0, 4.0, 0.0]),
                ("Lcl Rotation", "Lcl Rotation", [0.0, 90.0, 0.0])),
        ]),
        ("Material", [300, "Red\0\1Material", ""], [
            p70(("DiffuseColor", "Color", [0.8, 0.1, 0.1])),
        ]),
        ("Material", [301, "Top\0\1Material", ""], [
            p70(("DiffuseColor", "Color", [1.0, 1.0, 1.0])),
        ]),
        ("Texture", [400, "Top\0\1Texture", ""], [
            ("RelativeFilename", ["missing.png"], []),
        ]),
        ("Video", [500, "Top\0\1Video", "Clip"], [
            ("Content", [("R", picture)], []),
        ]),
        ("AnimationStack", [600, "Take\0\1AnimStack", ""], []),
        ("AnimationLayer", [601, "Layer\0\1AnimLayer", ""], []),
        ("AnimationCurveNode", [602, "T\0\1AnimCurveNode", ""], []),
        ("AnimationCurve", [603, "\0\1AnimCurve", ""], [
            ("KeyTime", [("l", [0, TICKS])], []),
            ("KeyValueFloat", [("f", [0.0, 6.0])], []),
        ]),
    ]
    connections = [
        ("OO", 200, 0), ("OO", 201, 200), ("OO", 100, 200), ("OO", 100, 201),
        ("OO", 300, 200), ("OO", 301, 200), ("OO", 300, 201), ("OO", 301, 201),
        ("OP", 400, 301, "DiffuseColor"), ("OO", 500, 400),
        ("OO", 601, 600), ("OO", 602, 601),
        ("OP", 602, 200, "Lcl Translation"), ("OP", 603, 602, "d|X"),
    ]
    return [
        ("FBXHeaderExtension", [], [("FBXVersion", [7400], [])]),
        ("GlobalSettings", [], []),
        ("Objects", [], objects),
        ("Connections", [], [("C", list(c), []) for c in connections]),
    ]


def value(v, compress):
    """A value of the binary form."""
    if isinstance(v, tuple):
        kind, data = v
        if kind == "R":
            return b"R" + struct.pack("<I", len(data)) + data
        code = {"d": "d", "f": "f", "i": "i", "l": "q"}[kind]
        raw = struct.pack("<%d%s" % (len(data), code), *data)
        if compress:
            packed = zlib.compress(raw)
            return kind.encode() + struct.pack("<III", len(data), 1, len(packed)) + packed
        return kind.encode() + struct.pack("<III", len(data), 0, len(raw)) + raw
    if isinstance(v, str):
        b = v.encode()
        return b"S" + struct.pack("<I", len(b)) + b
    if isinstance(v, float):
        return b"D" + struct.pack("<d", v)
    if -2**31 <= v < 2**31:
        return b"I" + struct.pack("<i", v)
    return b"L" + struct.pack("<q", v)


def binary(nodes, version):
    wide = version >= 7500
    head = 25 if wide else 13

    def node(n, at):
        name, values, children = n
        props = b"".join(value(v, True) for v in values)
        body_at = at + head + len(name) + len(props)
        body = b""
        for c in children:
            body += node(c, body_at + len(body))
        if children:
            body += b"\0" * head
        end = body_at + len(body)
        fmt = "<QQQB" if wide else "<IIIB"
        return (struct.pack(fmt, end, len(values), len(props), len(name))
                + name.encode() + props + body)

    out = b"Kaydara FBX Binary  \0\x1a\0" + struct.pack("<I", version)
    for n in nodes:
        out += node(n, len(out))
    return out + b"\0" * head


def text(nodes):
    def show(v):
        if isinstance(v, tuple):
            kind, data = v
            return "*%d {\n\ta: %s\n}" % (len(data), ",".join(repr(x) for x in data))
        if isinstance(v, str):
            return '"%s"' % v.replace("\0\1", "::")
        return repr(v)

    def node(n, depth):
        name, values, children = n
        pad = "\t" * depth
        line = pad + name + ": " + ", ".join(show(v) for v in values)
        if children:
            line += " {\n" + "".join(node(c, depth + 1) for c in children) + pad + "}"
        return line + "\n"
    return ("; FBX 7.4.0 project file\n; made by make_fbx.py\n\n"
            + "".join(node(n, 0) for n in nodes)).encode()


def main():
    os.makedirs("fbx", exist_ok=True)
    picture = png(2, 2, [255, 0, 0, 255, 0, 255, 0, 255,
                         0, 0, 255, 255, 255, 255, 255, 255])
    nodes = document(picture)
    with open("fbx/box-7400.fbx", "wb") as f:
        f.write(binary(nodes, 7400))
    with open("fbx/box-7500.fbx", "wb") as f:
        f.write(binary(nodes, 7500))
    # The text form, Z up, its picture beside it.
    for n in nodes:
        if n[0] == "GlobalSettings":
            n[2].append(p70(("UpAxis", "int", [2]), ("UpAxisSign", "int", [1])))
        if n[0] == "Objects":
            n[2][:] = [o for o in n[2] if o[0] != "Video"]
    with open("fbx/box.fbx", "wb") as f:
        f.write(text(nodes))
    with open("fbx/missing.png", "wb") as f:
        f.write(picture)


if __name__ == "__main__":
    main()
