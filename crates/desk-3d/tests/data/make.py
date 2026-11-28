# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: MPL-2.0

"""Makes the USDZ files of the tests of desk-3d with OpenUSD (usd-core),
so that the reader is checked against files of the library that defines
the format. Run it from this folder: python3 make.py"""

import os
import shutil
import struct
import tempfile
import zlib

from pxr import Gf, Sdf, Usd, UsdGeom, UsdShade, UsdUtils, Vt


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


def cube(stage, path):
    """A cube of side 2 around its center, with its six faces as quads."""
    mesh = UsdGeom.Mesh.Define(stage, path)
    mesh.CreatePointsAttr([(-1, -1, -1), (1, -1, -1), (1, 1, -1), (-1, 1, -1),
                           (-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)])
    mesh.CreateFaceVertexCountsAttr([4] * 6)
    mesh.CreateFaceVertexIndicesAttr([0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4,
                                      2, 3, 7, 6, 1, 2, 6, 5, 0, 4, 7, 3])
    return mesh


def material(stage, path, color, texture=None):
    """A UsdPreviewSurface material of a color, or of a texture."""
    mat = UsdShade.Material.Define(stage, path)
    shader = UsdShade.Shader.Define(stage, path + "/Surface")
    shader.CreateIdAttr("UsdPreviewSurface")
    diffuse = shader.CreateInput("diffuseColor", Sdf.ValueTypeNames.Color3f)
    if texture:
        tex = UsdShade.Shader.Define(stage, path + "/Texture")
        tex.CreateIdAttr("UsdUVTexture")
        tex.CreateInput("file", Sdf.ValueTypeNames.Asset).Set(texture)
        rgb = tex.CreateOutput("rgb", Sdf.ValueTypeNames.Float3)
        diffuse.ConnectToSource(rgb)
    else:
        diffuse.Set(color)
    mat.CreateSurfaceOutput().ConnectToSource(shader.ConnectableAPI(), "surface")
    return mat


def package(name, build, extension, extra=()):
    """Writes a USDZ of the stage that `build` fills, from a layer of the
    extension given, with the extra files beside it."""
    folder = tempfile.mkdtemp()
    layer = os.path.join(folder, "scene." + extension)
    stage = Usd.Stage.CreateNew(layer)
    build(stage, folder)
    stage.GetRootLayer().Save()
    for file, data in extra:
        os.makedirs(os.path.dirname(os.path.join(folder, file)), exist_ok=True)
        with open(os.path.join(folder, file), "wb") as out:
            out.write(data)
    assert UsdUtils.CreateNewUsdzPackage(layer, name)
    shutil.rmtree(folder)


def colored_cube(stage, folder):
    """A red cube moved, turned and scaled, with a green corner."""
    UsdGeom.SetStageUpAxis(stage, UsdGeom.Tokens.y)
    root = UsdGeom.Xform.Define(stage, "/Root")
    stage.SetDefaultPrim(root.GetPrim())
    xform = UsdGeom.Xform.Define(stage, "/Root/Turned")
    xform.AddTranslateOp().Set(Gf.Vec3d(10, 0, 0))
    xform.AddRotateXYZOp().Set(Gf.Vec3f(0, 90, 0))
    xform.AddScaleOp().Set(Gf.Vec3f(2, 2, 2))
    mesh = cube(stage, "/Root/Turned/Cube")
    colors = [(1, 0, 0)] * 8
    colors[6] = (0, 1, 0)
    primvar = mesh.CreateDisplayColorPrimvar(UsdGeom.Tokens.vertex)
    primvar.Set(colors)
    mat = material(stage, "/Root/Looks/Red", (0.8, 0.1, 0.1))
    UsdShade.MaterialBindingAPI.Apply(mesh.GetPrim()).Bind(mat)


def textured_quad(stage, folder):
    """A square of a texture of four colors, in a Z-up stage."""
    UsdGeom.SetStageUpAxis(stage, UsdGeom.Tokens.z)
    root = UsdGeom.Xform.Define(stage, "/Root")
    stage.SetDefaultPrim(root.GetPrim())
    mesh = UsdGeom.Mesh.Define(stage, "/Root/Quad")
    mesh.CreatePointsAttr([(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)])
    mesh.CreateFaceVertexCountsAttr([4])
    mesh.CreateFaceVertexIndicesAttr([0, 1, 2, 3])
    st = UsdGeom.PrimvarsAPI(mesh).CreatePrimvar(
        "st", Sdf.ValueTypeNames.TexCoord2fArray, UsdGeom.Tokens.faceVarying)
    st.Set([(0, 0), (1, 0), (1, 1), (0, 1)])
    st.SetIndices(Vt.IntArray([0, 1, 2, 3]))
    mat = material(stage, "/Root/Looks/Picture", None, "textures/four.png")
    UsdShade.MaterialBindingAPI.Apply(mesh.GetPrim()).Bind(mat)


