# Spec : Ddsnap — PSD → DDS pour skins Assetto Corsa, GUI GNOME

Statut : **approuvée** · 2026-10-19 · révision « sync ADR-001…004 » **en attente d'approbation**
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
4. Je coche/décoche des calques → l'aperçu se recalcule (le dernier clic gagne : les états
   intermédiaires ne sont pas rendus ; indicateur de calcul ; l'interface ne gèle jamais).
5. Je choisis le format (préréglages AC ci-dessous) et la taille. Le bouton **« Aperçu
   compressé »** montre l'image décodée depuis le DDS réellement produit, pour voir les
   artefacts BC1/BC3 avant d'enregistrer. Bascule rapide Avant/Après.
6. **« Exporter »** (Ctrl+E) écrit la sortie active ; **« Tout exporter »** (Ctrl+Maj+E)
   écrit toutes les sorties. **Dossier de sortie** : bouton « Dossier de sortie… » (sélecteur
   de dossier GNOME) au-dessus de la liste des sorties, mémorisé dans la recette ; sans choix,
   celui du PSD (ex. PSD dans `Documents`, DDS directement dans `…/skins/<skin>/`). Si le fichier existe,
   confirmation « Remplacer » (c'est le cas normal : on réexporte la skin), jamais le PSD.
   Pendant tout calcul (ouverture, aperçu, export), une **barre de progression** affiche
   l'étape en cours et le temps écoulé (« Rendu de 36 calques (psd-tools)… 42 s »,
   « Encodage BC3 + mipmaps… ») ; l'app ne doit jamais avoir l'air figée.
7. Fin → toast « livery.dds — 4096×4096 · BC3 · 21,3 Mo » avec « Afficher dans le dossier ».
   Erreur → message lisible + détails techniques dans un expander + bouton « Copier le
   rapport » (voir Diagnostic).

### Recette par PSD

Les sorties d'un PSD sont mémorisées dans un fichier **à côté du PSD** : `<nom>.ddsnap.json`
(lisible, versionnable avec la skin, réutilisé par la CLI). Les calques y sont identifiés par
leur **identifiant PSD** (`layer_id`, stable même si on renomme ou déplace le calque), puis
par leur chemin (`Groupe/Sous-groupe/Calque`) si l'identifiant a disparu (calque recréé ou
copié depuis un autre PSD). **Jamais par position** : ajouter un calque décale toutes les
positions. Rien n'est ignoré en silence ; sont signalés (avertissement CLI, bandeau GUI) :

- un calque retrouvé **par chemin seulement** alors que la recette avait un `layer_id` (recréé) ;
- un calque **introuvable**, ou dont le chemin désigne désormais **plusieurs** calques
  (deux calques de même nom, sans id) ;
