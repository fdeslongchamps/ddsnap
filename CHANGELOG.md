# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) · versions : [SemVer](https://semver.org/lang/fr/).

## [Unreleased]

### Ajouté
- Application GNOME `ddsnap-gui` (GTK 4 + libadwaita) : glisser-déposer / Ctrl+O, arbre des
  calques à cocher, sorties (ajouter, dupliquer, renommer, supprimer), format, taille,
  mipmaps, dossier de sortie, recette enregistrée à chaque changement.
- Aperçu sur la carte graphique : chaque calque extrait une fois, fusion GSK (modes de fusion,
  opacité de groupe, masques d'écrêtage), vues RVB+A / RVB / Alpha, « Aperçu compressé »,
  option « Sans aperçu ».
- Export / Tout exporter avec confirmation « Remplacer ? », toast « Afficher dans le dossier »,
  barre de progression et journal en direct.

## [0.1.0] - 2026-10-19

Première version : moteur et CLI, sans interface graphique.

### Ajouté
- `ddsnap layers <psd> [--json]` : arbre des calques (groupes, masqués, effets).
- `ddsnap export <psd>` : DDS BC1 / BC3 / BC7 / RGBA8 (Auto = BC1 si opaque, sinon BC3),
  mipmaps, réduction 4096 / 2048 / 1024, écriture atomique.
- Recette `<nom>.ddsnap.json` : plusieurs sorties par PSD, calques retrouvés par `layer_id`
  puis chemin, noms de sortie validés, dossier de sortie (`output_dir`, `--out-dir`).
- Export « tel quel » depuis l'image aplatie enregistrée par Photoshop (≈ 10× plus rapide).
- Progression affichée étape par étape avec le temps écoulé.
- Assistant psd-tools (`scripts/setup-venv.sh`), `scripts/try-psd.sh` pour tester un vrai PSD.

[Unreleased]: https://github.com/fdeslongchamps/ddsnap/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/fdeslongchamps/ddsnap/releases/tag/v0.1.0