def two_materials(stage, folder):
    """A cube whose top is blue and the rest yellow, by subsets of its
    faces, and a second cube from another layer by a reference."""
    root = UsdGeom.Xform.Define(stage, "/Root")
    stage.SetDefaultPrim(root.GetPrim())
    mesh = cube(stage, "/Root/Cube")
    blue = material(stage, "/Root/Looks/Blue", (0, 0, 1))
    yellow = material(stage, "/Root/Looks/Yellow", (1, 1, 0))
    binding = UsdShade.MaterialBindingAPI.Apply(mesh.GetPrim())
    top = binding.CreateMaterialBindSubset("Top", Vt.IntArray([3]))
    rest = binding.CreateMaterialBindSubset("Rest", Vt.IntArray([0, 1, 2, 4, 5]))
    UsdShade.MaterialBindingAPI.Apply(top.GetPrim()).Bind(blue)
    UsdShade.MaterialBindingAPI.Apply(rest.GetPrim()).Bind(yellow)
    other = Usd.Stage.CreateNew(os.path.join(folder, "part.usdc"))
    part = UsdGeom.Xform.Define(other, "/Part")
    other.SetDefaultPrim(part.GetPrim())
    cube(other, "/Part/Box")
    other.GetRootLayer().Save()
    placed = UsdGeom.Xform.Define(stage, "/Root/Placed")
    placed.AddTranslateOp().Set(Gf.Vec3d(0, 5, 0))
    placed.GetPrim().GetReferences().AddReference("./part.usdc")


def many(stage, folder):
    """A grid of 20 by 20 squares, large enough for the arrays to be
    compressed, its points whole numbers for the integer coding and its
    normals few for the table coding, turned by a quaternion and moved by
    samples in time; and a sphere of points that are not whole."""
    import math
    root = UsdGeom.Xform.Define(stage, "/Root")
    stage.SetDefaultPrim(root.GetPrim())
    xform = UsdGeom.Xform.Define(stage, "/Root/Moving")
    xform.AddTranslateOp().Set(Gf.Vec3d(0, 0, 100), 0)
    xform.GetPrim().GetAttribute("xformOp:translate").Set(Gf.Vec3d(0, 0, 200), 10)
    # A quarter turn about Z.
    xform.AddOrientOp().Set(Gf.Quatf(math.sqrt(0.5), 0, 0, math.sqrt(0.5)))
    grid = UsdGeom.Mesh.Define(stage, "/Root/Moving/Grid")
    grid.CreatePointsAttr([(x, y, 0) for y in range(21) for x in range(21)])
    grid.CreateFaceVertexCountsAttr([4] * 400)
    grid.CreateFaceVertexIndicesAttr([i for y in range(20) for x in range(20)
                                      for i in (y * 21 + x, y * 21 + x + 1,
                                                (y + 1) * 21 + x + 1, (y + 1) * 21 + x)])
    grid.CreateNormalsAttr([(0, 0, 1)] * 441)
    grid.SetNormalsInterpolation(UsdGeom.Tokens.vertex)
    sphere = UsdGeom.Mesh.Define(stage, "/Root/Sphere")
    points = []
    for i in range(9):
        a = math.pi * i / 8
        for j in range(16):
            b = 2 * math.pi * j / 16
            points.append((math.sin(a) * math.cos(b), math.cos(a), math.sin(a) * math.sin(b)))
    sphere.CreatePointsAttr(points)
    sphere.CreateFaceVertexCountsAttr([4] * (8 * 16))
    sphere.CreateFaceVertexIndicesAttr([k for i in range(8) for j in range(16)
                                        for k in (i * 16 + j, (i + 1) * 16 + j,
                                                  (i + 1) * 16 + (j + 1) % 16, i * 16 + (j + 1) % 16)])


four = png(2, 2, [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255])
for extension in ("usdc", "usda"):
    package(f"cube-{extension}.usdz", colored_cube, extension)
    package(f"two-materials-{extension}.usdz", two_materials, extension)
package("textured.usdz", textured_quad, "usdc", [("textures/four.png", four)])
package("many-usdc.usdz", many, "usdc")