- un calque coché **sans son groupe** : un calque n'est dessiné que si lui et **tous ses
  groupes parents** sont cochés (« calque « Numéro/42 » non dessiné : son groupe « Numéro »
  n'est pas coché »).

Un PSD sans aucun `layer_id` (fichiers écrits par certains outils) est résolu par chemin sans
avertissement. Dans un chemin, `/` et `\` à l'intérieur d'un nom s'écrivent `\/` et `\\`.

La recette est lue puis **entièrement validée avant le premier rendu** (une recette invalide
n'écrit rien) :

- `version` vérifiée en premier : une version inconnue donne « version de recette inconnue »,
  jamais une erreur de champ ;
- nom de fichier de chaque sortie validé **dans `core`** (`validate_output_name`), à la lecture
  comme à la saisie dans l'app : un simple nom sans `/` ni `\`, extension `.dds` (casse
  indifférente), différent du PSD et des autres sorties. Sans séparateur, `..` ne peut viser
  aucun autre dossier : `livery..v2.dds` est accepté. Une recette éditée à la main ne peut
  donc jamais écrire ailleurs que dans le dossier du PSD ni écraser le PSD ;
- `format`, `size`, `mipmaps` sont facultatifs (défauts : `auto`, `source`, `true`) ;
- `output_dir` (facultatif, commun à toutes les sorties) : dossier des DDS, absolu ou relatif
  au dossier du PSD ; absent = dossier du PSD. Il doit exister au moment de l'export, sinon
  erreur avant tout rendu (« dossier de sortie introuvable »), rien n'est écrit.

Sans recette, la recette par défaut (une sortie `<nom>.dds`, Auto, taille source, mipmaps,
calques visibles du PSD) est construite en mémoire ; la CLI ne l'écrit pas, la GUI l'écrit à
la première modification.

```json
{
  "version": 1,
  "output_dir": "/home/moi/.steam/…/assettocorsa/content/cars/ks_porsche_911_gt3_r/skins/42",
  "outputs": [
    {
      "file": "livery.dds",
      "format": "bc3",
      "size": "source",
      "mipmaps": true,
      "visible": [
        { "id": 12, "path": "Base" },
        { "id": 31, "path": "Sponsors" },
        { "id": 47, "path": "Numéro/42" }
      ]
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

- En-têtes DDS (ADR-003) : **DX9 classiques** pour BC1 (`DXT1`), BC3 (`DXT5`) et RGBA8
  (`A8R8G8B8`, octets BGRA), lisibles par tous les outils ; BC7 n'a pas de code DX9 → en-tête
  **DX10** (`BC7_UNORM`). Qualité d'encodage `Normal`.
- Mipmaps générés par défaut jusqu'à 1×1, désactivables. Filtre **boîte 2×2** (celui
  d'`image_dds`), pas Lanczos/Kaiser : écart assumé (ADR-003), à revoir si des moirés
  apparaissent au loin en jeu.
- Taille : « source » ou réduction 4096 / 2048 / 1024 du **plus grand côté** (Lanczos3,
  ratio conservé, jamais d'agrandissement). Si une dimension n'est pas une puissance de 2, ou
  pas multiple de 4 pour BC1/BC3/BC7 → avertissement seulement (l'encodeur complète les blocs
  4×4), jamais de refus.
- Les PSD 16 bits et CMYK sont convertis en RGBA 8 bits ; la conversion est indiquée dans
  l'interface.

### CLI (même cœur, sans GTK)

```bash
ddsnap export livery.psd                  # toutes les sorties de livery.ddsnap.json
                                          #   (sans recette : <nom>.dds, calques visibles du PSD)
ddsnap export livery.psd --output livery  # une seule sortie (« livery » ou « livery.dds »)
ddsnap export livery.psd --out-dir ~/ac/skins/42   # dossier de sortie pour cet appel
ddsnap export livery.psd --format bc3 --size 2048 --no-mipmaps
                                          # remplace les réglages de la recette pour cet appel
ddsnap layers livery.psd                  # arbre des calques (texte ou --json)
ddsnap doctor                             # vérifie python3, psd-tools, versions, chemins
```

Avertissements sur stderr (`avertissement : livery.dds : …`), une ligne par sortie écrite sur
stdout (`livery.dds — 4096×4096 · BC3 · 21,3 Mo`), code de sortie non nul en cas d'erreur.

La CLI est la frontière d'intégration : un script, un `Makefile`, ou plus tard PhotoCraft
l'appellent sans dépendre de l'interface graphique.

### Diagnostic (« pouvoir réparer mes problèmes »)

- Journal rotatif dans `~/.local/state/ddsnap/ddsnap.log` (opérations, durées, erreurs,
  stderr Python complet). `DDSNAP_LOG=debug` pour plus de détail.
- **Panneau « Journal »** dans l'app (repliable, en bas de la fenêtre) : les mêmes lignes que
  le journal, en direct (étape, durée, avertissements, stderr de l'assistant), pour voir ce
  que l'app fait pendant un long rendu.
- La CLI affiche chaque étape sur stderr avec le temps écoulé (`[  42.0 s] livery.dds :
  rendu de 36 calques…`) et un rappel toutes les 15 s pendant une étape longue.
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
| Async | `core` **synchrone** (`std::process`, bloquant) ; la GUI l'appelle depuis un thread de travail et revient sur la boucle GLib (ADR-002) ; pas de tokio | — |
| Lecture PSD + composition | **psd-tools** (Python) dans un processus assistant long-vivant | python ≥ 3.12 (testé 3.14), psd-tools **1.25.0** (figé dans `scripts/setup-venv.sh`) |
| Encodage DDS | crate `image_dds` (BC1/BC3/BC7 via intel_tex_2, décodage bcdec) ; `build.rs` lie libstdc++ pour intel_tex_2 | 0.7.2 |
| JSON | `serde` + `serde_json` | — |
| CLI | `clap` (derive) | 4.6.7 |

**Assistant Python** (`helper/psd_helper.py`, embarqué dans le binaire via `include_str!` et
lancé par `python -I -c <script>` : aucun fichier écrit, donc pas de course entre la GUI et
la CLI) : lancé une fois par PSD ouvert, garde le PSD en mémoire, dialogue en JSON par
lignes sur stdin/stdout. Les pixels repassent en RGBA brut (en-tête JSON + octets), pas en
PNG, pour éviter un encodage inutile.

- **stdout réservé au protocole** : au démarrage, l'assistant garde le vrai stdout pour lui et
  redirige `sys.stdout` vers stderr. Un `print` ou un avertissement de psd-tools ou d'une
  dépendance ne peut pas corrompre le flux d'octets.
- **stderr lu en continu** par un thread côté Rust (une sortie bavarde ne bloque jamais le
  tube) ; les 16 derniers Ko servent aux messages d'erreur et au rapport.
- **Pas de timeout** : un gros PSD est légitimement lent ; un assistant mort est détecté par la
  fin de son stdout. Erreurs typées côté Rust : `PythonMissing` (python du venv absent),
  `PsdToolsMissing` (`No module named 'psd_tools'`), `HelperCrashed(stderr)`, `Psd(message)`.
- **Un rendu à la fois, le dernier gagne** : côté Rust, au plus un `render` en cours et une
  seule demande en attente ; une nouvelle demande remplace celle en attente. Cliquer vite
  ne crée pas de file de rendus périmés.

```
→ {"cmd":"open","path":"/…/livery.psd"}
← {"ok":true,"width":4096,"height":4096,"depth":8,"color_mode":"RGB","layers":[
     {"index":0,"id":12,"name":"Base","path":["Base"],"parent":null,
      "kind":"pixel","visible":true,"has_effects":false}, …]}
→ {"cmd":"render","visible":[0,3,7],"scale":0.25}
← {"ok":true,"width":1024,"height":1024,"bytes":4194304}  + 4194304 octets RGBA
→ {"cmd":"quit"}
← {"ok":false,"kind":"protocol|not_found|invalid_psd|internal","error":"…"}   # en cas d'erreur
```

`render.visible` liste des **index** (ordre de `descendants()`, donnés par `open`), pas des
`layer_id` : les PSD écrits par certains outils n'en ont aucun (ADR-001). La recette garde
`layer_id` puis chemin ; `core::recipe` traduit vers les index. Une erreur ne termine jamais
l'assistant.

psd-tools est installé dans un venv dédié (`$DDSNAP_VENV`, sinon
`$XDG_DATA_HOME/ddsnap/venv`, sinon `~/.local/share/ddsnap/venv`) par
`scripts/setup-venv.sh`, **seul endroit** où sa version est figée ; `install.sh` l'appelle
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
build.rs                    → lien libstdc++ (intel_tex_2) ; plus tard Blueprint (.blp → .ui) + GResource
install.sh                  → appelle scripts/setup-venv.sh, binaires dans ~/.local/bin, .desktop
scripts/
  setup-venv.sh             → venv psd-tools (version figée ici seulement)
  release.sh                → vérifs, archive, tag, gh release
data/
  ui/*.blp, icons/, *.desktop.in
helper/
  psd_helper.py             → assistant psd-tools (open, render), embarqué via include_str!, lancé par python -I -c
  test_psd_helper.py        → tests pytest de l'assistant
  bench_psd_helper.py       → banc : temps open/render, écart avec le composite Photoshop
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
  fixtures/                 → réponses JSON enregistrées de l'assistant, make_psd.py
docs/
  decisions/                → ADR-001 assistant psd-tools, ADR-002 core synchrone,
                              ADR-003 image_dds, ADR-004 recette sidecar
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
  résolution recette ↔ arbre (calque renommé, déplacé, recréé, supprimé, doublon de nom,
  calque inséré au-dessus), validation des noms de sortie (`../x.dds`, `livery.psd`, `a/b.dds`,
  doublon), file de rendu « dernier gagne », migration du JSON de recette, parsing du protocole assistant sur des réponses enregistrées, en-tête DDS
  produit (dimensions, FourCC, nombre de mipmaps).
- **Assistant Python (`pytest helper/`)** : PSD générés par psd-tools dans le test (groupes,
  calques masqués, mode de fusion, 16 bits) ; `render` respecte la liste de visibilité ; un
  `print` parasite pendant un rendu ne corrompt pas la réponse.
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

- **Aperçu en direct fusionné sur le GPU via GTK** (calques extraits une fois par psd-tools) ;
  option « Sans aperçu » ; l'« Aperçu compressé » et l'export restent exacts (ADR-005).
- **Export « tel quel »** par l'image aplatie enregistrée par Photoshop quand la visibilité de
  la sortie est celle du PSD ; psd-tools sinon (ADR-005).
- Releases GitHub publiées à chaque jalon sans redemander (dépôt privé).

- Nom : **Ddsnap** (binaires `ddsnap` et `ddsnap-gui`).
- Recette à côté du PSD : `<nom>.ddsnap.json`.
- Dépôt GitHub **privé**.
- Design : GNOME HIG ; passe `/ui-ux-pro-max:design` sur les maquettes à la tâche UI.

## Questions ouvertes

1. **BC7** : ton AC a-t-il Custom Shaders Patch ? Si oui, BC7 pourrait devenir le défaut
   « haute qualité ».
2. **Effets de calque** (ombres, contours) : psd-tools les rend partiellement. À valider
   sur tes vrais PSD (chemins à fournir, ADR-001).
3. **Aperçu en direct — critères 1 et 2 menacés.** Mesure T1 (PSD synthétique 4096²,
   55 calques) : `open` 0,43 s mais `render` **57 s**, à 100 % comme à 25 % (psd-tools compose
   en pleine taille, ≈ 1 s par calque). L'export (CLI) reste acceptable ; l'aperçu < 2 s ne
   l'est pas tel quel. À trancher avant T7, idéalement sur tes vrais PSD : compositeur
   d'aperçu maison à l'échelle de l'écran (psd-tools reste la référence à l'export), cache par
   groupe, ou rendu à la demande sans direct.
