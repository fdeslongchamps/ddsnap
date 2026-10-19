# ADR-007 — Présentation générique : calques PSD → textures DDS

Statut : accepté par l'utilisateur · 2026-10-19

## Contexte

Ddsnap est né pour les livrées Assetto Corsa (CAM), mais rien dans le moteur n'est propre à
ce jeu : BC1/BC3/BC7/RGBA8 et les mipmaps servent à beaucoup de jeux, de mods et d'outils 3D.
L'utilisateur : « ça peut être utilisé pour autre chose qu'Assetto Corsa ».

## Décision

- Textes visibles génériques : « Choisir des calques PSD et les exporter en textures DDS »
  (entrée de menu, À propos, aide CLI, `Cargo.toml`). Assetto Corsa reste l'origine et
  l'exemple du README.
- Icône générique, sans voiture ni jeu : des calques empilés qui se plaquent en une tuile de
  texture. Forme proposée par Gemini (`gemini-3.8-flash`, variante 1), colorisée avec la
  palette GNOME ; version symbolique 16 px dessinée à la main. Pistes écartées : voiture en
  calques (trop AC), damier de course, mipmaps seules ou blocs 4×4 (moins lisibles en petit).

## Conséquences

- Les préréglages restent pensés pour Assetto Corsa (Auto = BC1 si opaque, sinon BC3) ; ce
  sont aussi les choix courants ailleurs. Aucun préréglage par jeu n'est ajouté.
- SPEC.md garde l'utilisateur AC comme persona principal.
