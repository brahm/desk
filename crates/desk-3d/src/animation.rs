// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The animation of a model: the parts of a scene that move over time,
//! as the nodes of glTF move by their translation, rotation and scale,
//! and the prims of USD by the samples in time of their transform ops.
//!
//! The meshes of a [`Scene`](crate::Scene) stay where they rest. A part
//! that moves keeps the node it hangs from and the inverse of where that
//! node rests, so that at a time its corners go back from where they rest
//! and on to where the node is then. A node that rests at a scale of 0
//! has no inverse: the corners of its meshes are kept in its own space
//! instead, and placed where the node is even at rest.

use crate::matrix::{
    IDENTITY, Matrix, invert, mul, orient, point, quaternion, rotate, scale, translate,
};

/// The animation of a scene, read with it: see
/// [`Scene::animation`](crate::Scene::animation).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    /// How long it lasts, in seconds.
    pub(crate) duration: f32,
    /// The nodes of the scene, each after its parent.
    pub(crate) joints: Vec<Joint>,
    /// What moves them.
    pub(crate) tracks: Vec<Track>,
    /// For each mesh of the scene that moves, how.
    pub(crate) bindings: Vec<Option<Binding>>,
    /// The box around the scene at all times.
    pub(crate) bounds: ([f32; 3], [f32; 3]),
}

/// A mesh that moves: the node it hangs from, and what takes its corners
/// to the space of that node.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Binding {
    joint: usize,
    /// The inverse of where the node rests, or none when the corners are
    /// in the node's space already.
    inverse: Option<Matrix>,
}

/// The node of a mesh as it is read: its joint, and whether its corners
/// are kept in the joint's space, as they are when the joint rests where
/// it has no inverse.
pub(crate) type MeshJoint = Option<(usize, bool)>;

/// A node: its parent, and where it is in its parent when nothing moves
/// it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Joint {
    pub(crate) parent: Option<usize>,
    pub(crate) rest: Local,
}

/// Where a node is in its parent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Local {
    /// A matrix, which no track moves.
    Matrix(Matrix),
    /// A translation, a rotation (as x, y, z and w) and a scale.
    Trs {
        t: [f64; 3],
        r: [f64; 4],
        s: [f64; 3],
    },
    /// The transform ops of a prim of USD, the last applied first, each
    /// with its samples in time.
    Ops(Vec<Op>),
}

/// What a transform op of USD does with its value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OpKind {
    /// A matrix of 16 numbers, by rows.
    Transform,
    Translate,
    Scale,
    /// A quaternion: the real part, then the imaginary ones.
    Orient,
    /// A turn about one axis, by an angle in degrees.
    Rotate(usize),
    /// Turns about the three axes, the first of these turning first, by
    /// the angles about X, Y and Z.
    RotateAxes(Vec<usize>),
}

impl OpKind {
    /// The matrix of the op for a value.
    pub(crate) fn matrix(&self, v: &[f64]) -> Option<Matrix> {
        Some(match self {
            Self::Transform => {
                let mut m = [0.0; 16];
                if v.len() != 16 {
                    return None;
                }
                m.copy_from_slice(v);
                m
            }
            Self::Translate => translate(v),
            Self::Scale => scale(v),
            Self::Orient => orient(v),
            Self::Rotate(axis) => rotate(*axis, *v.first()?),
            Self::RotateAxes(axes) => {
                let mut m = IDENTITY;
                for &axis in axes {
                    m = mul(&m, &rotate(axis, v.get(axis).copied().unwrap_or(0.0)));
                }
                m
            }
        })
    }
}

/// A transform op of USD, with its value at rest and its samples.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Op {
    pub(crate) kind: OpKind,
    /// Whether it is the inverse that applies.
    pub(crate) inverse: bool,
    pub(crate) rest: Vec<f64>,
    /// The times of the samples, in seconds, each after the one before.
    pub(crate) times: Vec<f32>,
    /// The values at those times, as many numbers each as `rest` has.
    pub(crate) values: Vec<Vec<f64>>,
}

