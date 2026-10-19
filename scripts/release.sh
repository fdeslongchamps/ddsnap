#!/usr/bin/env bash
# Publishes a GitHub release: scripts/release.sh 0.1.0
# Refuses a dirty tree, a version missing from CHANGELOG.md, or any failing check.
set -euo pipefail

VERSION="${1:?usage : scripts/release.sh X.Y.Z}"
TAG="v$VERSION"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

[[ -z "$(git status --porcelain)" ]] || { echo "arbre git sale : commitez d'abord" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && { echo "$TAG existe déjà" >&2; exit 1; }
grep -q "^version = \"$VERSION\"" Cargo.toml || { echo "Cargo.toml n'est pas en version $VERSION" >&2; exit 1; }
NOTES="$(awk -v v="$VERSION" '$0 ~ "^## \\[" v "\\]" {on=1; next} /^## \[/ {on=0} on' CHANGELOG.md)"
[[ -n "${NOTES//[[:space:]]/}" ]] || { echo "section [$VERSION] absente de CHANGELOG.md" >&2; exit 1; }

echo "== Vérifications"
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --quiet

echo "== Archive"
cargo build --release --quiet
NAME="ddsnap-$TAG-x86_64-linux"
STAGE="$(mktemp -d)/$NAME"
mkdir -p "$STAGE/scripts" dist
cp target/release/ddsnap "$STAGE/"
[[ -x target/release/ddsnap-gui ]] && cp target/release/ddsnap-gui "$STAGE/"
cp README.md CHANGELOG.md "$STAGE/"
cp scripts/setup-venv.sh "$STAGE/scripts/"
[[ -f install.sh ]] && cp install.sh "$STAGE/"
[[ -d data ]] && cp -r data "$STAGE/"
tar -czf "dist/$NAME.tar.gz" -C "$(dirname "$STAGE")" "$NAME"

echo "== Publication $TAG"
git tag -a "$TAG" -m "$TAG"
git push origin "$TAG"
gh release create "$TAG" "dist/$NAME.tar.gz" --title "Ddsnap $TAG" --notes "$NOTES"
