# Ddsnap

Calques PSD → textures DDS (BC1/BC3/BC7/RGBA8, mipmaps). CLI `ddsnap` + app GNOME `ddsnap-gui`.
Second mode : DDS → PSD d'un calque (`ddsnap to-psd`, ADR-008).
Spec : SPEC.md · plan : tasks/plan.md, tasks/todo.md · décisions : docs/decisions/ADR-001…008.

## Stack
- Rust 2024 ; `src/core/` sans GTK (lib) ; `src/ui/` inclus seulement dans `ddsnap-gui` (ADR-006).
- Lecture PSD : assistant Python `helper/psd_helper.py` (psd-tools 1.25.0[composite]) dans un
  venv (`scripts/setup-venv.sh`), protocole JSON-lignes (ADR-001). DDS : `image_dds` (ADR-003).
- GTK 4.14+ / libadwaita 1.5+ (Ubuntu 24.04+), UI construite en code, pas de Blueprint.

## Commandes
- `cargo fmt && cargo clippy --all-targets -- -D warnings` (toujours avant un commit)
- `cargo test` · `cargo test -- --ignored` (venv requis) · `~/.local/share/ddsnap/venv/bin/python -m pytest helper/`
- `./install.sh` (installe pour l'utilisateur) · `scripts/release.sh X.Y.Z`
- Contrôle GUI sans écran : `gtk4-broadwayd :5` + `dbus-run-session` + `GDK_BACKEND=broadway ddsnap-gui <psd>`
  (PSD de test : `tests/fixtures/make_psd.py`).

## Conventions
- Identifiants et commentaires en anglais, textes d'interface et messages en français.
- Erreurs typées dans `core` ; jamais de `sh -c` ; écriture atomique (`export::write_atomic`).
- Un commit par tâche, pousser sur `main` (dépôt privé `fdeslongchamps/ddsnap`).
- Raccourcis assumés : commentaire `shortcut: <limite>, <quand y revenir>`.

## Limites
- Jamais écrire dans le PSD source (DDS → PSD crée un nouveau fichier, jamais par-dessus) ; jamais laisser un DDS à moitié écrit ; jamais `pip install` hors du venv.
- Demander avant : nouvelle dépendance, changer le moteur PSD/DDS, Flatpak.
- Les PSD d'exemple (`samples/`, `*.psd`) restent hors de git.

## Pièges
- La GUI garde son `Rc<Window>` jusqu'à `close-request` ; les handlers tiennent des `Weak` (ADR-006).
- `AdwApplicationWindow` possède déjà `win.` : `window.add_action`, pas de second groupe.
- Les index de calques = ordre psd-tools (pré-ordre, bas → haut) ; résoudre la recette par
  `layer_id` puis chemin, jamais par position (ADR-004).
