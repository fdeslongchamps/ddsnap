# ADR-008 — Mode DDS → PSD pour retoucher un DDS dans un éditeur sans DDS

Statut : accepté par l'utilisateur · 2026-10-20

## Contexte

L'utilisateur retouche ses textures dans PhotoCraft et ne veut pas le modifier. Pour partir
d'un DDS existant (celui d'un jeu ou d'un mod), il faut d'abord le convertir dans un format
que l'éditeur ouvre : PhotoCraft ouvre PNG, PSD et TIFF.

## Décision

- Ddsnap gagne un second mode : un DDS ouvert (Ctrl+O, glisser-déposer, `ddsnap-gui x.dds`,
  `ddsnap to-psd x.dds`) devient un PSD d'un seul calque « Texture ».
- Le niveau pleine taille est décodé par `core::dds::decode` (le même décodeur que
  l'aperçu compressé) ; les mipmaps sont abandonnées, l'export les recalcule.
- Le PSD est écrit par l'assistant psd-tools (commande `to_psd`, image envoyée après la ligne
  de requête, octets du PSD renvoyés) puis par `export::write_atomic` côté Rust. L'assistant
  n'écrit aucun fichier lui-même.
- Nom par défaut : `<nom>.psd`, ou `<nom>-dds.psd` si `<nom>.psd` existe (sans doute le PSD
  source). La CLI refuse de remplacer un fichier sans `--force` ; l'app passe par une boîte
  « Enregistrer » qui demande avant de remplacer, et refuse le PSD ouvert.
- Choix de l'utilisateur : PSD plutôt que PNG, parce que le PSD ne demande aucune nouvelle
  dépendance (psd-tools et Pillow sont déjà dans le venv) et retombe dans le flux habituel :
  PhotoCraft enregistre le PSD, Ddsnap l'exporte en DDS.

## Conséquences

- L'image aplatie stockée dans le PSD garde l'alpha grâce à `PSDImage._merged_alpha`, un
  attribut privé de psd-tools 1.25.0 (version figée) : à revérifier en cas de mise à jour.
  Dans cette image aplatie, les pixels semi-transparents peuvent s'écarter de ±1 (arrondi de
  psd-tools) ; le calque, lui, est exact.
- Aller-retour DDS RGBA8 → PSD → DDS RGBA8 identique à l'octet près sur le PSD de test.
- Un DDS en BC1/BC3/BC7 a déjà perdu de la qualité à la compression ; le PSD contient cette
  image dégradée, pas l'original.
- Non vérifié : que PhotoCraft ouvre ce PSD (à tester par l'utilisateur).
