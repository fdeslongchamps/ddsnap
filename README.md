# Ddsnap

Choisir des calques d'un PSD et les exporter en textures DDS (BC1/BC3/BC7, mipmaps) pour les jeux, les mods et les skins. Né pour les skins Assetto Corsa, utilisable pour toute texture DDS. Spec : [SPEC.md](SPEC.md) · décisions : [docs/decisions/](docs/decisions/).

État : CLI (`ddsnap layers`, `ddsnap export`) et application GNOME (`ddsnap-gui`, à tester).

## Application

```bash
sudo apt install build-essential python3-venv libgtk-4-dev libadwaita-1-dev
./install.sh              # venv psd-tools, ~/.local/bin/ddsnap{,-gui}, entrée de menu, icône
./install.sh --remove     # désinstalle
```

Ensuite « Ddsnap » dans les applications, ou `ddsnap-gui livery.psd`. En cas de souci :
Menu › Diagnostic (ou `ddsnap doctor`) ; le journal est dans `~/.local/state/ddsnap/ddsnap.log`.

Glissez un PSD dans la fenêtre (ou Ctrl+O). À gauche : les sorties (une par DDS), leurs
réglages et le dossier de sortie (par défaut celui du PSD, donc celui de la skin), puis les
calques à dessiner. À droite : l'aperçu (calques extraits une fois, puis fusionnés par la
carte graphique ; Menu › Sans aperçu pour s'en passer). Ctrl+E exporte la sortie choisie,
Ctrl+Maj+E toutes. La barre du bas montre chaque étape ; le bouton terminal ouvre le journal.

## Tester sur un vrai PSD

```bash
# une fois
sudo apt install build-essential python3-venv
curl https://sh.rustup.rs -sSf | sh        # si cargo est absent

# à chaque test
git pull
scripts/try-psd.sh "/chemin/vers/livery.psd"
```

Le script travaille sur une **copie** du PSD dans un dossier temporaire (rien n'est écrit à
côté de l'original), puis affiche : RAM de la machine, arbre des calques, temps et mémoire de
psd-tools (`open`, rendu 100 % / 25 %, écart avec le composite Photoshop), et l'export DDS
avec son temps et son pic mémoire. Les rendus d'un PSD 4096² ou plus prennent plusieurs
minutes.

## Architecture

- `src/core/` : logique sans GTK (lecture du PSD via l'assistant Python, recette, DDS, export).
- `helper/psd_helper.py` : assistant psd-tools, lancé une fois et piloté en JSON par lignes.
- `src/ui/` : l'application GNOME, incluse seulement dans `ddsnap-gui`.
- Décisions et leurs raisons : [docs/decisions/](docs/decisions/) (ADR-001 à ADR-007).

## CLI

```bash
ddsnap layers livery.psd [--json]
ddsnap doctor
ddsnap export livery.psd [--output livery] [--out-dir dossier] [--format auto|bc1|bc3|bc7|rgba8] [--size source|4096|2048|1024] [--no-mipmaps]
```

Les sorties se règlent dans `livery.ddsnap.json` à côté du PSD (voir SPEC.md › Recette).

## Développement

```bash
bash scripts/setup-venv.sh --dev          # venv psd-tools + pytest
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings
cargo test && cargo test -- --ignored && ~/.local/share/ddsnap/venv/bin/python -m pytest helper/
```
