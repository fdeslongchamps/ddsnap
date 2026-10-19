# Spec : Ddsnap — PSD → DDS pour skins Assetto Corsa, GUI GNOME

Statut : **approuvée** · 2026-10-19
Projet frère : [`../Gui_FFMEG`](../Gui_FFMEG/SPEC.md) (même stack, mêmes conventions).

## Objectif

Une app de bureau GNOME qui ouvre **un PSD**, me laisse choisir **quels calques** composent
chaque texture, me montre **l'aperçu du DDS final** (compression comprise) et l'enregistre au
bon format pour Assetto Corsa. Plus besoin d'ouvrir Photoshop/Wine ni de masquer des calques à
la main avant chaque export.

Utilisateur : moi, peintre de livrées AC sous Ubuntu. Objectif secondaire : réutiliser le
moteur depuis un terminal et, plus tard, depuis PhotoCraft.

### Parcours utilisateur

1. J'ouvre l'app → page d'accueil « Déposez un PSD » + bouton « Ouvrir… » (Ctrl+O). Un PSD
   peut être déposé n'importe où dans la fenêtre.
2. Le PSD est lu par psd-tools → fenêtre en deux volets :
   - **gauche** : l'arbre des calques et groupes (nom, vignette, case à cocher de visibilité,
     indicateur « masqué dans le PSD ») ;
   - **droite** : l'aperçu, avec zoom (ajusté / 100 % / molette), déplacement, et un
     sélecteur **RVB / Alpha / RVB+A sur damier**.
3. En haut à gauche : la liste des **sorties** du PSD. Une sortie =
   `nom de fichier + ensemble de calques visibles + format DDS`. Par défaut une sortie
   « livery » avec la visibilité d'origine du PSD. Je peux en ajouter (ex. `livery.dds`,
   `ext_config_mask.dds`), les dupliquer, les renommer, les supprimer.
4. Je coche/décoche des calques → l'aperçu se recalcule (anti-rebond 300 ms, indicateur de
   calcul, l'interface ne gèle jamais).
5. Je choisis le format (préréglages AC ci-dessous) et la taille. Le bouton **« Aperçu
   compressé »** montre l'image décodée depuis le DDS réellement produit, pour voir les
   artefacts BC1/BC3 avant d'enregistrer. Bascule rapide Avant/Après.
