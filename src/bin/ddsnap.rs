use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
        /// Dossier où écrire les DDS (défaut : celui de la recette, sinon celui du PSD)
        #[arg(long)]
        out_dir: Option<PathBuf>,
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
            out_dir,
        } => {
            let progress = Progress::start();
            progress.step("démarrage de l'assistant psd-tools");
            let mut helper = Helper::spawn()?;
            progress.step(&format!("ouverture du PSD {}", psd.display()));
            let tree = helper.open(&psd)?;
            progress.step("lecture de la recette");
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
            let dir = out_dir.unwrap_or_else(|| recipe.output_dir(&psd));
            if !dir.is_dir() {
                return Err(format!("dossier de sortie introuvable : {}", dir.display()).into());
            }
            for out in outputs {
                let resolution = out.resolve(&tree);
                let mut settings = out.settings();
                settings.format = format.unwrap_or(settings.format);
                settings.size = size.unwrap_or(settings.size);
                settings.no_mipmaps |= no_mipmaps;
                let as_saved = tree.is_psd_visibility(&resolution.visible);
                let report = export::export(
                    &mut helper,
                    &resolution.visible,
                    as_saved,
                    settings,
                    &dir.join(&out.file),
                    &mut |s| progress.step(&format!("{} : {s}", out.file)),
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

/// Prints each step on stderr with the elapsed time, and a reminder every 15 s while a
/// step runs, so a long psd-tools render never looks frozen.
struct Progress {
    start: Instant,
    current: Arc<Mutex<String>>,
}

impl Progress {
    fn start() -> Progress {
        let progress = Progress {
            start: Instant::now(),
            current: Arc::default(),
        };
        let (start, current) = (progress.start, Arc::downgrade(&progress.current));
        thread::spawn(move || {
            let mut last = String::new();
            let mut since = Instant::now();
            // Ends once `Progress` (the only strong reference) is dropped.
            while let Some(current) = current.upgrade() {
                thread::sleep(Duration::from_secs(1));
                let Ok(now) = current.lock().map(|c| c.clone()) else {
                    break;
                };
                if now != last {
                    (last, since) = (now, Instant::now());
                } else if since.elapsed() >= Duration::from_secs(15) {
                    eprintln!(
                        "[{:>6.1} s] … toujours en cours : {now}",
                        start.elapsed().as_secs_f64()
                    );
                    since = Instant::now();
                }
            }
        });
        progress
    }

    fn step(&self, label: &str) {
        eprintln!("[{:>6.1} s] {label}", self.start.elapsed().as_secs_f64());
        if let Ok(mut current) = self.current.lock() {
            *current = label.to_string();
        }
    }
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
