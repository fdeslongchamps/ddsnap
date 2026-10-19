# ADR-003 — `image_dds` pour l'encodage et le décodage

Statut : accepté · 2026-10-19

## Décision

- Crate `image_dds` 0.7.2 (approuvée par la spec) : BC1/BC3/BC7 via intel_tex_2 (ISPC),
  décodage via bcdec. Le **même** `core::dds::decode` servira à l'« Aperçu compressé » →
  critère 4 garanti par construction.
- **En-têtes DX9 classiques** pour BC1 (`DXT1`), BC3 (`DXT5`) et RGBA8 (`A8R8G8B8`, octets
  BGRA) : `image_dds` écrit par défaut un en-tête DX10 que certains outils DDS anciens lisent
  mal. BC7 n'a pas de code DX9 → en-tête DX10 (`BC7_UNORM`), déjà réservé à CSP.
- Qualité d'encodage `Normal` ; mipmaps jusqu'à 1×1.
- Redimensionnement 4096/2048/1024 : Lanczos3 (`image::imageops`, réexporté par `image_dds`,
  aucune dépendance ajoutée). Jamais d'agrandissement, ratio conservé.
- Écriture atomique : fichier temporaire caché dans le même dossier, `sync_all`, `rename` ;
  supprimé en cas d'erreur.

## Écarts et limites

- **Mipmaps : filtre boîte 2×2** (celui d'`image_dds`), pas Lanczos/Kaiser comme l'indique
  la spec. Suffisant pour des textures de livrée ; à revoir si des moirés apparaissent au loin
  en jeu (générer les niveaux nous-mêmes avec `imageops::resize` puis encoder chaque niveau).
- intel_tex_2 embarque du C++ sans déclarer libstdc++ : `build.rs` ajoute `-lstdc++`.
- Tailles non multiples de 4 ou non puissances de 2 : avertissement seulement (l'encodeur
  complète les blocs), jamais de refus.
- Non vérifié : affichage dans le Showroom de Content Manager (manuel, checkpoint A).
