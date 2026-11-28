// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! A software renderer of scenes: the model turned as a [`View`] asks,
//! seen from the front without perspective, fitted to the picture, lit
//! from the upper left and drawn with a depth buffer, two samples a side
//! for each pixel to smooth its edges.
//!
//! Drawing on the CPU keeps the crate free of the GPU, so that a page
//! prints and exports the model as it shows it. The model is fitted by
//! the sphere around it, so that it keeps its size as it turns.

use crate::{Mesh, Scene, Texture, zip_with};

/// How the model is turned.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct View {
    /// The turn about the vertical axis, in degrees; a positive one
    /// brings the right side of the model to the front.
    pub yaw: f32,
    /// The turn about the horizontal axis, in degrees; a positive one
    /// brings the top of the model to the front.
    pub pitch: f32,
}

/// The longest side of a picture that [`render`] draws.
pub const MAX_SIDE: u32 = 8192;

/// The light, toward the viewer from the upper left.
const LIGHT: [f32; 3] = [-0.3, 0.5, 0.81];

/// Draws a scene turned as `view` says into a picture of `width` by
/// `height` pixels, as red, green, blue and alpha by rows from the top,
/// premultiplied; what the model does not cover is clear. None when a
/// side is 0 or longer than [`MAX_SIDE`].
pub fn render(scene: &Scene, view: View, width: u32, height: u32) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let samples = if u64::from(width) * u64::from(height) <= 4_000_000 {
        2
    } else {
        1
    };
    let (w, h) = (width as usize * samples, height as usize * samples);
    let mut target = Target {
        width: w,
        height: h,
        color: vec![[0.0; 4]; w * h],
        depth: vec![f32::NEG_INFINITY; w * h],
    };
    if let Some((lo, hi)) = scene.bounds() {
        let center = zip_with(lo, hi, |l, h| (l + h) / 2.0);
        let radius = zip_with(hi, lo, |h, l| (h - l) / 2.0)
            .iter()
            .map(|d| d * d)
            .sum::<f32>()
            .sqrt();
        let radius = if radius > 0.0 && radius.is_finite() {
            radius
        } else {
            1.0
        };
        let camera = Camera {
            turn: turn(view),
            center,
            scale: w.min(h) as f32 / 2.0 * 0.96 / radius,
            mid: [w as f32 / 2.0, h as f32 / 2.0],
        };
        // The opaque meshes first, then those that let the others show
        // through, over them.
        for translucent in [false, true] {
            for mesh in &scene.meshes {
                if (mesh.material.color[3] < 0.999) == translucent {
                    draw(&mut target, &camera, mesh, &scene.textures, translucent);
                }
            }
        }
    }
    Some(resolve(&target, width as usize, height as usize, samples))
}

/// The pixels drawn into, at the samples' resolution.
struct Target {
    width: usize,
    height: usize,
    /// Premultiplied colors from 0 to 1.
    color: Vec<[f32; 4]>,
    /// The depth of what is nearest, larger nearer.
    depth: Vec<f32>,
}

/// Where the model is seen from.
struct Camera {
    turn: [[f32; 3]; 3],
    center: [f32; 3],
    scale: f32,
    mid: [f32; 2],
}

impl Camera {
    /// A point in the picture: its x and y in samples, and its depth.
    fn project(&self, p: [f32; 3]) -> [f32; 3] {
        let v = apply(&self.turn, zip_with(p, self.center, |a, b| a - b));
        [
            self.mid[0] + v[0] * self.scale,
            self.mid[1] - v[1] * self.scale,
            v[2],
        ]
    }
}

/// The matrix of the turn of a view: yaw, then pitch.
fn turn(view: View) -> [[f32; 3]; 3] {
    let (sy, cy) = view.yaw.to_radians().sin_cos();
    let (sp, cp) = view.pitch.to_radians().sin_cos();
    // Yaw about Y takes +X toward the viewer; pitch about X takes +Y
    // toward the viewer.
    let yaw = [[cy, 0.0, -sy], [0.0, 1.0, 0.0], [sy, 0.0, cy]];
    let pitch = [[1.0, 0.0, 0.0], [0.0, cp, -sp], [0.0, sp, cp]];
    pitch.map(|row| {
        let mut out = [0.0; 3];
        for (p, yaw_row) in row.iter().zip(&yaw) {
            for (o, y) in out.iter_mut().zip(yaw_row) {
                *o += p * y;
            }
        }
        out
    })
}

