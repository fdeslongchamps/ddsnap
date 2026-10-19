# ADR-001 — psd-tools dans un assistant Python long-vivant

Statut : accepté pour l'export · aperçu en direct **à revoir avant T7** · 2026-10-19

## Contexte

Ddsnap doit lire des PSD (calques, groupes, modes de fusion) et composer une sélection de
calques. Aucune crate Rust ne le fait correctement ; psd-tools (Python) oui.

## Décision

- Un processus Python par PSD ouvert, lancé par `<venv>/bin/python -I -c <script>` (script
  embarqué, aucun fichier écrit). Venv dédié créé par `scripts/setup-venv.sh` (psd-tools 1.25.0).
- Protocole JSON-lignes ; `render` répond un en-tête JSON puis `bytes` octets RGBA bruts.
- Au démarrage, le vrai stdout (fd 1) est dupliqué pour le protocole, puis fd 1 et
  `sys.stdout` sont redirigés vers stderr : aucun `print` ni écriture C ne corrompt le flux
  (testé).
- **`render.visible` liste des index** (ordre de `descendants()`, renvoyés par `open`), pas des
  `layer_id` : un PSD généré par psd-tools n'a aucun `layer_id` (55/55 absents dans le banc
  synthétique), donc l'identifiant ne peut pas servir de clé de protocole. La recette (T4)
  garde `layer_id` puis chemin ; c'est `core::layers` qui traduit vers les index.
- Un calque n'est dessiné que s'il est listé **ainsi que tous ses groupes parents**.
- Erreurs : `{"ok":false,"kind":"protocol|not_found|invalid_psd|internal","error":…}`, le
  processus reste vivant.

## Mesures (banc `helper/bench_psd_helper.py`)

| PSD | open | render 100 % | render 25 % | écart vs composite enregistré |
|---|---|---|---|---|
| Synthétique 4096², 55 calques (pixels aléatoires, 6 groupes) | 0,43 s | 57 s | 57 s | 0,00 (aucun effet) |
| Synthétique 2048², 21 calques | — | 4,8 s | — | — |
| Vrais PSD de l'utilisateur | **en attente des fichiers** | | | |

Profil : tout le temps est dans `psd_tools.composite._apply_source` (fusion numpy en
flottants), ≈ 1 s par calque à 4096². `scale` ne réduit rien : le rendu est composé en
pleine taille puis réduit.

## Conséquences

- **Export (CLI, T2–T4) : go.** Une minute par export reste acceptable pour un outil batch.
- **Aperçu en direct (critère 2, < 2 s) : non atteignable tel quel.** Décision à prendre avec
  l'utilisateur avant T7, sur ses vrais PSD. Pistes, par ordre de préférence :
  1. compositeur d'aperçu maison : chaque calque réduit à l'échelle de l'aperçu puis fusionné
     (mode normal + modes simples) — ~16× moins de pixels à 25 % ; psd-tools reste la
     référence à l'export ;
  2. cache par groupe (ne recomposer que le groupe dont un calque change) ;
  3. abandon du direct : rendu à la demande avec indicateur.
- Effets de calque : pas encore mesurés (aucun vrai PSD fourni).