impl Op {
    /// The matrix of the op at a time, or at rest.
    fn matrix(&self, time: Option<f32>) -> Option<Matrix> {
        let value = match time {
            Some(time) => self.value(time),
            None => self.rest.clone(),
        };
        let m = self.kind.matrix(&value)?;
        if self.inverse { invert(&m) } else { Some(m) }
    }

    /// The value at a time, held before the first sample and after the
    /// last, and in line between two; a quaternion turns the short way.
    fn value(&self, time: f32) -> Vec<f64> {
        let next = self.times.partition_point(|t| *t <= time);
        let Some(k) = next.checked_sub(1) else {
            return self.values.first().unwrap_or(&self.rest).clone();
        };
        let (Some(a), Some(b), Some(t0), Some(t1)) = (
            self.values.get(k),
            self.values.get(next),
            self.times.get(k),
            self.times.get(next),
        ) else {
            return self.values.last().unwrap_or(&self.rest).clone();
        };
        let s = if t1 > t0 {
            f64::from((time - t0) / (t1 - t0))
        } else {
            0.0
        };
        if self.kind == OpKind::Orient
            && let (Ok(a4), Ok(b4)) = (
                <[f64; 4]>::try_from(a.as_slice()),
                <[f64; 4]>::try_from(b.as_slice()),
            )
        {
            return slerp(a4, b4, s).to_vec();
        }
        a.iter().zip(b).map(|(a, b)| a + (b - a) * s).collect()
    }
}

/// What a track moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Path {
    Translation,
    Rotation,
    Scale,
}

impl Path {
    /// The numbers of a value.
    pub(crate) fn width(self) -> usize {
        match self {
            Self::Rotation => 4,
            _ => 3,
        }
    }
}

/// How a track goes from one key to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Interpolation {
    Step,
    Linear,
    /// A cubic spline, with a tangent into and out of each key.
    Cubic,
}

/// The values of a translation, a rotation or a scale over time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Track {
    pub(crate) joint: usize,
    pub(crate) path: Path,
    pub(crate) interpolation: Interpolation,
    /// The times of the keys, in seconds, each after the one before.
    pub(crate) times: Vec<f32>,
    /// The values of the keys, [`Path::width`] numbers each, three times
    /// that for a cubic spline: the tangent in, the value and the tangent
    /// out.
    pub(crate) values: Vec<f32>,
}

impl Track {
    /// The value at a time, held at the first and the last keys.
    fn at(&self, time: f32) -> Option<[f64; 4]> {
        let w = self.path.width();
        let stride = if self.interpolation == Interpolation::Cubic {
            3 * w
        } else {
            w
        };
        let value = |k: usize| -> Option<[f64; 4]> {
            let from = k * stride + if stride > w { w } else { 0 };
            let v = self.values.get(from..from + w)?;
            let mut out = [0.0; 4];
            for (o, v) in out.iter_mut().zip(v) {
                *o = f64::from(*v);
            }
            Some(out)
        };
        let last = self.times.len().checked_sub(1)?;
        let next = self.times.partition_point(|t| *t <= time);
        if next == 0 {
            return value(0);
        }
        if next > last {
            return value(last);
        }
        let k = next - 1;
        let (t0, t1) = (*self.times.get(k)?, *self.times.get(next)?);
        let span = t1 - t0;
        let s = if span > 0.0 {
            f64::from((time - t0) / span)
        } else {
            0.0
        };
        let (a, b) = (value(k)?, value(next)?);
        let mut out = match self.interpolation {
            Interpolation::Step => a,
            Interpolation::Linear if self.path == Path::Rotation => slerp(a, b, s),
            Interpolation::Linear => lerp(a, b, s),
            Interpolation::Cubic => {
                // The tangent out of this key and into the next.
                let tangent = |k: usize, at: usize| -> Option<[f64; 4]> {
                    let from = k * stride + at;
                    let v = self.values.get(from..from + w)?;
                    let mut out = [0.0; 4];
                    for (o, v) in out.iter_mut().zip(v) {
                        *o = f64::from(*v) * f64::from(span);
                    }
                    Some(out)
                };
                let (out_a, in_b) = (tangent(k, 2 * w)?, tangent(next, 0)?);
                let (s2, s3) = (s * s, s * s * s);
                let mut out = [0.0; 4];
                for (i, o) in out.iter_mut().enumerate() {
                    let get = |v: &[f64; 4]| v.get(i).copied().unwrap_or(0.0);
                    *o = (2.0 * s3 - 3.0 * s2 + 1.0) * get(&a)
                        + (s3 - 2.0 * s2 + s) * get(&out_a)
                        + (-2.0 * s3 + 3.0 * s2) * get(&b)
                        + (s3 - s2) * get(&in_b);
                }
                out
            }
        };
        if self.path == Path::Rotation {
            out = unit(out);
        }
        Some(out)
    }
}