fn apply(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

/// The texel of a texture at a coordinate, repeated past its edges, with
/// V up.
fn sample(texture: &Texture, uv: [f32; 2]) -> [f32; 4] {
    let (w, h) = (texture.width as usize, texture.height as usize);
    if w == 0 || h == 0 {
        return [1.0; 4];
    }
    let u = uv[0] - uv[0].floor();
    let v = 1.0 - (uv[1] - uv[1].floor());
    let x = ((u * w as f32) as usize).min(w - 1);
    let y = ((v * h as f32) as usize).min(h - 1);
    let at = (y * w + x) * 4;
    match texture.rgba.get(at..at + 4) {
        Some(&[r, g, b, a]) => [r, g, b, a].map(|v| f32::from(v) / 255.0),
        _ => [1.0; 4],
    }
}

/// Draws the triangles of a mesh.
fn draw(
    target: &mut Target,
    camera: &Camera,
    mesh: &Mesh,
    textures: &[Texture],
    translucent: bool,
) {
    let texture = mesh.material.texture.and_then(|t| textures.get(t));
    let light = {
        let n = (LIGHT[0] * LIGHT[0] + LIGHT[1] * LIGHT[1] + LIGHT[2] * LIGHT[2]).sqrt();
        LIGHT.map(|c| c / n)
    };
    for tri in &mesh.triangles {
        let idx = tri.map(|i| i as usize);
        let [Some(a), Some(b), Some(c)] =
            idx.map(|i| mesh.positions.get(i).map(|p| camera.project(*p)))
        else {
            continue;
        };
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if area == 0.0 || !area.is_finite() {
            continue;
        }
        let normals = idx.map(|i| mesh.normals.get(i).map(|n| apply(&camera.turn, *n)));
        let uvs = idx.map(|i| mesh.uvs.get(i).copied());
        let colors = idx.map(|i| mesh.colors.get(i).copied());
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil().max(0.0) as usize).min(target.width);
        let y1 = (a[1].max(b[1]).max(c[1]).ceil().max(0.0) as usize).min(target.height);
        // A flat normal for corners that have none, from the corners
        // turned as the normals are, before they are put on the screen.
        let flat = {
            let [Some(ta), Some(tb), Some(tc)] =
                idx.map(|i| mesh.positions.get(i).map(|p| apply(&camera.turn, *p)))
            else {
                continue;
            };
            let e1 = [tb[0] - ta[0], tb[1] - ta[1], tb[2] - ta[2]];
            let e2 = [tc[0] - ta[0], tc[1] - ta[1], tc[2] - ta[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2])
                .sqrt()
                .max(f32::MIN_POSITIVE);
            n.map(|v| v / l)
        };
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let w0 = ((b[0] - px) * (c[1] - py) - (b[1] - py) * (c[0] - px)) / area;
                let w1 = ((c[0] - px) * (a[1] - py) - (c[1] - py) * (a[0] - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let at = y * target.width + x;
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                if target.depth.get(at).is_none_or(|d| z <= *d) {
                    continue;
                }
                let weights = [w0, w1, w2];
                let mix3 = |v: [Option<[f32; 3]>; 3]| -> Option<[f32; 3]> {
                    let mut out = [0.0; 3];
                    for (w, v) in weights.iter().zip(v) {
                        out = zip_with(out, v?, |o, v| o + w * v);
                    }
                    Some(out)
                };
                let mut color = mesh.material.color;
                if let Some(c) = mix3(colors) {
                    color[..3].copy_from_slice(&c);
                }
                if let Some(t) = texture {
                    let uv = uvs
                        .iter()
                        .zip(&weights)
                        .try_fold([0.0f32; 2], |acc, (uv, w)| {
                            uv.map(|uv| [acc[0] + w * uv[0], acc[1] + w * uv[1]])
                        });
                    if let Some(uv) = uv {
                        let texel = sample(t, uv);
                        color = zip_with(color, texel, |c, t| c * t);
                    }
                }
                // A texture's clear texels cut the opaque meshes out.
                if !translucent && color[3] < 0.5 {
                    continue;
                }
                let n = mix3(normals).unwrap_or(flat);
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                let lit = if l > 0.0 {
                    ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / l).abs()
                } else {
                    1.0
                };
                let shade = 0.3 + 0.7 * lit;
                let alpha = if translucent { color[3] } else { 1.0 };
                let [r, g, b, _] = color;
                let rgb = [r, g, b].map(|c| (c * shade).clamp(0.0, 1.0) * alpha);
                if let (Some(pixel), Some(depth)) =
                    (target.color.get_mut(at), target.depth.get_mut(at))
                {
                    if translucent {
                        for (p, c) in pixel.iter_mut().zip(rgb) {
                            *p = c + *p * (1.0 - alpha);
                        }
                        pixel[3] = alpha + pixel[3] * (1.0 - alpha);
                    } else {
                        *pixel = [rgb[0], rgb[1], rgb[2], 1.0];
                        *depth = z;
                    }
                }
            }
        }
    }
}

