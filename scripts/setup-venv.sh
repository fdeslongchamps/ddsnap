#!/usr/bin/env bash
# Creates the Python venv used by Ddsnap's psd-tools helper.
# The only place where the psd-tools version is pinned (install.sh calls this script).
set -euo pipefail

PSD_TOOLS_VERSION="1.25.0"
VENV="${DDSNAP_VENV:-${XDG_DATA_HOME:-$HOME/.local/share}/ddsnap/venv}"

if ! python3 -c 'import venv, ensurepip' 2>/dev/null; then
    echo "python3-venv manquant : sudo apt install python3-venv" >&2
    exit 1
fi

python3 -m venv "$VENV"
"$VENV/bin/pip" install --quiet --upgrade pip
# [composite]: scipy etc., needed to render layer effects such as strokes.
"$VENV/bin/pip" install --quiet "psd-tools[composite]==$PSD_TOOLS_VERSION"
if [[ "${1:-}" == "--dev" ]]; then
    "$VENV/bin/pip" install --quiet pytest
fi
echo "venv prêt : $VENV (psd-tools $PSD_TOOLS_VERSION)"
