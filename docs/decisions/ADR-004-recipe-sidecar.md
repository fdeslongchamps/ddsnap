# ADR-004 — Recette `<nom>.ddsnap.json` à côté du PSD

Statut : accepté · 2026-10-19

## Décision

- `livery.psd` → `livery.ddsnap.json` dans le même dossier ; JSON lisible, `version: 1`.
  Version inconnue → erreur claire, pas de lecture approximative.
- Pas de recette → recette par défaut en mémoire (une sortie `<nom>.dds`, format Auto,
  taille source, mipmaps, calques visibles du PSD). La CLI ne l'écrit pas ; la GUI (T8) la
  sauvegardera à la première modification.
- Champs `format`, `size`, `mipmaps` facultatifs (défauts : auto, source, true).
- `output_dir` facultatif, commun à la recette : absolu ou relatif au dossier du PSD, absent =
  dossier du PSD. Ajout compatible (reste `version: 1`). Vérifié avant le premier rendu.
  Le nom de fichier reste validé (simple nom `.dds`), donc le PSD ne peut pas être écrasé ;
  choisir un dossier qui contient déjà le DDS de la skin est le cas normal (remplacement).
- **Calques** : `{id, path}`. Résolution : `layer_id` d'abord ; sinon chemin échappé
  (`Groupe/Calque`, `\/` et `\\` dans les noms) s'il désigne **un seul** calque.
  Jamais la position.
  - retrouvé par chemin alors que la recette avait un id → avertissement (calque recréé) ;
  - chemin absent ou ambigu (deux calques de même nom, sans id) → « introuvable » ;
  - PSD sans aucun `layer_id` (fichiers générés par certains outils) → résolution par chemin
    sans avertissement, sinon chaque export avertirait.
- **Noms de sortie** validés dans `core::recipe::validate_output_name` (partagé CLI/GUI) :
  pas de `/`, `\` ni `..`, extension `.dds`, différent du PSD, unique. Toute la recette est
  validée **avant** le premier rendu : une recette invalide n'écrit rien.
- CLI : `ddsnap export x.psd` exporte toutes les sorties ; `--output livery` (ou
  `livery.dds`) une seule ; `--format`, `--size`, `--no-mipmaps` remplacent les réglages de
  la recette pour cet appel (scripts).

## Conséquences

- Une recette éditée à la main ne peut écrire que dans **son** dossier de sortie, et jamais
  sur le PSD (extension `.dds` imposée).
- Les sorties sont rendues l'une après l'autre (un rendu psd-tools par sortie, ≈ 1 min en
  4096² d'après ADR-001).
