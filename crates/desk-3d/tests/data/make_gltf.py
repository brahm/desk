# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: MPL-2.0

"""Makes the glTF files of the tests of desk-3d, by the specification of
glTF 2.0, with no library besides Python's. Run it from this folder:
python3 make_gltf.py"""

import base64
import json
import os
import struct
import zlib

FLOAT, USHORT, UBYTE = 5126, 5123, 5121


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


class Builder:
    """The buffer, views and accessors of a glTF being made."""

    def __init__(self):
        self.data = bytearray()
        self.views = []
        self.accessors = []

    def view(self, data, stride=None):
        while len(self.data) % 4:
            self.data.append(0)
        view = {"buffer": 0, "byteOffset": len(self.data), "byteLength": len(data)}
        if stride:
            view["byteStride"] = stride
        self.data += data
        self.views.append(view)
        return len(self.views) - 1

    def accessor(self, kind, component, values, normalized=False, bounds=False):
        width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[kind]
        code = {FLOAT: "f", USHORT: "H", UBYTE: "B"}[component]
        flat = [v for value in values for v in (value if width > 1 else [value])]
        accessor = {
            "bufferView": self.view(struct.pack("<%d%s" % (len(flat), code), *flat)),
            "componentType": component,
            "count": len(values),
            "type": kind,
        }
        if normalized:
            accessor["normalized"] = True
        if bounds:
            columns = list(zip(*values)) if width > 1 else [values]
            accessor["min"] = [min(c) for c in columns]
            accessor["max"] = [max(c) for c in columns]
        self.accessors.append(accessor)
        return len(self.accessors) - 1


def box(builder):
    """A box of side 2 around its center: 24 corners, each face its own
    normals, 36 indices as unsigned shorts."""
    positions, normals, indices = [], [], []
    for axis in range(3):
        for sign in (-1, 1):
            n = [0, 0, 0]
            n[axis] = sign
            u, v = [(a) for a in range(3) if a != axis]
            start = len(positions)
            for du, dv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                p = [0, 0, 0]
                p[axis], p[u], p[v] = sign, du, dv
                positions.append(p)
                normals.append(n)
            quad = [start, start + 1, start + 2, start, start + 2, start + 3]
            if (sign > 0) != (axis == 1):
                quad = [start, start + 2, start + 1, start, start + 3, start + 2]
            indices += quad
    return {
        "attributes": {
            "POSITION": builder.accessor("VEC3", FLOAT, positions, bounds=True),
            "NORMAL": builder.accessor("VEC3", FLOAT, normals),
        },
        "indices": builder.accessor("SCALAR", USHORT, indices),
    }


def glb(document, data):
    text = json.dumps(document, separators=(",", ":")).encode()
    text += b" " * (-len(text) % 4)
    data = bytes(data) + b"\0" * (-len(data) % 4)
    length = 12 + 8 + len(text) + 8 + len(data)
    return (b"glTF" + struct.pack("<II", 2, length)
            + struct.pack("<II", len(text), 0x4E4F534A) + text
            + struct.pack("<II", len(data), 0x004E4942) + data)


def animated():
    """A red box moved 10 in X and scaled by 2, which goes up 5 in two
    seconds, and a blue box hung under it, 4 above, that turns a quarter
    about Y at one second (a step). A green box beside them does not
    move."""
    b = Builder()
    mesh = box(b)
    times = b.accessor("SCALAR", FLOAT, [0.0, 2.0], bounds=True)
    moves = b.accessor("VEC3", FLOAT, [[10, 0, 0], [10, 5, 0]])
    turn_times = b.accessor("SCALAR", FLOAT, [0.0, 1.0], bounds=True)
    h = 0.5 ** 0.5
    turns = b.accessor("VEC4", FLOAT, [[0, 0, 0, 1], [0, h, 0, h]])
    document = {
        "asset": {"version": "2.0", "generator": "make_gltf.py"},
        "scene": 0,
        "scenes": [{"nodes": [0, 2]}],
        "nodes": [
            {"mesh": 0, "translation": [10, 0, 0], "scale": [2, 2, 2], "children": [1]},
            {"mesh": 1, "translation": [0, 4, 0]},
            {"mesh": 2, "matrix": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, -10, 0, 0, 1]},
        ],
        "meshes": [
            {"primitives": [dict(mesh, material=0)]},
            {"primitives": [dict(mesh, material=1)]},
            {"primitives": [dict(mesh, material=2)]},
        ],
        "materials": [
            {"pbrMetallicRoughness": {"baseColorFactor": [0.8, 0.1, 0.1, 1.0]}},
            {"pbrMetallicRoughness": {"baseColorFactor": [0.0, 0.0, 1.0, 1.0]}},
            {"pbrMetallicRoughness": {"baseColorFactor": [0.0, 1.0, 0.0, 0.5]},
             "alphaMode": "BLEND"},
        ],
        "animations": [{
            "channels": [
                {"sampler": 0, "target": {"node": 0, "path": "translation"}},
                {"sampler": 1, "target": {"node": 1, "path": "rotation"}},
            ],
            "samplers": [
                {"input": times, "output": moves},
                {"input": turn_times, "output": turns, "interpolation": "STEP"},
            ],
        }],
        "buffers": [{"byteLength": len(b.data) + (-len(b.data) % 4)}],
        "bufferViews": b.views,
        "accessors": b.accessors,
    }
    return glb(document, b.data)


