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

Recommandation : 1 maintenant (utile tout de suite), 4 proposé en amont ; 3 seulement si
psd-tools reste trop lent ou inexact sur les PSD de PhotoCraft.