6. **« Exporter »** (Ctrl+E) écrit la sortie active ; **« Tout exporter »** (Ctrl+Maj+E)
   écrit toutes les sorties. Dossier de sortie par défaut : celui du PSD. Si le fichier existe,
   confirmation « Remplacer » (c'est le cas normal : on réexporte la skin), jamais le PSD.
7. Fin → toast « livery.dds — 4096×4096 · BC3 · 21,3 Mo » avec « Afficher dans le dossier ».
   Erreur → message lisible + détails techniques dans un expander + bouton « Copier le
   rapport » (voir Diagnostic).

### Recette par PSD

Les sorties d'un PSD sont mémorisées dans un fichier **à côté du PSD** : `<nom>.ddsnap.json`
(lisible, versionnable avec la skin, réutilisé par la CLI). Les calques y sont identifiés par
leur chemin (`Groupe/Sous-groupe/Calque`) **et** leur index ; si un calque a été renommé ou
supprimé depuis, l'app le signale (« 2 calques de la recette introuvables ») au lieu de
l'ignorer en silence.

```json
{
  "version": 1,
  "outputs": [
    {
      "file": "livery.dds",
      "format": "bc3",
      "size": "source",
      "mipmaps": true,
      "visible": ["Base", "Sponsors", "Numéro/42"]
    }
  ]
}
```

### Formats DDS — préréglages Assetto Corsa

| Préréglage | Format | Quand |
|---|---|---|
| **Auto** (défaut) | BC1 si l'alpha est entièrement opaque, sinon BC3 | cas général |
| Couleur sans alpha | BC1 / DXT1 | livery, decals opaques |
| Couleur + alpha | BC3 / DXT5 | livery avec transparence, masques |
| Haute qualité | BC7 | nécessite Custom Shaders Patch — avertissement affiché |
| Sans perte | RGBA8 non compressé | debug, petites textures |

- Mipmaps générés par défaut (filtre Lanczos/Kaiser, alpha préservé), désactivables.
- Taille : « source » ou réduction 4096 / 2048 / 1024. Si une dimension n'est pas une
  puissance de 2 ou pas multiple de 4 → avertissement (BCn exige des blocs 4×4).
- Les PSD 16 bits et CMYK sont convertis en RGBA 8 bits ; la conversion est indiquée dans
  l'interface.

### CLI (même cœur, sans GTK)

```bash
ddsnap export livery.psd                  # toutes les sorties de livery.ddsnap.json
ddsnap export livery.psd --output livery  # une seule sortie
ddsnap layers livery.psd                  # arbre des calques (texte ou --json)
ddsnap doctor                             # vérifie python3, psd-tools, versions, chemins
```

La CLI est la frontière d'intégration : un script, un `Makefile`, ou plus tard PhotoCraft
l'appellent sans dépendre de l'interface graphique.

### Diagnostic (« pouvoir réparer mes problèmes »)

- Journal rotatif dans `~/.local/state/ddsnap/ddsnap.log` (opérations, durées, erreurs,
  stderr Python complet). `DDSNAP_LOG=debug` pour plus de détail.
- Toute erreur affiche : quoi (phrase en français), où (fichier, calque), et un bouton
  **« Copier le rapport »** = version de l'app, versions python/psd-tools, OS, dernières
  lignes du journal. C'est ce que je colle dans Claude Code ou une issue pour qu'on corrige.
- `ddsnap doctor` (et Aide › Diagnostic dans l'app) : vérifie l'environnement et dit
  exactement quoi installer.
- Un PSD qui pose problème doit pouvoir devenir un test : `ddsnap layers --json` + le
  rendu attendu servent de fixtures de régression.

## Stack technique

| Élément | Choix | Version |
|---|---|---|
| Langage | Rust, édition 2024 | stable ≥ 1.92 |
| UI | gtk4-rs + libadwaita-rs | gtk4 0.11.5 (`v4_14`), libadwaita 0.9.2 (`v1_5`) |
| Interface déclarative | Blueprint → GtkBuilder, compilé dans `build.rs` | blueprint-compiler ≥ 0.10 |
| Async | boucle GLib + `gio::Subprocess`, pas de tokio | — |
| Lecture PSD + composition | **psd-tools** (Python) dans un processus assistant long-vivant | python ≥ 3.12, psd-tools ≥ 1.10 |
| Encodage DDS | crate `image_dds` (BC1/BC3/BC7, mipmaps) | à figer à l'ajout |
| JSON | `serde` + `serde_json` | — |
| CLI | `clap` (derive) | à figer à l'ajout |

**Assistant Python** (`helper/psd_helper.py`, embarqué dans le binaire et écrit au premier
lancement) : lancé une fois par PSD ouvert, garde le PSD en mémoire, dialogue en JSON par
lignes sur stdin/stdout. Les pixels repassent en RGBA brut (en-tête JSON + octets), pas en
PNG, pour éviter un encodage inutile.

```
→ {"cmd":"open","path":"/…/livery.psd"}
← {"ok":true,"width":4096,"height":4096,"layers":[…]}
→ {"cmd":"render","visible":[0,3,7],"scale":0.25}
← {"ok":true,"width":1024,"height":1024,"bytes":4194304}  + 4194304 octets RGBA
```

psd-tools est installé dans un venv dédié `~/.local/share/ddsnap/venv` par `install.sh`
(Ubuntu interdit `pip install` système, PEP 668).

Distributions cibles : Ubuntu 24.04 LTS et plus récentes. Paquets système requis :
`python3 python3-venv libgtk-4-dev libadwaita-1-dev blueprint-compiler build-essential pkg-config`.

## Commandes

```bash
# Installation (une fois)
sudo apt install python3 python3-venv libgtk-4-dev libadwaita-1-dev blueprint-compiler build-essential pkg-config
./install.sh                               # venv psd-tools + binaire + .desktop

# Dev
cargo run --bin ddsnap-gui                # l'app
cargo run --bin ddsnap -- export x.psd    # la CLI
cargo build --release

# Qualité
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test                                 # unitaires, sans Python
cargo test -- --ignored                    # intégration, nécessite le venv psd-tools

# Release
git tag -a vX.Y.Z -m "vX.Y.Z" && git push --tags
gh release create vX.Y.Z dist/ddsnap-vX.Y.Z-x86_64-linux.tar.gz --notes-file <notes>
```

## Structure du projet

```
Cargo.toml
build.rs                    → Blueprint (.blp → .ui) + GResource
install.sh                  → venv psd-tools, binaires dans ~/.local/bin, .desktop
data/
  ui/*.blp, icons/, *.desktop.in
helper/
  psd_helper.py             → assistant psd-tools (open, tree, render), embarqué via include_str!
  test_psd_helper.py        → tests pytest de l'assistant
src/
  lib.rs                    → expose core/
  core/                     → logique pure, AUCUNE dépendance GTK
    helper.rs               → protocole JSON-lignes avec l'assistant Python
    layers.rs               → arbre de calques, chemins, résolution recette ↔ PSD
    recipe.rs               → lecture/écriture/migration de <nom>.ddsnap.json
    dds.rs                  → choix du format (Auto), redimensionnement, encodage, décodage pour aperçu
    export.rs               → une sortie de bout en bout : rendu → DDS → écriture atomique
    diagnostics.rs          → journal, rapport copiable, doctor
  bin/
    ddsnap.rs               → CLI
    ddsnap-gui.rs           → point d'entrée AdwApplication
  ui/                       → widgets, présentation uniquement
    window.rs, layer_tree.rs, preview.rs, outputs_panel.rs
tests/
  integration.rs            → #[ignore], PSD générés par psd-tools au moment du test
docs/
  decisions/                → ADR (psd-tools en sous-processus, image_dds, recette sidecar)
CHANGELOG.md
SPEC.md
```

Règle d'architecture : `ui/` et `bin/` dépendent de `core/`, jamais l'inverse.

## Style de code

Identique à Transcode : `rustfmt` par défaut, `clippy -D warnings`, identifiants en anglais,
textes d'interface en français, erreurs typées dans `core/` (`thiserror` non requis : enum +
`Display`), pas de `unwrap()` hors tests. Arguments de processus en `Vec<OsString>`, jamais
`sh -c`. Écriture des DDS atomique (fichier temporaire dans le même dossier puis `rename`).

```rust
/// Picks BC1 when every pixel is opaque, BC3 otherwise (Assetto Corsa "Auto" preset).
pub fn auto_format(rgba: &[u8]) -> DdsFormat {
    if rgba.chunks_exact(4).all(|px| px[3] == u8::MAX) {
        DdsFormat::Bc1
    } else {
        DdsFormat::Bc3
    }
}
```

## Stratégie de test

- **Unitaires Rust (`cargo test`)**, sans Python : format Auto, validation des tailles,
  résolution recette ↔ arbre (calque renommé, supprimé, doublon de nom), migration du JSON
  de recette, parsing du protocole assistant sur des réponses enregistrées, en-tête DDS
  produit (dimensions, FourCC, nombre de mipmaps).
- **Assistant Python (`pytest helper/`)** : PSD générés par psd-tools dans le test (groupes,
  calques masqués, mode de fusion, 16 bits) ; `render` respecte la liste de visibilité.
- **Intégration (`cargo test -- --ignored`)** : PSD → DDS de bout en bout, relecture du DDS
  et comparaison au rendu psd-tools (écart moyen sous un seuil par format).
- **Régression** : chaque bug signalé sur un vrai PSD ajoute un cas de test avant la
  correction.
- **UI** : checklist manuelle par tâche ; la logique reste dans `core/`.

## Git et releases

- `git init` dès l'approbation, branche `main`. Un commit par tâche du plan, au format
  Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `chore:`), tests verts avant
  chaque commit.
