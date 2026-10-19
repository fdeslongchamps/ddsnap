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
    /// Indices of the layers visible in the PSD as saved (the default recipe).
    pub fn psd_visible(&self) -> Vec<usize> {
        self.layers
            .iter()
            .filter(|l| l.visible)
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
    fn psd_visible_skips_hidden_layers() {
        assert_eq!(fixture_tree().psd_visible(), [0, 1, 2]);
    }
}