fn lerp(a: [f64; 4], b: [f64; 4], s: f64) -> [f64; 4] {
    crate::zip_with(a, b, |a, b| a + (b - a) * s)
}

fn unit(q: [f64; 4]) -> [f64; 4] {
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    if n > 0.0 && n.is_finite() {
        q.map(|v| v / n)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// The turn between two quaternions, the short way.
fn slerp(a: [f64; 4], b: [f64; 4], s: f64) -> [f64; 4] {
    let (a, mut b) = (unit(a), unit(b));
    let mut dot: f64 = a.iter().zip(&b).map(|(a, b)| a * b).sum();
    if dot < 0.0 {
        b = b.map(|v| -v);
        dot = -dot;
    }
    if dot > 0.9995 {
        return unit(lerp(a, b, s));
    }
    let angle = dot.clamp(-1.0, 1.0).acos();
    let sin = angle.sin();
    let (wa, wb) = (((1.0 - s) * angle).sin() / sin, (s * angle).sin() / sin);
    crate::zip_with(a, b, |a, b| a * wa + b * wb)
}

impl Local {
    /// The matrix, at a time for the ops of USD, or at rest.
    pub(crate) fn matrix(&self, time: Option<f32>) -> Matrix {
        match self {
            Self::Matrix(m) => *m,
            Self::Trs { t, r, s } => trs(*t, *r, *s),
            Self::Ops(ops) => {
                let mut m = IDENTITY;
                for op in ops {
                    if let Some(o) = op.matrix(time) {
                        // The last op turns the point first, so it goes
                        // leftmost.
                        m = mul(&o, &m);
                    }
                }
                if m.iter().all(|v| v.is_finite()) {
                    m
                } else {
                    IDENTITY
                }
            }
        }
    }

    /// The times of the samples of the ops of USD.
    fn times(&self) -> impl Iterator<Item = f32> + '_ {
        let ops: &[Op] = match self {
            Self::Ops(ops) => ops,
            _ => &[],
        };
        ops.iter().flat_map(|op| op.times.iter().copied())
    }

    /// Whether the ops of USD change in time.
    fn changes(&self) -> bool {
        match self {
            Self::Ops(ops) => ops
                .iter()
                .any(|op| op.values.array_windows::<2>().any(|[a, b]| a != b)),
            _ => false,
        }
    }
}

/// The matrix of a translation, a rotation and a scale: the scale first.
pub(crate) fn trs(t: [f64; 3], r: [f64; 4], s: [f64; 3]) -> Matrix {
    mul(&mul(&scale(&s), &quaternion(r)), &translate(&t))
}

impl Animation {
    /// How long it lasts, in seconds.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Where each node is in the scene at a time, or at rest.
    fn worlds(&self, time: Option<f32>) -> Vec<Matrix> {
        let mut locals: Vec<Local> = self.joints.iter().map(|j| j.rest.clone()).collect();
        if let Some(time) = time {
            for track in &self.tracks {
                let (Some(Local::Trs { t, r, s }), Some(v)) =
                    (locals.get_mut(track.joint), track.at(time))
                else {
                    continue;
                };
                match track.path {
                    Path::Translation => *t = [v[0], v[1], v[2]],
                    Path::Rotation => *r = v,
                    Path::Scale => *s = [v[0], v[1], v[2]],
                }
            }
        }
        let mut worlds: Vec<Matrix> = Vec::with_capacity(locals.len());
        for (joint, local) in self.joints.iter().zip(&locals) {
            let parent = joint
                .parent
                .and_then(|p| worlds.get(p))
                .copied()
                .unwrap_or(IDENTITY);
            worlds.push(mul(&local.matrix(time), &parent));
        }
        worlds
    }

    /// For each mesh of the scene, what takes its corners from where they
    /// are kept to where they are at a time, or at rest: none for one
    /// that is where it is kept.
    pub(crate) fn poses(&self, time: Option<f32>) -> Vec<Option<Matrix>> {
        let worlds = self.worlds(time);
        self.bindings
            .iter()
            .map(|b| {
                let b = b.as_ref()?;
                let world = worlds.get(b.joint)?;
                match &b.inverse {
                    None => Some(*world),
                    Some(_) if time.is_none() => None,
                    Some(inverse) => Some(mul(inverse, world)),
                }
            })
            .collect()
    }

    /// Makes the animation of a scene: binds the meshes that hang from
    /// moving nodes, and those kept in the space of their node, and finds
    /// the box around the scene as it moves. None when nothing moves.
    pub(crate) fn new(
        joints: Vec<Joint>,
        tracks: Vec<Track>,
        mesh_joints: &[MeshJoint],
        meshes: &[crate::Mesh],
    ) -> Option<Self> {
        let duration = tracks
            .iter()
            .filter_map(|t| t.times.last().copied())
            .chain(joints.iter().flat_map(|j| j.rest.times()))
            .fold(0.0f32, f32::max);
        if !(duration > 0.0 && duration.is_finite()) {
            return None;
        }
        // A node moves when a track moves it, its ops change, or its
        // parent moves.
        let mut moves: Vec<bool> = joints.iter().map(|j| j.rest.changes()).collect();
        for track in &tracks {
            if let Some(m) = moves.get_mut(track.joint) {
                *m = true;
            }
        }
        if !moves.contains(&true) {
            return None;
        }
        for (i, joint) in joints.iter().enumerate() {
            if joint.parent.and_then(|p| moves.get(p).copied()) == Some(true)
                && let Some(m) = moves.get_mut(i)
            {
                *m = true;
            }
        }
        let mut animation = Self {
            duration,
            joints,
            tracks,
            bindings: Vec::new(),
            bounds: ([0.0; 3], [0.0; 3]),
        };
        let rest = animation.worlds(None);
        animation.bindings = mesh_joints
            .iter()
            .map(|j| {
                let (joint, local) = (*j)?;
                if local {
                    return Some(Binding {
                        joint,
                        inverse: None,
                    });
                }
                if !moves.get(joint).copied().unwrap_or(false) {
                    return None;
                }
                Some(Binding {
                    joint,
                    inverse: Some(invert(rest.get(joint)?)?),
                })
            })
            .collect();
        if animation.bindings.iter().all(Option::is_none) {
            return None;
        }
        // The corners of the box of each mesh, at times through the
        // animation.
        let boxes: Vec<Option<([f32; 3], [f32; 3])>> = meshes.iter().map(mesh_box).collect();
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        let mut grow = |p: [f32; 3]| {
            if p.iter().all(|v| v.is_finite()) {
                lo = crate::zip_with(lo, p, f32::min);
                hi = crate::zip_with(hi, p, f32::max);
            }
        };
        // At rest, at even times, and at the keys, as many as are not too
        // many.
        const SAMPLES: usize = 48;
        const KEYS: usize = 1024;
        let mut times: Vec<Option<f32>> = vec![None];
        times.extend((0..=SAMPLES).map(|step| Some(duration * step as f32 / SAMPLES as f32)));
        let keys: Vec<f32> = animation
            .tracks
            .iter()
            .flat_map(|t| t.times.iter().copied())
            .chain(animation.joints.iter().flat_map(|j| j.rest.times()))
            .collect();
        let every = keys.len().div_ceil(KEYS).max(1);
        times.extend(keys.iter().step_by(every).map(|t| Some(*t)));
        for time in times {
            let poses = animation.poses(time);
            for (i, b) in boxes.iter().enumerate() {
                let Some((l, h)) = b else { continue };
                let pose = poses.get(i).copied().flatten();
                for corner in 0..8 {
                    let c = [0, 1, 2].map(|k| {
                        let pick_hi = (corner >> k) & 1 == 1;
                        match (pick_hi, l.get(k), h.get(k)) {
                            (true, _, Some(v)) | (false, Some(v), _) => *v,
                            _ => 0.0,
                        }
                    });
                    grow(match &pose {
                        Some(m) => point(m, c),
                        None => c,
                    });
                }
            }
        }
        if lo.iter().zip(&hi).any(|(l, h)| l > h) {
            return None;
        }
        animation.bounds = (lo, hi);
        Some(animation)
    }
}

/// The box around the corners of a mesh.
fn mesh_box(mesh: &crate::Mesh) -> Option<([f32; 3], [f32; 3])> {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    let mut any = false;
    for p in &mesh.positions {
        lo = crate::zip_with(lo, *p, f32::min);
        hi = crate::zip_with(hi, *p, f32::max);
        any = true;
    }
    any.then_some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: Path, interpolation: Interpolation, values: Vec<f32>) -> Track {
        Track {
            joint: 0,
            path,
            interpolation,
            times: vec![0.0, 1.0],
            values,
        }
    }

    #[test]
    fn w116_tracks_go_from_key_to_key() {
        let t = track(
            Path::Translation,
            Interpolation::Linear,
            vec![0.0, 0.0, 0.0, 2.0, 4.0, 6.0],
        );
        assert_eq!(t.at(0.5).unwrap()[..3], [1.0, 2.0, 3.0]);
        assert_eq!(t.at(-1.0).unwrap()[..3], [0.0, 0.0, 0.0]);
        assert_eq!(t.at(5.0).unwrap()[..3], [2.0, 4.0, 6.0]);
        let t = Track {
            interpolation: Interpolation::Step,
            ..t
        };
        assert_eq!(t.at(0.99).unwrap()[..3], [0.0, 0.0, 0.0]);
        // A half turn about Y, half way: a quarter turn.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let r = track(
            Path::Rotation,
            Interpolation::Linear,
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0],
        );
        let q = r.at(0.5).unwrap();
        assert!((q[1] - f64::from(h)).abs() < 1e-6 && (q[3] - f64::from(h)).abs() < 1e-6);
        // A cubic spline with flat tangents ends at its keys and is half
        // way at the middle.
        let c = track(
            Path::Scale,
            Interpolation::Cubic,
            [[0.0; 3], [1.0; 3], [0.0; 3], [0.0; 3], [3.0; 3], [0.0; 3]].concat(),
        );
        assert_eq!(c.at(0.0).unwrap()[0], 1.0);
        assert_eq!(c.at(1.0).unwrap()[0], 3.0);
        assert!((c.at(0.5).unwrap()[0] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn w116_a_moving_node_takes_its_meshes_and_its_children() {
        let joints = vec![
            Joint {
                parent: None,
                rest: Local::Trs {
                    t: [1.0, 0.0, 0.0],
                    r: [0.0, 0.0, 0.0, 1.0],
                    s: [1.0; 3],
                },
            },
            Joint {
                parent: Some(0),
                rest: Local::Matrix(translate(&[0.0, 1.0, 0.0])),
            },
            Joint {
                parent: None,
                rest: Local::Matrix(IDENTITY),
            },
        ];
        let tracks = vec![Track {
            joint: 0,
            path: Path::Translation,
            interpolation: Interpolation::Linear,
            times: vec![0.0, 2.0],
            values: vec![1.0, 0.0, 0.0, 5.0, 0.0, 0.0],
        }];
        let mesh = |p: [f32; 3]| crate::Mesh {
            positions: vec![p],
            ..Default::default()
        };
        let meshes = [mesh([1.0, 1.0, 0.0]), mesh([0.0, 0.0, 0.0])];
        let animation = Animation::new(
            joints,
            tracks,
            &[Some((1, false)), Some((2, false))],
            &meshes,
        )
        .unwrap();
        assert_eq!(animation.duration(), 2.0);
        let poses = animation.poses(Some(1.0));
        assert!(poses[1].is_none());
        assert_eq!(
            point(poses[0].as_ref().unwrap(), [1.0, 1.0, 0.0]),
            [3.0, 1.0, 0.0]
        );
        assert_eq!(animation.bounds, ([0.0, 0.0, 0.0], [5.0, 1.0, 0.0]));
    }
}