- SemVer : `v0.x.y` jusqu'à la sortie du MVP, puis `v1.0.0`. `CHANGELOG.md` tenu à jour
  (Keep a Changelog), une section par version.
- Release = tag annoté + archive `ddsnap-vX.Y.Z-x86_64-linux.tar.gz` (binaires, `install.sh`,
  `.desktop`, icône) + notes tirées du CHANGELOG.

## Limites

- **Toujours** : `fmt` + `clippy` + `cargo test` verts avant chaque commit ; garder `core/`
  sans GTK ; ne jamais écrire dans le PSD ; signaler toute perte (16 bits → 8 bits, calque
  de recette introuvable, effet non rendu par psd-tools).
- **Demander d'abord** : ajouter une dépendance au-delà de gtk4, libadwaita,
  glib-build-tools, serde, serde_json, image_dds, clap (et psd-tools côté Python) ; créer le
  dépôt distant ou publier une release ; changer de moteur PSD ou DDS ; ajouter Flatpak ;
  modifier le périmètre du MVP.
- **Jamais** : `sh -c` ; `pip install` hors du venv dédié ; supprimer ou désactiver un test
  pour passer au vert ; laisser un DDS à moitié écrit après une erreur ou une annulation.

## Hors MVP (et pourquoi)

- **Plugin PhotoCraft** — son API de plugins WebAssembly n'est pas documentée et le projet est
  en alpha. La CLI `ddsnap` est la frontière prête pour ça ; on y reviendra quand l'API sera
  stable. PhotoCraft n'écrit pas encore de DDS, donc le besoin reste réel.
