//! `<name>.ddsnap.json` next to the PSD: the outputs and their layers (ADR-004).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::dds::{Format, Size};
use super::export::Settings;
use super::layers::LayerTree;

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub version: u32,
    /// Where the DDS files go: absolute, or relative to the PSD's folder. None = PSD's folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    pub outputs: Vec<Output>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub file: String,
    #[serde(default)]
    pub format: Format,
    #[serde(default)]
    pub size: Size,
    #[serde(default = "yes")]
    pub mipmaps: bool,
    pub visible: Vec<LayerRef>,
}

fn yes() -> bool {
    true
}

/// A layer, by Photoshop id first and escaped path second; never by position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    pub path: String,
}

#[derive(Debug)]
pub enum RecipeError {
    Io(PathBuf, io::Error),
    Json(PathBuf, serde_json::Error),
    Version(u32),
    InvalidName(String),
}

impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(p, e) => write!(f, "recette {} : {e}", p.display()),
            Self::Json(p, e) => write!(f, "recette {} illisible : {e}", p.display()),
            Self::Version(v) => write!(
                f,
                "version de recette inconnue : {v} (ddsnap lit la version {VERSION})"
            ),
            Self::InvalidName(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for RecipeError {}

/// `skins/livery.psd` → `skins/livery.ddsnap.json`.
pub fn recipe_path(psd: &Path) -> PathBuf {
    psd.with_extension("ddsnap.json")
}

/// Checks an output file name: a plain `.dds` name in the PSD's folder, not the PSD,
/// not used by another output. `others` are the other outputs' names.
pub fn validate_output_name<'a>(
    name: &str,
    psd_name: &str,
    others: impl IntoIterator<Item = &'a str>,
) -> Result<(), RecipeError> {
    let refuse = |why: &str| {
        Err(RecipeError::InvalidName(format!(
            "sortie « {name} » : {why}"
        )))
    };
    // With no separator and a .dds ending, a name can never reach `..` or another folder.
    if name.contains('/') || name.contains('\\') {
        return refuse("un simple nom de fichier est attendu (sans / ni \\)");
    }
    if name == psd_name {
        return refuse("ce serait écraser le PSD");
    }
    let stem_ok = name.len() > 4 && name.is_char_boundary(name.len() - 4);
    if !stem_ok || !name[name.len() - 4..].eq_ignore_ascii_case(".dds") {
        return refuse("l'extension doit être .dds");
    }
    if others.into_iter().any(|o| o == name) {
        return refuse("nom déjà utilisé par une autre sortie");
    }
    Ok(())
}

impl Recipe {
    pub fn output_dir(&self, psd: &Path) -> PathBuf {
        let psd_dir = psd.parent().unwrap_or(Path::new("."));
        match &self.output_dir {
            Some(dir) => psd_dir.join(dir), // `join` keeps an absolute `dir` as is
            None => psd_dir.to_path_buf(),
        }
    }

    /// One output named after the PSD, with the layers visible in the PSD.
    pub fn default_for(psd: &Path, tree: &LayerTree) -> Recipe {
        let stem = psd.file_stem().unwrap_or_default().to_string_lossy();
        Recipe {
            version: VERSION,
            output_dir: None,
            outputs: vec![Output {
                file: format!("{stem}.dds"),
                format: Format::Auto,
                size: Size::Source,
                mipmaps: true,
                visible: tree
                    .psd_visible()
                    .into_iter()
                    .map(|i| {
                        let layer = &tree.layers[i];
                        LayerRef {
                            id: layer.id,
                            path: layer.path_string(),
                        }
                    })
                    .collect(),
            }],
        }
    }

    /// Checks the version and every output name.
    pub fn validate(&self, psd_name: &str) -> Result<(), RecipeError> {
        if self.version != VERSION {
            return Err(RecipeError::Version(self.version));
        }
        for (i, output) in self.outputs.iter().enumerate() {
            let others = self.outputs[..i].iter().map(|o| o.file.as_str());
            validate_output_name(&output.file, psd_name, others)?;
        }
        Ok(())
    }

