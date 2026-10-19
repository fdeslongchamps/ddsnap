# Ddsnap et PhotoCraft — état des lieux (2026-10-19)

But : utiliser Ddsnap « comme un plugin » de PhotoCraft (éditeur type Photoshop en Rust,
`storytold/photocraft`, alpha v0.2.x).

## Ce que PhotoCraft offre

| Point d'entrée | Ce qu'il permet | Utile pour Ddsnap ? |
|---|---|---|
| Plug-ins WebAssembly, ABI v1 | Filtre seulement : reçoit les pixels d'**un** calque (bandes f32), les renvoie modifiés. Aucun import : pas de fichiers, réseau, processus. Pas de format de fichier ni d'export en v1. | **Non** : ne peut ni lire les autres calques ni écrire un DDS. |
| `photocraft-cli run x.psd --cmd … --out x.png` | Lot de commandes du catalogue (500+), export raster. | Partiel : PNG en sortie, pas de DDS, pas de choix de calques documenté. |
| `photocraft-cli mcp` (serveur MCP, JSON-RPC) | Lire l'arbre des calques, basculer masques, exporter des rasters. | Possible, mais noms des outils non documentés publiquement ; API alpha. |
| Fichiers PSD (crate `photocraft-psd`, 169/170 tests aller-retour) | PhotoCraft lit et écrit le PSD. | **Oui, dès aujourd'hui** : Ddsnap lit ces PSD. |

Sources : [docs/plugins.md](https://github.com/storytold/photocraft/blob/main/docs/plugins.md),
[article HoangYell](https://hoangyell.com/photocraft-explained/) (non officiel). Non vérifié :
noms des outils MCP, présence de l'image aplatie (« Maximiser la compatibilité ») dans les PSD
écrits par PhotoCraft — sans elle, l'export « tel quel » de Ddsnap retombe sur psd-tools (lent).

## Ce que dit le code de PhotoCraft (clone du 2026-10-19, commit 3a896a5)

- PhotoCraft **ouvre** déjà les DDS (lecture seule, #1327 : niveau 0, BC1–BC3, non compressé).
- L'écriture est refusée exprès : « écrire un DDS correctement demande de choisir une
  compression par blocs et de générer les mipmaps, ce qui relève d'un **export de texture**,
  pas de Enregistrer sous » (`crates/codecs/src/format.rs`, `ASYMMETRIC_EXCEPTIONS`).
  Un export de texture est donc le bon endroit, et les mainteneurs l'ont déjà envisagé.
- Contraintes du projet (`AGENTS.md`) : Rust seulement, pas d'`unsafe`, jamais de panique,
  les couches basses doivent compiler en WebAssembly (`cargo xtask wasm`). Le moteur de Ddsnap
  (`image_dds` → intel_tex_2, du C++) ne passe pas en wasm : il faudrait un encodeur BC
  écrit en Rust (BC1/BC3 au minimum ; BC7 à vérifier).
- Tout passe par des commandes typées (aussi pilotables par CLI et MCP) avec un test d'échec
  propre par commande, et des documents de parité à régénérer.

## Pistes

1. **Surveiller le PSD** : `ddsnap watch livery.psd` (et un interrupteur dans l'app) réexporte
   les DDS à chaque enregistrement depuis PhotoCraft. Aucune dépendance à PhotoCraft, marche
   avec n'importe quel éditeur. Le plus petit pas.
2. **Bouton « Ouvrir dans Ddsnap »** côté PhotoCraft : impossible avec l'ABI v1 (pas de
   processus) ; à demander en amont (commande externe / outil).
3. **MCP** : Ddsnap pilote PhotoCraft (arbre, visibilité, rendu) au lieu de psd-tools. Rendu
   plus fidèle et plus rapide possible, mais dépend d'une API alpha non documentée.
4. **Export DDS natif dans PhotoCraft** : contribuer un format DDS en amont (PhotoCraft est en
   Rust, `image_dds` s'y intègre). Le vrai « dans l'app », mais c'est leur dépôt et leur feuille
   de route.

Besoin réel de l'utilisateur : **enregistrer en .dds depuis PhotoCraft**. Seule la piste 4 y
répond vraiment : une commande « Exporter en texture (DDS)… » dans PhotoCraft (format BC,
mipmaps, comme le plug-in NVIDIA de Photoshop). Une version locale (fork) sans PR est possible,
mais PhotoCraft fusionne des dizaines de PR par jour : la garder à jour coûterait cher.
La piste 1 reste un pont utile en attendant.

## Décision (2026-10-19)

L'utilisateur enregistre son PSD dans PhotoCraft, puis ouvre ce PSD dans Ddsnap. Aucun
changement dans PhotoCraft, aucun plug-in. L'aller-retour automatique (choisir `.dds` dans
PhotoCraft → Ddsnap s'ouvre → se ferme) demanderait du code dans PhotoCraft : écarté pour
l'instant. À vérifier sur un vrai fichier : les PSD écrits par PhotoCraft contiennent-ils
l'image aplatie ? Sinon l'export « tel quel » passe par psd-tools (plus lent).
