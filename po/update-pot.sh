#!/bin/bash
# po/update-pot.sh
#
# Script standalone para regenerar po/pkgbuild_manager.pot fora do Meson.
# Extrai strings de TODAS as fontes do projeto automaticamente:
#
#   Fonte                            Ferramenta         Keyword
#   ──────────────────────────────── ────────────────── ─────────
#   src/**/*.rs  (Rust CLI)          xgettext -L C      gettext()
#   src/settings/app.py  (GTK app)   xgettext -L Python _()
#   data/nautilus-extension/*.py     xgettext -L Python _()
#   bash notify_* keys               po/bash_notify.pot.in  (estático)
#
# Uso:
#   cd <raiz do repo>
#   bash po/update-pot.sh
#
# Após rodar, atualize os .po com:
#   msgmerge --update po/pt_BR.po po/pkgbuild_manager.pot
#   msgmerge --update po/de.po    po/pkgbuild_manager.pot
#   ... (ou use: bash po/update-pot.sh --merge)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.."
PO_DIR="$ROOT/po"
OUT="$PO_DIR/pkgbuild_manager.pot"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

DO_MERGE=false
[[ "${1:-}" == "--merge" ]] && DO_MERGE=true

echo "=== PKGBUILD Manager — regenerando .pot ==="
echo ""

# ── 1. Rust ──────────────────────────────────────────────────────────────────
echo "[1/4] Extraindo strings do Rust (gettext())..."
RUST_FILES=()
while IFS= read -r line; do
    [[ "$line" =~ ^# || -z "$line" ]] && continue
    f="$ROOT/$line"
    [[ "$f" == *.rs && -f "$f" ]] && RUST_FILES+=("$f")
done < "$PO_DIR/POTFILES.in"

if [[ ${#RUST_FILES[@]} -gt 0 ]]; then
    xgettext \
        --from-code=UTF-8 \
        --language=C \
        --keyword=gettext \
        --add-comments=translators \
        --package-name=pkgbuild_manager \
        --package-version="$(grep -m1 'version' "$ROOT/Cargo.toml" | sed 's/.*= *"//;s/"//')" \
        --output="$TMP/rust.pot" \
        "${RUST_FILES[@]}"
    echo "   → ${#RUST_FILES[@]} arquivos .rs processados"
else
    echo "   → nenhum arquivo .rs encontrado no POTFILES.in"
fi

# ── 2. Python settings ───────────────────────────────────────────────────────
echo "[2/4] Extraindo strings do Python (_())..."
PY_FILES=()
while IFS= read -r line; do
    [[ "$line" =~ ^# || -z "$line" ]] && continue
    f="$ROOT/$line"
    [[ "$f" == *.py && -f "$f" ]] && PY_FILES+=("$f")
done < "$PO_DIR/POTFILES.in"

# Extensões do gerenciador de arquivos (Nautilus, Caja, Nemo)
for ext_py in "$ROOT"/data/nautilus-extension/*.py; do
    [[ -f "$ext_py" ]] && PY_FILES+=("$ext_py")
done

if [[ ${#PY_FILES[@]} -gt 0 ]]; then
    xgettext \
        --from-code=UTF-8 \
        --language=Python \
        --keyword=_ \
        --add-comments=translators \
        --package-name=pkgbuild_manager \
        --output="$TMP/python.pot" \
        "${PY_FILES[@]}"
    echo "   → ${#PY_FILES[@]} arquivos .py processados"
else
    echo "   → nenhum arquivo .py encontrado"
fi

# ── 3. Mescla as extrações ────────────────────────────────────────────────────
echo "[3/4] Mesclando .pot extraídos..."
MERGE=()
[[ -f "$TMP/rust.pot" ]]   && MERGE+=("$TMP/rust.pot")
[[ -f "$TMP/python.pot" ]] && MERGE+=("$TMP/python.pot")
MERGE+=("$PO_DIR/bash_notify.pot.in")   # chaves bash estáticas

msgcat \
    --use-first \
    --output="$TMP/merged.pot" \
    "${MERGE[@]}"

# ── 4. Corrige cabeçalho e grava o .pot final ─────────────────────────────────
echo "[4/4] Gravando $OUT..."
PKG_VER=$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*= *"//;s/"//')
DATE=$(date +"%Y-%m-%d %H:%M%z")

sed \
    -e "s|^\"Project-Id-Version:.*|\"Project-Id-Version: pkgbuild_manager $PKG_VER\\\\n\"|" \
    -e "s|^\"POT-Creation-Date:.*|\"POT-Creation-Date: $DATE\\\\n\"|" \
    -e "s|^\"PO-Revision-Date:.*|\"PO-Revision-Date: YEAR-MO-DA HO:MI+ZONE\\\\n\"|" \
    -e "s|^\"Last-Translator:.*|\"Last-Translator: FULL NAME <EMAIL@ADDRESS>\\\\n\"|" \
    -e "s|^\"Language-Team:.*|\"Language-Team: LANGUAGE <LL@li.org>\\\\n\"|" \
    -e "s|^\"Language:.*|\"Language: \\\\n\"|" \
    "$TMP/merged.pot" > "$OUT"

echo ""
echo "✓ Gerado com sucesso: $OUT"

# ── Opcional: atualiza todos os .po ───────────────────────────────────────────
if [[ "$DO_MERGE" == true ]]; then
    echo ""
    echo "=== Atualizando arquivos .po com msgmerge ==="
    for po in "$PO_DIR"/*.po; do
        lang=$(basename "$po" .po)
        echo "  → $lang.po"
        msgmerge --quiet --update --backup=none "$po" "$OUT"
    done
    echo "✓ Todos os .po atualizados!"
else
    echo ""
    echo "Para atualizar os .po existentes, rode:"
    echo "  bash po/update-pot.sh --merge"
    echo ""
    echo "Ou manualmente para cada idioma:"
    for po in "$PO_DIR"/*.po; do
        lang=$(basename "$po" .po)
        echo "  msgmerge --update po/$lang.po po/pkgbuild_manager.pot"
    done
fi