    /// Reads and validates the PSD's recipe; the default recipe when there is none yet.
    pub fn load(psd: &Path, tree: &LayerTree) -> Result<Recipe, RecipeError> {
        let path = recipe_path(psd);
        let json = match fs::read_to_string(&path) {
            Ok(json) => json,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::default_for(psd, tree));
            }
            Err(e) => return Err(RecipeError::Io(path, e)),
        };
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| RecipeError::Json(path.clone(), e))?;
        // Version first: a newer schema must say so, not fail on a renamed field.
        match value["version"].as_u64() {
            Some(v) if v != u64::from(VERSION) => {
                return Err(RecipeError::Version(u32::try_from(v).unwrap_or(u32::MAX)));
            }
            _ => {}
        }
        let recipe = Recipe::deserialize(value).map_err(|e| RecipeError::Json(path, e))?;
        recipe.validate(&psd.file_name().unwrap_or_default().to_string_lossy())?;
        Ok(recipe)
    }
}

/// Where each recipe layer was found in the open PSD.
#[derive(Debug, Default, PartialEq)]
pub struct Resolution {
    /// Indices for `render`.
    pub visible: Vec<usize>,
    /// Layers whose id vanished but whose path still matches (recreated or copied).
    pub by_path_only: Vec<String>,
    /// Layers not found, or whose path now matches several layers.
    pub missing: Vec<String>,
    /// (layer, group) pairs: the layer is listed but a group above it is not, so the
    /// helper will not draw it.
    pub outside_group: Vec<(String, String)>,
}

impl Resolution {
    pub fn warnings(&self) -> Vec<String> {
        let by_path = self
            .by_path_only
            .iter()
            .map(|p| format!("calque « {p} » retrouvé par son chemin seulement (recréé ?)"));
        let missing = self
            .missing
            .iter()
            .map(|p| format!("calque « {p} » de la recette introuvable"));
        let outside = self.outside_group.iter().map(|(layer, group)| {
            format!("calque « {layer} » non dessiné : son groupe « {group} » n'est pas coché")
        });
        by_path.chain(missing).chain(outside).collect()
    }
}

impl Output {
    pub fn resolve(&self, tree: &LayerTree) -> Resolution {
        let mut res = Resolution::default();
        for wanted in &self.visible {
            if let Some(layer) = wanted
                .id
                .and_then(|id| tree.layers.iter().find(|l| l.id == Some(id)))
            {
                res.visible.push(layer.index);
                continue;
            }
            let mut same_path = tree
                .layers
                .iter()
                .filter(|l| l.path_string() == wanted.path);
            match (same_path.next(), same_path.next()) {
                (Some(layer), None) => {
                    res.visible.push(layer.index);
                    if wanted.id.is_some() {
                        res.by_path_only.push(wanted.path.clone());
                    }
                }
                _ => res.missing.push(wanted.path.clone()),
            }
        }
        // By `index`, not position: the two only match in trees straight from the helper.
        let by_index = |i: usize| tree.layers.iter().find(|l| l.index == i);
        for &index in &res.visible {
            let Some(layer) = by_index(index) else {
                continue;
            };
            let mut parent = layer.parent.and_then(by_index);
            while let Some(group) = parent {
                if !res.visible.contains(&group.index) {
                    let pair = (layer.path_string(), group.path_string());
                    res.outside_group.push(pair);
                    break;
                }
                parent = group.parent.and_then(by_index);
            }
        }
        res
    }

