use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ddsnap::core::dds::{Format, Size};
use ddsnap::core::export::{self, Settings};
use ddsnap::core::helper::Helper;
use ddsnap::core::layers::LayerTree;

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
    /// Exporte le PSD en DDS (calques visibles dans le PSD)
    Export {
        psd: PathBuf,
        /// auto, bc1, bc3, bc7, rgba8
        #[arg(long, default_value = "auto")]
        format: Format,
        /// source, 4096, 2048, 1024
        #[arg(long, default_value = "source")]
        size: Size,
        #[arg(long)]
        no_mipmaps: bool,
        /// Fichier de sortie (défaut : <psd>.dds à côté du PSD)
        #[arg(short, long)]
        output: Option<PathBuf>,
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
            format,
            size,
            no_mipmaps,
            output,
        } => {
            let dest = output.unwrap_or_else(|| psd.with_extension("dds"));
            if std::path::absolute(&dest)? == std::path::absolute(&psd)? {
                return Err("la sortie ne peut pas être le PSD lui-même".into());
            }
            let mut helper = Helper::spawn()?;
            let tree = helper.open(&psd)?;
            let settings = Settings {
                format,
                size,
                no_mipmaps,
            };
            let report = export::export(&mut helper, &tree.psd_visible(), settings, &dest)?;
            for warning in &report.warnings {
                eprintln!("avertissement : {warning}");
            }
            println!("{report}");
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
