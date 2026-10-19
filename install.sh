#!/usr/bin/env bash
# Installs Ddsnap for the current user: psd-tools venv, binaries in ~/.local/bin,
# menu entry and icon. Run again after `git pull` to update.
#   ./install.sh            install
#   ./install.sh --remove   uninstall (the venv and the log stay)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
ID=io.github.fdeslongchamps.Ddsnap
BIN="$HOME/.local/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP="$DATA/applications/$ID.desktop"
ICON="$DATA/icons/hicolor/scalable/apps/$ID.svg"

if [[ "${1:-}" == "--remove" ]]; then
    rm -f "$BIN/ddsnap" "$BIN/ddsnap-gui" "$DESKTOP" "$ICON"
    echo "Ddsnap désinstallé (venv et journal conservés)."
    exit 0
fi

command -v cargo >/dev/null || {
    echo "cargo introuvable : curl https://sh.rustup.rs -sSf | sh" >&2
    exit 1
}
pkg-config --exists gtk4 libadwaita-1 || {
    echo "GTK 4 / libadwaita absents : sudo apt install build-essential libgtk-4-dev libadwaita-1-dev python3-venv" >&2
    exit 1
}

bash "$ROOT/scripts/setup-venv.sh"
cargo build --release --manifest-path "$ROOT/Cargo.toml"
install -Dm755 "$ROOT/target/release/ddsnap" "$BIN/ddsnap"
install -Dm755 "$ROOT/target/release/ddsnap-gui" "$BIN/ddsnap-gui"
# Absolute Exec: ~/.local/bin is only in the session PATH if it existed at login.
mkdir -p "$(dirname "$DESKTOP")"
sed "s|^Exec=ddsnap-gui|Exec=$BIN/ddsnap-gui|" "$ROOT/data/$ID.desktop" >"$DESKTOP"
install -Dm644 "$ROOT/data/icons/$ID.svg" "$ICON"
update-desktop-database "$DATA/applications" 2>/dev/null || true
gtk4-update-icon-cache -q "$DATA/icons/hicolor" 2>/dev/null || true

case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "Ajoutez $BIN à votre PATH (ex. dans ~/.profile)." ;;
esac
"$BIN/ddsnap" doctor | head -1
echo "Ddsnap installé : cherchez « Ddsnap » dans les applications."
