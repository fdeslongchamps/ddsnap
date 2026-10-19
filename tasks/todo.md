# Tâches — Ddsnap

Plan : [plan.md](plan.md) · Spec : [SPEC.md](../SPEC.md)
Chaque tâche = un commit. Avant chaque commit : `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test` (et `pytest helper/` si l'assistant change).

---

## Phase 1 — Moteur et CLI

### T1 — Spike : assistant psd-tools + mesures sur vrais PSD  ·  dépend : —  ·  M

Valide le risque n°1 avant tout le reste. Écrit ADR-001.

- `scripts/setup-venv.sh` : crée `~/.local/share/ddsnap/venv`, installe `psd-tools` (version figée).
- `helper/psd_helper.py` : boucle JSON-lignes sur stdin/stdout, commandes `open` (arbre avec
  `layer_id`, chemin, type, visibilité), `render` (`visible: [layer_id…]`, `scale`), `quit`.
  Réponse d'erreur `{"ok":false,"error":…,"kind":…}`. Première chose au démarrage : garder le
  vrai stdout (binaire) pour le protocole et rediriger `sys.stdout` vers stderr.
- `helper/test_psd_helper.py` : PSD générés dans le test (groupe, calque masqué, mode de fusion).
- Mesures sur les vrais PSD de l'utilisateur : temps `open`, `render` plein format et à 25 %,
  écart avec l'image composite enregistrée dans le PSD (effets de calque).

Critères :
- [x] `pytest helper/` vert ; `render` respecte la liste de visibilité (pixel attendu par calque).
- [x] Erreur sur fichier non-PSD → réponse `ok:false`, le processus reste vivant.
- [x] Test : un `print("parasite")` pendant un `render` → la réponse et ses octets restent intacts.
- [x] Le script tourne via `python -I -c "$(cat helper/psd_helper.py)"` (pas de dépendance à son emplacement).
- [~] Mesures notées dans `docs/decisions/ADR-001-psd-tools-helper.md` ; décision go / ajustement.
  Synthétique fait (57 s/rendu 4096² → go export, aperçu à revoir avant T7) ; **vrais PSD en attente**.

Fichiers : `scripts/setup-venv.sh`, `helper/psd_helper.py`, `helper/test_psd_helper.py`,
`docs/decisions/ADR-001-psd-tools-helper.md`

### T2 — Squelette Cargo + client de l'assistant + `ddsnap layers`  ·  dépend : T1  ·  M

Écrit ADR-002.

- `Cargo.toml` (édition 2024, `serde`, `serde_json`, `clap` figés), `src/lib.rs`, `.gitignore`.
- `core::helper` : lance l'assistant par `<venv>/bin/python -I -c <script embarqué via include_str!>`
  (aucun fichier écrit), envoie/reçoit, lit les octets RGBA, erreurs typées (`PythonMissing`,
  `PsdToolsMissing`, `HelperCrashed`, `Psd(String)`). Pas de timeout sur `open` (gros PSD
  légitimement lents) ; un assistant mort est détecté par la fin du flux.
- `core::layers` : `LayerTree` (`layer_id`, chemin `Groupe/Calque`, type, visibilité d'origine).
- `src/bin/ddsnap.rs` : `ddsnap layers <psd> [--json]`.

Critères :
- [x] Tests unitaires sur des réponses JSON enregistrées (`tests/fixtures/`) : arbre, chemins
  avec `/` dans un nom (échappé), erreur de l'assistant.
- [x] `#[ignore]` : `ddsnap layers` sur un PSD généré affiche l'arbre attendu.
- [x] Sans venv → message « psd-tools introuvable : lancez scripts/setup-venv.sh ».

Fichiers : `Cargo.toml`, `src/lib.rs`, `src/core/{mod,helper,layers}.rs`, `src/bin/ddsnap.rs`,
`tests/fixtures/*.json`

### T3 — Encodage DDS + `ddsnap export` (visibilité du PSD)  ·  dépend : T2  ·  M

Écrit ADR-003. Ajoute la dépendance `image_dds` (approuvée par la spec).

- `core::dds` : `auto_format`, validation des tailles (multiple de 4, puissance de 2 → avertissement),
  redimensionnement 4096/2048/1024, encodage BC1/BC3/BC7/RGBA8 avec mipmaps, décodage.
- `core::export` : rendu plein format → DDS → écriture atomique (temporaire + `rename`).
- CLI : `ddsnap export <psd> [--format auto|bc1|bc3|bc7|rgba8] [--size …] [--no-mipmaps] [-o fichier]`.

Critères :
- [x] Tests : `auto_format`, tailles invalides, en-tête DDS (dimensions, FourCC/DXGI, nombre de mips).
- [x] `#[ignore]` : PSD généré → DDS → décodé, écart moyen avec le rendu sous un seuil par format.
- [x] Erreur au milieu de l'écriture → aucun fichier `.dds` partiel ne reste.
- [ ] Manuel : un DDS d'un vrai PSD s'affiche correctement dans le Showroom de Content Manager.

Fichiers : `Cargo.toml`, `src/core/{dds,export}.rs`, `src/bin/ddsnap.rs`,
`docs/decisions/ADR-003-image-dds.md`

### T4 — Recette `<nom>.ddsnap.json` + sorties multiples en CLI  ·  dépend : T3  ·  M

Écrit ADR-004.

- `core::recipe` : lecture/écriture (`version: 1`), recette par défaut = visibilité du PSD,
  résolution recette ↔ arbre par `layer_id` puis chemin (jamais par position), rapport des
  calques retrouvés par chemin seulement ou introuvables.
- Validation des noms de sortie, à la lecture : simple nom sans `/` ni `..`, extension `.dds`,
  différent du PSD, unique dans la recette. Même fonction utilisée par la GUI (T8).
- CLI : `ddsnap export <psd>` exporte toutes les sorties ; `--output <nom>` une seule.

Critères :
- [x] Tests : aller-retour JSON ; calque renommé ou déplacé (retrouvé par `layer_id`, sans
  avertissement) ; calque inséré au-dessus (aucun autre calque ne change) ; calque recréé
  (retrouvé par chemin + avertissement) ; calque supprimé (signalé) ; deux calques de même
  nom ; version inconnue → erreur claire.
- [x] Tests : `livery.psd`, `../x.dds`, `a/b.dds`, `x.png`, nom en double → recette refusée,
  aucun fichier écrit.
- [x] `#[ignore]` : recette à 2 sorties → 2 DDS différents attendus.

Fichiers : `src/core/recipe.rs`, `src/core/export.rs`, `src/bin/ddsnap.rs`,
`docs/decisions/ADR-004-recipe-sidecar.md`

### Checkpoint A — moteur validé
- [ ] `cargo test`, `cargo test -- --ignored`, `pytest helper/` verts
- [ ] 3 DDS issus de vrais PSD vérifiés dans Content Manager
- [ ] Revue avec l'utilisateur avant la GUI

### T5 — Dépôt GitHub privé + CHANGELOG + release `v0.1.0`  ·  dépend : checkpoint A  ·  S

- `gh repo create ddsnap --private --source . --push` (dépôt privé approuvé le 2026-10-19).
- `CHANGELOG.md` (Keep a Changelog), `scripts/release.sh <version>` : vérifs qualité, build
  release, archive `dist/ddsnap-vX.Y.Z-x86_64-linux.tar.gz`, tag annoté, `gh release create`
  avec la section du CHANGELOG comme notes.

Critères :
- [ ] Le script refuse de publier si l'arbre git est sale ou si un test échoue.
- [ ] Release `v0.1.0` visible sur GitHub avec l'archive ; l'archive extraite fait tourner `ddsnap layers`.

Fichiers : `CHANGELOG.md`, `scripts/release.sh`, `README.md`

---

## Phase 2 — GUI

### T6 — GUI : accueil, ouverture du PSD, arbre des calques  ·  dépend : T2  ·  M

Passe `/ui-ux-pro-max:design` sur les maquettes (dans le cadre des GNOME HIG) au début de la tâche.

- Ajoute gtk4, libadwaita, glib-build-tools (approuvés). `build.rs` Blueprint → GResource.
- `src/bin/ddsnap-gui.rs` (`AdwApplication` `io.github.fdeslongchamps.Ddsnap`), `ui/window.rs`,
  page d'accueil (glisser-déposer + Ctrl+O + PSD passé en argument), `ui/layer_tree.rs` en
  lecture seule (nom, type, « masqué dans le PSD »).
- L'assistant tourne dans un thread de travail ; l'UI ne bloque jamais.

Critères :
- [ ] Test de fumée sous `gtk4-broadwayd` : la fenêtre s'ouvre sans critical GTK.
- [ ] Manuel : un vrai PSD s'ouvre et l'arbre s'affiche ; fichier non-PSD → dialogue d'erreur clair.

Fichiers : `Cargo.toml`, `build.rs`, `data/ui/{window,welcome}.blp`, `data/resources.gresource.xml`,
`src/bin/ddsnap-gui.rs`, `src/ui/{mod,window,layer_tree}.rs`

### T7 — Aperçu en direct  ·  dépend : T6  ·  M

- `ui/preview.rs` : rendu à l'échelle de la fenêtre, zoom (ajusté / 100 % / molette), déplacement,
  RVB / Alpha / RVB+A sur damier.
- Cases à cocher dans l'arbre → `core::render_queue` : au plus un rendu en cours et une seule
  demande en attente, une nouvelle demande remplace celle en attente (le dernier clic gagne) ;
  indicateur de calcul.

Critères :
- [ ] Test unitaire (`core`, sans GTK) : 5 demandes pendant un rendu en cours → un seul rendu
  de plus, avec la dernière demande.
- [ ] Manuel : critère de réussite 2 (< 2 s, pas de gel) sur un vrai PSD 4096².

Fichiers : `src/ui/{preview,layer_tree}.rs`, `src/core/render_queue.rs`, `data/ui/window.blp`

### T8 — Panneau Sorties lié à la recette  ·  dépend : T4, T7  ·  M

- `ui/outputs_panel.rs` : liste des sorties, ajouter / dupliquer / renommer / supprimer ;
  format, taille, mipmaps ; changer de sortie restaure ses calques cochés.
- Sauvegarde de la recette à chaque modification (écriture atomique) ; bandeau
  « N calques de la recette introuvables » si besoin.

Critères :
- [ ] Manuel : fermer / rouvrir le PSD restaure toutes les sorties (critère 5, partie GUI).
- [ ] Nom de fichier de sortie invalide ou en double → refusé avec explication (validation de `core::recipe`, pas de copie dans l'UI).

Fichiers : `src/ui/outputs_panel.rs`, `data/ui/outputs_panel.blp`, `src/ui/window.rs`

### T9 — Exporter / Tout exporter + Aperçu compressé  ·  dépend : T3, T8  ·  M

- Ctrl+E / Ctrl+Maj+E, confirmation « Remplacer » si le DDS existe, toast résultat +
  « Afficher dans le dossier ».
- « Aperçu compressé » : encode → décode avec `core::dds`, bascule Avant/Après.

Critères :
- [ ] `#[ignore]` : « Tout exporter » (via `core::export`) == `ddsnap export` octet pour octet (critère 5).
- [ ] Test : l'image de l'aperçu compressé == décodage du fichier écrit (critère 4).
- [ ] Manuel : parcours 1→7 de la spec.

Fichiers : `src/ui/{window,preview}.rs`, `src/core/export.rs`, `data/ui/window.blp`

### Checkpoint B — parcours complet
- [ ] Parcours de la spec fait à la main sur un vrai PSD ; critères 1, 2, 4, 5 vérifiés
- [ ] Release `v0.2.0` (`scripts/release.sh 0.2.0`)

---

## Phase 3 — Robustesse et livraison

### T10 — Diagnostic  ·  dépend : T9  ·  M

- `core::diagnostics` : journal `~/.local/state/ddsnap/ddsnap.log` (rotation maison à 5 Mo × 3,
  sans nouvelle dépendance), `DDSNAP_LOG=debug`, rapport (versions app/python/psd-tools, OS,
  dernières lignes du journal), `doctor`.
- GUI : expander « Détails » + « Copier le rapport » sur chaque erreur, Aide › Diagnostic.
- CLI : `ddsnap doctor`.

Critères :
- [ ] Tests : rotation, contenu du rapport, `doctor` avec python/psd-tools absents (simulés).
- [ ] Manuel : venv supprimé → l'app et `doctor` disent quoi faire, aucune trace Python brute (critère 6).

Fichiers : `src/core/diagnostics.rs`, `src/bin/ddsnap.rs`, `src/ui/{window,error_dialog}.rs`

### T11 — Cas limites signalés  ·  dépend : T10  ·  M

- PSD 16 bits / CMYK → conversion indiquée ; taille non multiple de 4 → avertissement ;
  assistant planté → relancé une fois puis erreur avec rapport ; effets de calque non rendus →
  avertissement par calque (selon résultat T1).

Critères :
- [ ] Un test par cas (PSD générés dans `pytest` et `#[ignore]`).
- [ ] Aucun échec silencieux : chaque perte apparaît dans l'UI et la sortie CLI.

Fichiers : `helper/psd_helper.py`, `helper/test_psd_helper.py`, `src/core/{helper,export}.rs`, `src/ui/window.rs`

### T12 — `install.sh`, `.desktop`, icône, `v1.0.0`  ·  dépend : T11  ·  S

- `install.sh` : appelle `scripts/setup-venv.sh` (aucune version de psd-tools ici), binaires dans
  `~/.local/bin`, `.desktop` + icône, désinstallation.
- `README.md` (installation, CLI, PhotoCraft via CLI), passage de version `1.0.0`.

Critères :
- [ ] Depuis l'archive de release sur une session propre : `./install.sh` puis l'app apparaît
  dans le menu GNOME et ouvre un PSD.
- [ ] Les 8 critères de réussite de la spec cochés ; `scripts/release.sh 1.0.0` publie `v1.0.0`.

Fichiers : `install.sh`, `data/io.github.fdeslongchamps.Ddsnap.desktop.in`, `data/icons/*`, `README.md`, `CHANGELOG.md`

### Checkpoint C — terminé
- [ ] Critères 1 à 8 de la spec vérifiés
- [ ] `v1.0.0` publiée
