#!/usr/bin/env bash
# Full test on a real PSD: build, layer tree, helper benchmark, timed export.
# Works on a copy in a temporary folder: nothing is ever written next to the original PSD.
#   scripts/try-psd.sh "/chemin/vers/livery.psd"
set -euo pipefail

if [[ $# -ne 1 || ! -f "$1" ]]; then
    echo "usage : scripts/try-psd.sh /chemin/vers/fichier.psd" >&2
    exit 1
fi
PSD="$(realpath "$1")"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENV="${DDSNAP_VENV:-${XDG_DATA_HOME:-$HOME/.local/share}/ddsnap/venv}"

command -v cargo >/dev/null || {
    echo "cargo introuvable : curl https://sh.rustup.rs -sSf | sh, puis sudo apt install build-essential" >&2
    exit 1
}
[[ -x "$VENV/bin/python" ]] || bash "$ROOT/scripts/setup-venv.sh"

echo "== Machine"
free -h | head -2
nproc | sed 's/^/cœurs : /'

echo "== Compilation"
cargo build --release --manifest-path "$ROOT/Cargo.toml"  # première fois : 2 à 5 min
DDSNAP="$ROOT/target/release/ddsnap"

WORK="$(mktemp -d)"
cp "$PSD" "$WORK/"
COPY="$WORK/$(basename "$PSD")"
echo "== Copie de travail : $WORK"

echo "== Arbre des calques"
"$DDSNAP" layers "$COPY"

echo "== Banc de l'assistant (open, rendu 100 % et 25 %, écart avec le composite Photoshop)"
/usr/bin/time -f "   pic mémoire %M Ko · %e s" "$VENV/bin/python" "$ROOT/helper/bench_psd_helper.py" "$COPY" || true

echo "== Export DDS (recette par défaut : calques visibles du PSD)"
if /usr/bin/time -f "   pic mémoire %M Ko · %e s" "$DDSNAP" export "$COPY"; then
    echo
    echo "DDS écrits dans $WORK :"
    ls -lh "$WORK"/*.dds
    echo "Copie-les dans le dossier de la skin pour vérifier dans Content Manager (Showroom)."
else
    echo "Export échoué (voir ci-dessus)."
fi
