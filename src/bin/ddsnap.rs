use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
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
