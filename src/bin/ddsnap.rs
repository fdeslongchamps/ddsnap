use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ddsnap::core::dds::{Format, Size};
use ddsnap::core::export;
use ddsnap::core::helper::Helper;
use ddsnap::core::layers::LayerTree;
use ddsnap::core::recipe::Recipe;

#[derive(Parser)]
#[command(version, about = "PSD → DDS pour skins Assetto Corsa")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Affiche l'arbre des calques d'un PSD
    Layers {
        psd: PathBuf,
        /// Sortie JSON (réponse brute de l'assistant)
        #[arg(long)]
        json: bool,
    },
    /// Exporte les sorties de <psd>.ddsnap.json (sans recette : les calques visibles du PSD)
    Export {
        psd: PathBuf,
        /// N'exporte que cette sortie (« livery » ou « livery.dds »)
        #[arg(short, long)]
        output: Option<String>,
        /// Remplace le format de la recette : auto, bc1, bc3, bc7, rgba8
        #[arg(long)]
        format: Option<Format>,
        /// Remplace la taille de la recette : source, 4096, 2048, 1024
        #[arg(long)]
        size: Option<Size>,
        /// Désactive les mipmaps
        #[arg(long)]
        no_mipmaps: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ddsnap : {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Cmd::Layers { psd, json } => {
            let tree = Helper::spawn()?.open(&psd)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&tree)?);
            } else {
                print_tree(&tree);
            }
        }
        Cmd::Export {
            psd,
            output,
            format,
            size,
            no_mipmaps,
        } => {
            let mut helper = Helper::spawn()?;
            let tree = helper.open(&psd)?;
            // Validated here: every output name is a plain .dds name, so `dir.join` stays in dir.
            let recipe = Recipe::load(&psd, &tree)?;
            let outputs: Vec<_> = match &output {
                None => recipe.outputs.iter().collect(),
                Some(name) => {
                    let found: Vec<_> = recipe
                        .outputs
                        .iter()
                        .filter(|o| o.file == *name || o.file == format!("{name}.dds"))
                        .collect();
                    if found.is_empty() {
                        let names: Vec<_> =
                            recipe.outputs.iter().map(|o| o.file.as_str()).collect();
                        return Err(format!(
                            "sortie « {name} » absente de la recette ({})",
                            names.join(", ")
                        )
                        .into());
                    }
                    found
                }
            };
            let dir = psd.parent().unwrap_or(std::path::Path::new("."));
            for out in outputs {
                let resolution = out.resolve(&tree);
                let mut settings = out.settings();
                settings.format = format.unwrap_or(settings.format);
                settings.size = size.unwrap_or(settings.size);
                settings.no_mipmaps |= no_mipmaps;
                let report = export::export(
                    &mut helper,
                    &resolution.visible,
                    settings,
                    &dir.join(&out.file),
                )?;
                for warning in resolution.warnings().iter().chain(&report.warnings) {
                    eprintln!("avertissement : {} : {warning}", out.file);
                }
                println!("{report}");
            }
        }
    }
    Ok(())
}

fn print_tree(tree: &LayerTree) {
    println!(
        "{}×{} · {} bits · {}",
        tree.width, tree.height, tree.depth, tree.color_mode
    );
    for layer in &tree.layers {
        let mut flags = Vec::new();
        if layer.kind != "pixel" {
            flags.push(layer.kind.as_str());
        }
        if !layer.visible {
            flags.push("masqué dans le PSD");
        }
        if layer.has_effects {
            flags.push("effets");
        }
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", flags.join(", "))
        };
        println!("{}{}{flags}", "  ".repeat(layer.depth()), layer.name);
    }
}
