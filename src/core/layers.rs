//! Layer tree of an open PSD, as reported by the helper's `open` command.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Position in the helper's depth-first order; the key used by `render`.
    pub index: usize,
    /// Photoshop `layer_id`; absent in files written by some tools.
    pub id: Option<u32>,
    pub name: String,
    /// Names from the top-level group down to this layer.
    pub path: Vec<String>,
    pub parent: Option<usize>,
    pub kind: String,
    /// Visibility saved in the PSD.
    pub visible: bool,
    pub has_effects: bool,
    /// 0–255, as in Photoshop.
    #[serde(default = "opaque")]
    pub opacity: u8,
    /// psd-tools blend mode name, lowercase (`normal`, `multiply`, `pass_through`…).
    #[serde(default = "normal")]
    pub blend: String,
    /// Clipped to the layer below (not reproduced by the GPU preview).
    #[serde(default)]
    pub clipping: bool,
}

fn opaque() -> u8 {
    u8::MAX
}

fn normal() -> String {
    "normal".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerTree {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub color_mode: String,
    pub layers: Vec<Layer>,
}

impl Layer {
    /// `Group/Sub/Layer`, with `\` and `/` inside names escaped as `\\` and `\/`.
    pub fn path_string(&self) -> String {
        join_path(&self.path)
    }

    pub fn is_group(&self) -> bool {
        self.kind == "group"
    }

    pub fn depth(&self) -> usize {
        self.path.len() - 1
    }
}

pub fn join_path(names: &[String]) -> String {
    names
        .iter()
        .map(|n| n.replace('\\', "\\\\").replace('/', "\\/"))
        .collect::<Vec<_>>()
        .join("/")
}

impl LayerTree {
    /// Indices of the layers visible in the PSD as saved (the default recipe): the layer and
    /// every group above it are visible, as in Photoshop, where a hidden group hides its content.
    pub fn psd_visible(&self) -> Vec<usize> {
        let by_index = |i: usize| self.layers.iter().find(|l| l.index == i);
        self.layers
            .iter()
            .filter(|layer| {
                let mut node = Some(*layer);
                while let Some(l) = node {
                    if !l.visible {
                        return false;
                    }
                    node = l.parent.and_then(by_index);
                }
                true
            })
            .map(|l| l.index)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn fixture_tree() -> LayerTree {
        serde_json::from_str(include_str!("../../tests/fixtures/open_reply.json")).unwrap()
    }

    #[test]
    fn parses_recorded_tree() {
        let tree = fixture_tree();
        assert_eq!((tree.width, tree.height, tree.depth), (8, 8, 8));
        let names: Vec<_> = tree.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Red", "G", "Blue/Bleu", "Green"]);
        assert_eq!(tree.layers[2].parent, Some(1));
        assert_eq!(tree.layers[2].depth(), 1);
        assert_eq!(tree.layers[1].kind, "group");
        assert_eq!(tree.layers[3].id, Some(30));
    }

    #[test]
    fn slash_in_name_is_escaped() {
        let tree = fixture_tree();
        assert_eq!(tree.layers[2].path_string(), "G/Blue\\/Bleu");
        assert_eq!(join_path(&["a\\b".into(), "c".into()]), "a\\\\b/c");
    }

    #[test]
    fn psd_visible_skips_children_of_hidden_groups() {
        let mut tree = fixture_tree();
        tree.layers[1].visible = false; // group G hidden, its child Blue still flagged visible
        assert_eq!(tree.psd_visible(), [0]);
    }

    #[test]
    fn psd_visible_skips_hidden_layers() {
        assert_eq!(fixture_tree().psd_visible(), [0, 1, 2]);
    }
}