/// The picture at its size: the samples of each pixel averaged.
fn resolve(target: &Target, width: usize, height: usize, samples: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(width * height * 4);
    let n = (samples * samples) as f32;
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 4];
            for sy in 0..samples {
                for sx in 0..samples {
                    let at = (y * samples + sy) * target.width + x * samples + sx;
                    if let Some(p) = target.color.get(at) {
                        sum = zip_with(sum, *p, |s, v| s + v);
                    }
                }
            }
            out.extend(sum.map(|v| (v / n * 255.0).round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Material;

    fn square(color: [f32; 4], z: f32) -> Mesh {
        Mesh {
            positions: vec![
                [-1.0, -1.0, z],
                [1.0, -1.0, z],
                [1.0, 1.0, z],
                [-1.0, 1.0, z],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            material: Material {
                color,
                texture: None,
            },
            ..Mesh::default()
        }
    }

    fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        let at = (y * width + x) * 4;
        [image[at], image[at + 1], image[at + 2], image[at + 3]]
    }

    #[test]
    fn w116_the_nearest_triangle_shows_and_the_rest_is_clear() {
        let scene = Scene {
            meshes: vec![
                square([1.0, 0.0, 0.0, 1.0], 0.0),
                square([0.0, 0.0, 1.0, 1.0], 0.5),
            ],
            textures: Vec::new(),
        };
        let image = render(&scene, View::default(), 40, 40).unwrap();
        assert_eq!(image.len(), 40 * 40 * 4);
        let center = pixel(&image, 40, 20, 20);
        assert_eq!(center[3], 255);
        assert!(center[2] > 100 && center[0] == 0, "{center:?}");
        // The square fits its sphere, so the corners are clear.
        assert_eq!(pixel(&image, 40, 0, 0), [0; 4]);
        // Turned half round, the red one is in front.
        let back = render(
            &scene,
            View {
                yaw: 180.0,
                pitch: 0.0,
            },
            40,
            40,
        )
        .unwrap();
        let center = pixel(&back, 40, 20, 20);
        assert!(center[0] > 100 && center[2] == 0, "{center:?}");
        assert!(render(&scene, View::default(), 0, 10).is_none());
        assert!(render(&scene, View::default(), MAX_SIDE + 1, 10).is_none());
    }

    #[test]
    fn w116_textures_and_translucent_meshes() {
        let mut quad = square([1.0; 4], 0.0);
        quad.uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        quad.material.texture = Some(0);
        let texture = Texture {
            width: 2,
            height: 2,
            // Red and green on the top row, blue and white below.
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        };
        let scene = Scene {
            meshes: vec![quad],
            textures: vec![texture],
        };
        let image = render(&scene, View::default(), 100, 100).unwrap();
        let top_left = pixel(&image, 100, 30, 30);
        let bottom_left = pixel(&image, 100, 30, 70);
        assert!(top_left[0] > 100 && top_left[1] == 0, "{top_left:?}");
        assert!(
            bottom_left[2] > 100 && bottom_left[0] == 0,
            "{bottom_left:?}"
        );

        let scene = Scene {
            meshes: vec![square([1.0, 1.0, 1.0, 0.5], 0.0)],
            textures: Vec::new(),
        };
        let image = render(&scene, View::default(), 20, 20).unwrap();
        let center = pixel(&image, 20, 10, 10);
        assert!((120..=135).contains(&center[3]), "{center:?}");
        assert!(center[0] <= center[3]);
    }
}
