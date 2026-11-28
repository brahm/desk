// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Universal Scene Description: the layers that the text and the binary
//! readers make, as a tree of prims with their attributes, relationships
//! and references, and the meshes of a scene built from them.
//!
//! Only what drawing a model needs is kept: the prims, their transforms,
//! the meshes with their primvars and the `UsdPreviewSurface` materials
//! bound to them, and the references that bring other layers of the same
//! package. Variants, inherits, specializes, instancing and time are left
//! out; a value that changes in time is taken at its first sample.

mod scene;
pub(crate) mod usda;
pub(crate) mod usdc;

use std::collections::BTreeMap;

pub(crate) use scene::{Files, build};

/// A value of an attribute, or of the metadata of a prim.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A number.
    Float(f64),
    /// A token or a string.
    Text(String),
    /// An asset path: a file, as `textures/wood.png`.
    Asset(String),
    /// A vector, a color or a quaternion, as its numbers; quaternions as
    /// the real part, then the imaginary ones.
    Tuple(Vec<f64>),
    /// A 4×4 matrix, by rows, with the translation in the last row.
    Matrix(Box<[f64; 16]>),
    /// An array of integers.
    Ints(Vec<i32>),
    /// An array of numbers or of vectors of `dim` numbers each, one
    /// vector after another.
    Floats {
        /// How many numbers each element has.
        dim: usize,
        /// The numbers.
        data: Vec<f32>,
    },
    /// An array of tokens or strings.
    Texts(Vec<String>),
    /// An array of matrices, as of `xformOp:transform` samples.
    Matrices(Vec<[f64; 16]>),
}

impl Value {
    /// The value as a number.
    pub(crate) fn number(&self) -> Option<f64> {
        match self {
            Self::Int(i) => Some(*i as f64),
            Self::Float(f) => Some(*f),
            Self::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    /// The value as a token or a string.
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Text(t) | Self::Asset(t) => Some(t),
            _ => None,
        }
    }

    /// The value as a vector of numbers: a tuple, or a single number.
    pub(crate) fn tuple(&self) -> Option<Vec<f64>> {
        match self {
            Self::Tuple(t) => Some(t.clone()),
            Self::Floats { data, .. } => Some(data.iter().map(|v| f64::from(*v)).collect()),
            other => other.number().map(|n| vec![n]),
        }
    }
}

/// An attribute of a prim.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Attribute {
    /// Its value: its default, or else its first sample in time.
    pub(crate) value: Option<Value>,
    /// The `interpolation` of a primvar: `constant`, `uniform`, `vertex`,
    /// `varying` or `faceVarying`.
    pub(crate) interpolation: Option<String>,
    /// The attribute it is connected to, by its path, as
    /// `/Looks/Wood/Texture.outputs:rgb`.
    pub(crate) connection: Option<String>,
}

/// A reference or a payload: another layer, or a prim of one, brought
/// under a prim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    /// The layer, as a path in the package; empty for a prim of the same
    /// layer.
    pub(crate) asset: String,
    /// The prim of that layer, by its path; none for its default prim.
    pub(crate) prim: Option<String>,
}

/// A prim: a node of the scene, with its children.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prim {
    /// Its name, as `Cube`; empty for the root of a layer.
    pub(crate) name: String,
    /// Its type, as `Mesh` or `Xform`; empty when it has none.
    pub(crate) type_name: String,
    /// Whether it is active; an inactive prim is left out with all it
    /// holds.
    pub(crate) active: bool,
    /// Its attributes, by name, as `points` or `primvars:st`.
    pub(crate) attributes: BTreeMap<String, Attribute>,
    /// The targets of its relationships, by name, as `material:binding`.
    pub(crate) relationships: BTreeMap<String, Vec<String>>,
    /// Its references and payloads.
    pub(crate) references: Vec<Reference>,
    /// Its children, in order.
    pub(crate) children: Vec<Prim>,
}

impl Prim {
    /// A prim of a name, with nothing in it.
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_name: String::new(),
            active: true,
            attributes: BTreeMap::new(),
            relationships: BTreeMap::new(),
            references: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The value of an attribute.
    pub(crate) fn value(&self, name: &str) -> Option<&Value> {
        self.attributes.get(name)?.value.as_ref()
    }

    /// The child of a name.
    pub(crate) fn child(&self, name: &str) -> Option<&Prim> {
        self.children.iter().find(|c| c.name == name)
    }

    /// The child of a name, made when there is none.
    pub(crate) fn child_mut(&mut self, name: &str) -> &mut Prim {
        let at = match self.children.iter().position(|c| c.name == name) {
            Some(at) => at,
            None => {
                self.children.push(Prim::new(name));
                self.children.len() - 1
            }
        };
        // The child is there: it was found or pushed.
        #[allow(clippy::indexing_slicing)]
        &mut self.children[at]
    }

    /// The prim at a path below this one, as `/Root/Cube`.
    pub(crate) fn find(&self, path: &str) -> Option<&Prim> {
        let mut prim = self;
        for name in path.split('/').filter(|n| !n.is_empty()) {
            prim = prim.child(name)?;
        }
        Some(prim)
    }

    /// The prim at a path below this one, made with its parents when it
    /// is not there.
    pub(crate) fn find_mut(&mut self, path: &str) -> &mut Prim {
        let mut prim = self;
        for name in path.split('/').filter(|n| !n.is_empty()) {
            prim = prim.child_mut(name);
        }
        prim
    }
}

/// A layer: a file of USD, as its root prim with the prims under it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layer {
    /// The root, whose children are the prims at the top of the layer.
    pub(crate) root: Prim,
    /// The `defaultPrim`: the prim that a reference to the layer brings.
    pub(crate) default_prim: Option<String>,
    /// The `upAxis`: whether Z is up, rather than Y.
    pub(crate) z_up: bool,
}

impl Layer {
    /// An empty layer.
    pub(crate) fn new() -> Self {
        Self {
            root: Prim::new(""),
            default_prim: None,
            z_up: false,
        }
    }
}

/// Reads a layer, binary or text, from the bytes of its file.
pub(crate) fn read_layer(bytes: &[u8]) -> crate::Result<Layer> {
    if bytes.starts_with(b"PXR-USDC") {
        return usdc::read(bytes);
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| crate::malformed("a text layer not in UTF-8"))?;
    if !text.trim_start().starts_with("#usda") {
        return Err(crate::Error::Unknown);
    }
    usda::read(text)
}

/// Splits the path of a property into the path of its prim and its name,
/// as `/Looks/Wood/Surface.outputs:surface`.
pub(crate) fn split_property(path: &str) -> (&str, &str) {
    let slash = path.rfind('/').map_or(0, |s| s + 1);
    match path.get(slash..).and_then(|last| last.find('.')) {
        Some(dot) => (
            path.get(..slash + dot).unwrap_or(path),
            path.get(slash + dot + 1..).unwrap_or(""),
        ),
        None => (path, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_paths_of_properties_split_at_their_dot() {
        assert_eq!(
            split_property("/Looks/Wood/Surface.outputs:surface"),
            ("/Looks/Wood/Surface", "outputs:surface")
        );
        assert_eq!(split_property("/Root/Cube"), ("/Root/Cube", ""));
        let mut root = Prim::new("");
        root.find_mut("/A/B").type_name = "Mesh".into();
        assert_eq!(root.find("/A/B").unwrap().type_name, "Mesh");
        assert!(root.find("/A/C").is_none());
    }
}
