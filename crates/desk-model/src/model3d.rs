// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::unwrap_used))]

//! 3D objects (W116, PXV-25): a model from a file, as it came, drawn in
//! its frame and turned as the one who placed it turned it by dragging.
//! `desk-3d` reads and draws the file; the model keeps only the bytes, the
//! turn and the description.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::{Object, ObjectKind};

pub use desk_3d::Format as ModelFormat;

/// The most a model may be tipped toward or away, in hundredths of a
/// degree: a quarter turn, to see it from straight above or below.
pub const MAX_PITCH: i32 = 9000;

/// A 3D model, with how it is turned.
#[derive(Clone)]
pub struct Model3d {
    data: Arc<[u8]>,
    format: ModelFormat,
    /// The turn about the vertical axis, in hundredths of a degree, from
    /// -18000 up to 18000; a positive one brings the right of the model
    /// forward.
    pub yaw: i32,
    /// The tip about the horizontal axis, in hundredths of a degree,
    /// within [`MAX_PITCH`] each way; a positive one brings its top
    /// forward.
    pub pitch: i32,
    /// What it shows, for those who cannot see it, as the Description of
    /// the Object tab of Pages 15.4; none when it has none.
    pub description: Option<Arc<str>>,
}

impl Model3d {
    /// A model from the bytes of its file, if Desk reads its format, seen
    /// from the front.
    pub fn from_bytes(data: impl Into<Arc<[u8]>>) -> Option<Self> {
        let data = data.into();
        let format = ModelFormat::sniff(&data)?;
        Some(Self {
            data,
            format,
            yaw: 0,
            pitch: 0,
            description: None,
        })
    }

    /// The bytes of the file.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The bytes of the file, shared rather than copied.
    pub fn shared_data(&self) -> Arc<[u8]> {
        Arc::clone(&self.data)
    }

    /// The format of the file.
    pub fn format(&self) -> ModelFormat {
        self.format
    }

    /// Whether it is the same file as `other`, not a copy.
    pub fn same_file(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }

    /// The model turned to a yaw and a pitch, in hundredths of a degree:
    /// the yaw brought within a turn, the pitch within [`MAX_PITCH`].
    pub fn turned(&self, yaw: i32, pitch: i32) -> Self {
        Self {
            yaw: (yaw.rem_euclid(36000) + 18000) % 36000 - 18000,
            pitch: pitch.clamp(-MAX_PITCH, MAX_PITCH),
            ..self.clone()
        }
    }

    /// Its scene, read from its file.
    pub fn scene(&self) -> desk_3d::Result<desk_3d::Scene> {
        desk_3d::read(&self.data)
    }

    /// How `desk-3d` draws it.
    pub fn view(&self) -> desk_3d::View {
        desk_3d::View {
            yaw: self.yaw as f32 / 100.0,
            pitch: self.pitch as f32 / 100.0,
        }
    }
}

impl Object {
    /// Its 3D model, if it is one.
    pub fn model(&self) -> Option<&Model3d> {
        match &self.kind {
            ObjectKind::Model(model) => Some(model),
            _ => None,
        }
    }
}

impl std::fmt::Debug for Model3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model3d")
            .field("format", &self.format)
            .field("bytes", &self.data.len())
            .field("yaw", &self.yaw)
            .field("pitch", &self.pitch)
            .field("description", &self.description)
            .finish()
    }
}

impl PartialEq for Model3d {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.data, &other.data) || self.data == other.data)
            && self.format == other.format
            && self.yaw == other.yaw
            && self.pitch == other.pitch
            && self.description == other.description
    }
}

impl Eq for Model3d {}

impl Hash for Model3d {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.len().hash(state);
        self.format.hash(state);
        self.yaw.hash(state);
        self.pitch.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_models_turn_within_a_turn() {
        assert!(Model3d::from_bytes(b"not a model".to_vec()).is_none());
        let mut zip = b"PK\x03\x04".to_vec();
        zip.extend([0; 22]);
        zip.extend(10u16.to_le_bytes());
        zip.extend(0u16.to_le_bytes());
        zip.extend(b"scene.usdc");
        let model = Model3d::from_bytes(zip).unwrap();
        assert_eq!(model.format(), ModelFormat::Usdz);
        let turned = model.turned(27000, 12000);
        assert_eq!((turned.yaw, turned.pitch), (-9000, MAX_PITCH));
        assert_eq!(model.turned(-18000, 0).yaw, -18000);
        assert_eq!(model.turned(18000, 0).yaw, -18000);
        assert_eq!(model.turned(36500, -100).yaw, 500);
        assert!(turned.same_file(&model));
        assert_ne!(turned, model);
        assert_eq!(turned.view().yaw, -90.0);
    }
}
