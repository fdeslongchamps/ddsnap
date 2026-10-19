# Ddsnap

PSD → DDS pour les skins Assetto Corsa. Spec : [SPEC.md](SPEC.md) · décisions : [docs/decisions/](docs/decisions/).

État : CLI (`ddsnap layers`, `ddsnap export`) ; la GUI n'existe pas encore.

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

## CLI

```bash
ddsnap layers livery.psd [--json]
ddsnap export livery.psd [--output livery] [--out-dir dossier] [--format auto|bc1|bc3|bc7|rgba8] [--size source|4096|2048|1024] [--no-mipmaps]
```

Les sorties se règlent dans `livery.ddsnap.json` à côté du PSD (voir SPEC.md › Recette).

## Développement

```bash
bash scripts/setup-venv.sh --dev          # venv psd-tools + pytest
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings
cargo test && cargo test -- --ignored && ~/.local/share/ddsnap/venv/bin/python -m pytest helper/
```
