# Plan d'implémentation — Ddsnap

Source : [SPEC.md](../SPEC.md) · Tâches détaillées : [todo.md](todo.md)
Une tâche = un commit (Conventional Commits), `fmt` + `clippy -D warnings` + `cargo test` verts.
Les vérifications UI et en jeu sont faites à la main par l'utilisateur (checklist par tâche).

Prérequis (fait par l'utilisateur, une fois) :
`sudo apt install python3-venv libgtk-4-dev libadwaita-1-dev blueprint-compiler build-essential pkg-config`.
Déjà présents sur la machine : rustc 1.99.1, Python 3.14.4, GTK 4.22.4, libadwaita 1.9.1,
blueprint-compiler, `gh` connecté. psd-tools **pas encore installé** (la tâche 1 crée le venv).

## Vue d'ensemble

On construit d'abord le moteur et la **CLI** (PSD → DDS de bout en bout, sans GUI), parce que
les deux risques techniques sont là : la qualité/vitesse de psd-tools et l'encodage DDS accepté
par AC. La CLI donne une première release utilisable (`v0.1.0`) et reste la frontière pour
PhotoCraft. La GUI vient ensuite par tranches verticales (ouvrir → voir → choisir → exporter),
puis le diagnostic et l'emballage pour `v1.0.0`.

## Décisions d'architecture (ADR écrits pendant la tâche qui les fige)

- **ADR-001 — psd-tools dans un assistant Python long-vivant** (T1). Un processus par PSD
  ouvert, lancé par `python -I -c` (aucun fichier écrit), protocole JSON-lignes + octets RGBA
  bruts sur un stdout réservé (`sys.stdout` redirigé vers stderr). Évite de relire le PSD à
  chaque case cochée. Venv dédié `~/.local/share/ddsnap/venv`, créé par `scripts/setup-venv.sh` seul.
- **ADR-002 — `core/` synchrone, sans GLib** (T2). Le client de l'assistant utilise
  `std::process` en bloquant ; la CLI l'appelle directement, la GUI depuis un thread de travail
  avec retour sur la boucle GLib (`glib::MainContext::channel` / `spawn_future_local`). Écart
  assumé avec Transcode (qui utilise `gio::Subprocess`) : ici `core` doit servir la CLI sans GTK.
- **ADR-003 — `image_dds` pour l'encodage et le décodage** (T3). Le même code décode le DDS
  écrit pour l'« Aperçu compressé » → critère de réussite 4 garanti par construction.
- **ADR-004 — Recette `<nom>.ddsnap.json` à côté du PSD** (T4) : calques identifiés par
  `layer_id` puis chemin, jamais par position ; noms de sortie validés dans `core` ; champ
  `version` pour migrer.

## Graphe de dépendances

```
T1 assistant Python (spike) ──► T2 squelette + client Rust + `ddsnap layers`
                                   │
                                   ├──► T3 encodage DDS + `ddsnap export` (recette implicite)
                                   │        │
                                   │        └──► T4 recette sidecar ──► T5 dépôt + release v0.1.0
                                   │
                                   └──► T6 GUI : ouvrir + arbre ──► T7 aperçu + cases
                                                                       │
                                            (T4) ──────────────────► T8 panneau Sorties
                                                                       │
                                            (T3) ──────────────────► T9 export + aperçu compressé
                                                                       │
                                                                     T10 diagnostic
                                                                     T11 cas limites
                                                                     T12 install + v1.0.0
```

## Tâches

Liste, critères et points d'étape : **[todo.md](todo.md)** (seule source, cochée au fil de l'eau).
Phase 1 moteur + CLI (T1–T4, point d'étape A, T5 → `v0.1.0`) · Phase 2 GUI (T6–T9, point
d'étape B → `v0.2.0`) · Phase 3 robustesse (T10–T12 → `v1.0.0`).

## Risques et parades

| Risque | Impact | Parade |
|---|---|---|
| psd-tools trop lent pour l'aperçu en direct (PSD 4096², ~50 calques) | Élevé | Mesuré en T1 sur tes PSD. Pistes : composer à l'échelle de l'aperçu, cache des groupes non modifiés, plein format seulement à l'export. Si > 2 s malgré tout → on en reparle avant T7. |
| Effets de calque (ombres, contours) mal rendus par psd-tools | Élevé | Comparaison visuelle en T1 avec l'image composite enregistrée dans le PSD. Si écart → avertissement par calque concerné + recommandation de pixelliser l'effet. |
| `image_dds` : sortie refusée par AC ou mipmaps alpha incorrects | Moyen | T3 vérifie l'en-tête (FourCC, mip count) en test et en jeu au checkpoint A, avant toute GUI. |
| Mémoire : RGBA 4096² = 64 Mo par image, plusieurs copies | Moyen | Aperçu réduit ; une seule image plein format vivante à l'export. |
| Calques renommés, déplacés ou insérés entre deux sessions → recette cassée | Moyen | `layer_id` puis chemin, jamais la position ; signalement explicite (T4, T11). |
| Recette éditée à la main qui vise `livery.psd` ou `../` → PSD écrasé | Élevé | Validation des noms de sortie dans `core::recipe`, partagée CLI/GUI (T4). |
| Python 3.14 trop récent pour une dépendance de psd-tools (numpy, scikit-image…) | Moyen | Découvert dès T1 ; repli : `python3.12` via `uv` si nécessaire. |

## Parallélisation

Seul, l'ordre ci-dessus suffit. Si on veut paralléliser : T3 et T6 sont indépendants une fois T2
fait ; T10 peut avancer en parallèle de T8–T9.

## Questions ouvertes

1. **Vrais PSD de test** : donne-moi le chemin de 1 à 3 skins PSD (dont une avec des effets de
   calque si tu en as). Ils restent **hors du dépôt** ; T1 en a besoin pour les mesures.
2. **BC7 / Custom Shaders Patch** : toujours ouverte (spec). Sans réponse, BC7 reste une option
   avec avertissement.