def grow():
    """A box that rests at a scale of 0 and grows to its size in a second,
    with a material masked at 0.25."""
    b = Builder()
    mesh = box(b)
    times = b.accessor("SCALAR", FLOAT, [0.0, 1.0], bounds=True)
    sizes = b.accessor("VEC3", FLOAT, [[0, 0, 0], [1, 1, 1]])
    document = {
        "asset": {"version": "2.0"},
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "scale": [0, 0, 0]}],
        "meshes": [{"primitives": [dict(mesh, material=0)]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 0.5]},
                       "alphaMode": "MASK", "alphaCutoff": 0.25}],
        "animations": [{
            "channels": [{"sampler": 0, "target": {"node": 0, "path": "scale"}}],
            "samplers": [{"input": times, "output": sizes}],
        }],
        "buffers": [{"byteLength": len(b.data) + (-len(b.data) % 4)}],
        "bufferViews": b.views,
        "accessors": b.accessors,
    }
    return glb(document, b.data)


def picture():
    """A picture of 2 by 2: red, green above blue, white."""
    return png(2, 2, [255, 0, 0, 255, 0, 255, 0, 255,
                      0, 0, 255, 255, 255, 255, 255, 255])


def textured(external):
    """A quad of 1 by 1 at the origin, facing +Z, as a strip of four
    corners with a texture whose top left is red; the corners are white,
    by colors of unsigned bytes. Inside the file as data URIs, or, when
    `external`, in a .bin and a .png beside it."""
    b = Builder()
    strip = {
        "attributes": {
            "POSITION": b.accessor("VEC3", FLOAT, [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0]],
                                   bounds=True),
            "TEXCOORD_0": b.accessor("VEC2", FLOAT, [[0, 1], [1, 1], [0, 0], [1, 0]]),
            "COLOR_0": b.accessor("VEC4", UBYTE, [[255, 255, 255, 255]] * 4, normalized=True),
        },
        "mode": 5,
        "material": 0,
    }
    pic = picture()
    if external:
        buffer = {"byteLength": len(b.data), "uri": "quad%20data.bin"}
        image = {"uri": "pictures/quad.png"}
    else:
        buffer = {"byteLength": len(b.data),
                  "uri": "data:application/octet-stream;base64,"
                  + base64.b64encode(bytes(b.data)).decode()}
        image = {"uri": "data:image/png;base64," + base64.b64encode(pic).decode()}
    document = {
        "asset": {"version": "2.0"},
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [strip]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
        "textures": [{"source": 0}],
        "images": [image],
        "buffers": [buffer],
        "bufferViews": b.views,
        "accessors": b.accessors,
    }
    return json.dumps(document, indent=1).encode(), bytes(b.data), pic


def main():
    with open("animated.glb", "wb") as f:
        f.write(animated())
    with open("grow.glb", "wb") as f:
        f.write(grow())
    text, _, _ = textured(False)
    with open("textured.gltf", "wb") as f:
        f.write(text)
    text, data, pic = textured(True)
    os.makedirs("external/pictures", exist_ok=True)
    with open("external/quad.gltf", "wb") as f:
        f.write(text)
    with open("external/quad data.bin", "wb") as f:
        f.write(data)
    with open("external/pictures/quad.png", "wb") as f:
        f.write(pic)


if __name__ == "__main__":
    main()