    pub fn settings(&self) -> Settings {
        Settings {
            format: self.format,
            size: self.size,
            no_mipmaps: !self.mipmaps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::layers::Layer;

    fn layer(index: usize, id: Option<u32>, path: &[&str], parent: Option<usize>) -> Layer {
        Layer {
            index,
            id,
            name: path.last().unwrap().to_string(),
            path: path.iter().map(|s| s.to_string()).collect(),
            parent,
            kind: "pixel".into(),
            visible: true,
            has_effects: false,
        }
    }

    fn tree(layers: Vec<Layer>) -> LayerTree {
        LayerTree {
            width: 8,
            height: 8,
            depth: 8,
            color_mode: "RGB".into(),
            layers,
        }
    }

    /// Base(12), Sponsors(31), Numéro{42(47)}.
    fn original() -> LayerTree {
        tree(vec![
            layer(0, Some(12), &["Base"], None),
            layer(1, Some(31), &["Sponsors"], None),
            layer(2, Some(46), &["Numéro"], None),
            layer(3, Some(47), &["Numéro", "42"], Some(2)),
        ])
    }

    fn output_of(tree: &LayerTree) -> Output {
        Recipe::default_for(Path::new("/s/livery.psd"), tree)
            .outputs
            .remove(0)
    }

    #[test]
    fn default_recipe_round_trips() {
        let recipe = Recipe::default_for(Path::new("/s/livery.psd"), &original());
        assert_eq!(recipe.outputs[0].file, "livery.dds");
        assert_eq!(
            recipe.outputs[0].visible[3],
            LayerRef {
                id: Some(47),
                path: "Numéro/42".into()
            }
        );
        let json = serde_json::to_string_pretty(&recipe).unwrap();
        assert!(json.contains("\"format\": \"auto\"") && json.contains("\"size\": \"source\""));
        assert_eq!(serde_json::from_str::<Recipe>(&json).unwrap(), recipe);
    }

    #[test]
    fn reads_the_spec_example() {
        let json = r#"{"version": 1, "outputs": [{"file": "livery.dds", "format": "bc3",
            "size": "source", "mipmaps": true, "visible": [{"id": 12, "path": "Base"}]}]}"#;
        let recipe: Recipe = serde_json::from_str(json).unwrap();
        assert_eq!(recipe.outputs[0].format, Format::Bc3);
        recipe.validate("livery.psd").unwrap();
    }

    #[test]
    fn renamed_or_moved_layer_found_by_id_silently() {
        let output = output_of(&original());
        let mut moved = original();
        moved.layers[3].path = vec!["Sponsors".into(), "Quarante-deux".into()];
        moved.layers[3].parent = Some(1);
        let res = output.resolve(&moved);
        assert_eq!(res.visible, [0, 1, 2, 3]);
        assert!(res.warnings().is_empty());
    }

    #[test]
    fn layer_inserted_above_changes_nothing_else() {
        let output = output_of(&original());
        let mut layers = vec![layer(0, Some(99), &["Nouveau"], None)];
        for mut l in original().layers {
            l.index += 1;
            l.parent = l.parent.map(|p| p + 1);
            layers.push(l);
        }
        let res = output.resolve(&tree(layers));
        assert_eq!(res.visible, [1, 2, 3, 4]); // "Nouveau" (0) stays off
        assert!(res.warnings().is_empty());
    }

    #[test]
    fn recreated_layer_found_by_path_with_warning() {
        let output = output_of(&original());
        let mut recreated = original();
        recreated.layers[1].id = Some(77);
        let res = output.resolve(&recreated);
        assert_eq!(res.visible, [0, 1, 2, 3]);
        assert_eq!(res.by_path_only, ["Sponsors"]);
        assert_eq!(res.warnings().len(), 1);
    }

    #[test]
    fn deleted_layer_is_reported() {
        let output = output_of(&original());
        let mut deleted = original();
        deleted.layers.remove(1);
        let res = output.resolve(&deleted);
        assert_eq!(res.missing, ["Sponsors"]);
        assert_eq!(
            res.warnings(),
            ["calque « Sponsors » de la recette introuvable"]
        );
    }

    #[test]
    fn same_name_layers_resolve_by_id_and_are_ambiguous_without() {
        let twins = tree(vec![
            layer(0, Some(1), &["Logo"], None),
            layer(1, Some(2), &["Logo"], None),
        ]);
        let pick_second = Output {
            visible: vec![LayerRef {
                id: Some(2),
                path: "Logo".into(),
            }],
            ..output_of(&twins)
        };
        assert_eq!(pick_second.resolve(&twins).visible, [1]);
        let no_id = Output {
            visible: vec![LayerRef {
                id: None,
                path: "Logo".into(),
            }],
            ..output_of(&twins)
        };
        assert_eq!(no_id.resolve(&twins).missing, ["Logo"]);
    }

    #[test]
    fn layer_without_its_group_is_reported() {
        let only_child = Output {
            visible: vec![LayerRef {
                id: Some(47),
                path: "Numéro/42".into(),
            }],
            ..output_of(&original())
        };
        let res = only_child.resolve(&original());
        assert_eq!(res.visible, [3]);
        assert_eq!(
            res.warnings(),
            ["calque « Numéro/42 » non dessiné : son groupe « Numéro » n'est pas coché"]
        );
        assert!(
            output_of(&original())
                .resolve(&original())
                .warnings()
                .is_empty()
        );
    }

    #[test]
    fn psd_without_ids_resolves_by_path_without_warning() {
        let mut no_ids = original();
        no_ids.layers.iter_mut().for_each(|l| l.id = None);
        let res = output_of(&no_ids).resolve(&no_ids);
        assert_eq!(res.visible, [0, 1, 2, 3]);
        assert!(res.warnings().is_empty());
    }

    #[test]
    fn unknown_version_is_a_clear_error() {
        let mut recipe = Recipe::default_for(Path::new("/s/livery.psd"), &original());
        recipe.version = 2;
        let err = recipe.validate("livery.psd").unwrap_err();
        assert_eq!(
            err.to_string(),
            "version de recette inconnue : 2 (ddsnap lit la version 1)"
        );
    }

    #[test]
    fn future_version_reports_version_not_schema() {
        let dir = std::env::temp_dir().join(format!("ddsnap-unit-v2-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let psd = dir.join("livery.psd");
        fs::write(
            recipe_path(&psd),
            r#"{"version": 2, "outputs": [{"file": "a.dds", "layers": []}]}"#,
        )
        .unwrap();
        let err = Recipe::load(&psd, &original()).unwrap_err();
        let _ = fs::remove_dir_all(&dir);
        assert!(matches!(err, RecipeError::Version(2)), "{err}");
    }

    #[test]
    fn output_dir_defaults_to_psd_folder() {
        let psd = Path::new("/skins/src/livery.psd");
        let mut recipe = Recipe::default_for(psd, &original());
        assert_eq!(recipe.output_dir(psd), Path::new("/skins/src"));
        recipe.output_dir = Some("../export".into());
        assert_eq!(recipe.output_dir(psd), Path::new("/skins/src/../export"));
        recipe.output_dir = Some("/ac/skins/42".into());
        assert_eq!(recipe.output_dir(psd), Path::new("/ac/skins/42"));
        let json = serde_json::to_string(&Recipe::default_for(psd, &original())).unwrap();
        assert!(
            !json.contains("output_dir"),
            "absent tant qu'il n'est pas choisi"
        );
    }

    #[test]
    fn bad_output_names_are_refused() {
        for bad in ["livery.psd", "../x.dds", "a/b.dds", "x.png", ".dds", "é.dd"] {
            assert!(
                validate_output_name(bad, "livery.psd", []).is_err(),
                "{bad}"
            );
        }
        assert!(validate_output_name("x.dds", "livery.psd", ["x.dds"]).is_err());
        assert!(validate_output_name("ext_config_mask.DDS", "livery.psd", ["livery.dds"]).is_ok());
        assert!(validate_output_name("livery..v2.dds", "livery.psd", []).is_ok());

        let mut recipe = Recipe::default_for(Path::new("/s/livery.psd"), &original());
        recipe.outputs.push(recipe.outputs[0].clone());
        let err = recipe.validate("livery.psd").unwrap_err();
        assert!(err.to_string().contains("déjà utilisé"), "{err}");
    }
}