- **Autres sims** (iRacing, rFactor 2, ACC) — les préréglages sont des données ; on en
  ajoutera si besoin.
- **Édition des pixels** — on compose et on exporte, on ne peint pas.
- **Surveillance du PSD et réexport automatique** — utile mais pas indispensable ; facile à
  ajouter par-dessus la CLI.
- **Windows / macOS, Flatpak** — Ubuntu d'abord.
- **Normal maps / BC5** — pas utilisées par les skins AC courantes.

## Critères de réussite

1. Un PSD de skin AC 4096×4096 (~50 calques) s'ouvre et affiche l'arbre + l'aperçu en < 5 s.
2. Cocher un calque met l'aperçu à jour en < 2 s (aperçu à l'échelle de l'écran) sans geler
   l'interface.
3. Le DDS exporté s'ouvre dans Assetto Corsa / Content Manager (Showroom) avec les bons
   calques, des mipmaps et le format attendu (vérifié manuellement sur 3 de mes skins).
4. L'« Aperçu compressé » correspond pixel pour pixel au DDS écrit (même décodeur).
5. Réouvrir le PSD restaure toutes les sorties ; `ddsnap export livery.psd` produit des
   fichiers identiques à « Tout exporter » de l'app.
6. Sans psd-tools installé, l'app et `doctor` disent exactement quoi faire ; aucune erreur
   brute Python n'apparaît dans l'interface.
7. Toute erreur offre « Copier le rapport » avec assez d'infos pour reproduire.
8. `v1.0.0` publiée en release avec archive installable par `install.sh`.

## Décisions (2026-10-19)

- Nom : **Ddsnap** (binaires `ddsnap` et `ddsnap-gui`).
- Recette à côté du PSD : `<nom>.ddsnap.json`.
- Dépôt GitHub **privé**.
- Design : GNOME HIG ; passe `/ui-ux-pro-max:design` sur les maquettes à la tâche UI.

## Questions ouvertes

1. **BC7** : ton AC a-t-il Custom Shaders Patch ? Si oui, BC7 pourrait devenir le défaut
   « haute qualité ».
2. **Effets de calque** (ombres, contours) : psd-tools les rend partiellement. Validé par le
   spike de la tâche 1 sur tes vrais PSD.
