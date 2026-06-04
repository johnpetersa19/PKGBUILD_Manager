#!/bin/bash
# po/update-pot.sh
#
# Regenera po/pkgbuild_manager.pot extraindo strings de TODAS as fontes
# do projeto automaticamente (sem depender do POTFILES.in estar completo).
#
#   Fonte                              Ferramenta         Keyword
#   ──────────────────────────────── ────────────────── ─────────
#   src/**/*.rs        (Rust)          xgettext -L C      gettext()
#   src/**/*.py        (Python GTK)    xgettext -L Python _()
#   data/**/*.py       (extensões)     xgettext -L Python _()
#   bash notify_* keys                 po/bash_notify.pot.in  (estático)
#
# Após rodar, o POTFILES.in é atualizado automaticamente com os arquivos
# encontrados.
#
# Uso:
#   cd <raiz do repo>
#   bash po/update-pot.sh            # só regenera o .pot
#   bash po/update-pot.sh --merge    # regenera e atualiza todos os .po

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.."
PO_DIR="$ROOT/po"
OUT="$PO_DIR/pkgbuild_manager.pot"
POTFILES="$PO_DIR/POTFILES.in"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

DO_MERGE=false
[[ "${1:-}" == "--merge" ]] && DO_MERGE=true

echo "=== PKGBUILD Manager — regenerando .pot ==="
echo ""

# ── 1. Descobrir arquivos Rust ────────────────────────────────────────────────
echo "[1/5] Descobrindo arquivos Rust (.rs com gettext())..."
mapfile -t RUST_FILES < <(
    find "$ROOT/src" -name "*.rs" -type f | sort
)
echo "   → ${#RUST_FILES[@]} arquivos .rs encontrados"

if [[ ${#RUST_FILES[@]} -gt 0 ]]; then
    xgettext \
        --from-code=UTF-8 \
        --language=C \
        --keyword=gettext \
        --add-comments=translators \
        --package-name=pkgbuild_manager \
        --package-version="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*= *"//;s/"//')" \
        --output="$TMP/rust.pot" \
        "${RUST_FILES[@]}"
    echo "   → rust.pot gerado ($(grep -c '^msgid' "$TMP/rust.pot" || true) entradas)"
else
    echo "   → nenhum arquivo .rs encontrado"
fi

# ── 2. Descobrir arquivos Python ──────────────────────────────────────────────
echo "[2/5] Descobrindo arquivos Python (.py com _())..."
mapfile -t PY_FILES < <(
    {
        find "$ROOT/src"  -name "*.py" -type f
        find "$ROOT/data" -name "*.py" -type f
    } | sort -u
)
echo "   → ${#PY_FILES[@]} arquivos .py encontrados"

if [[ ${#PY_FILES[@]} -gt 0 ]]; then
    xgettext \
        --from-code=UTF-8 \
        --language=Python \
        --keyword=_ \
        --add-comments=translators \
        --package-name=pkgbuild_manager \
        --output="$TMP/python.pot" \
        "${PY_FILES[@]}"
    echo "   → python.pot gerado ($(grep -c '^msgid' "$TMP/python.pot" || true) entradas)"
else
    echo "   → nenhum arquivo .py encontrado"
fi

# ── 3. Atualizar POTFILES.in automaticamente ──────────────────────────────────
echo "[3/5] Atualizando POTFILES.in..."
{
    echo "# Auto-gerado por po/update-pot.sh — não edite manualmente"
    echo ""
    for f in "${RUST_FILES[@]}" "${PY_FILES[@]}"; do
        # caminho relativo à raiz do repo
        echo "${f#"$ROOT/"}"
    done
} > "$POTFILES"
echo "   → POTFILES.in atualizado com $((${#RUST_FILES[@]} + ${#PY_FILES[@]})) entradas"

# ── 4. Mesclar todas as fontes ────────────────────────────────────────────────
echo "[4/5] Mesclando .pot extraídos + chaves bash estáticas..."
MERGE=()
[[ -f "$TMP/rust.pot" ]]   && MERGE+=("$TMP/rust.pot")
[[ -f "$TMP/python.pot" ]] && MERGE+=("$TMP/python.pot")
MERGE+=("$PO_DIR/bash_notify.pot.in")   # chaves notify_* dos scripts bash

msgcat \
    --use-first \
    --output="$TMP/merged.pot" \
    "${MERGE[@]}"

TOTAL=$(grep -c '^msgid ' "$TMP/merged.pot" || true)
echo "   → $TOTAL entradas mescladas no total"

# ── 5. Corrigir cabeçalho e gravar .pot final ─────────────────────────────────
echo "[5/5] Gravando $OUT..."
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
echo "✓ Gerado: $OUT"
echo "✓ Entradas totais: $TOTAL"
echo "✓ Versão: pkgbuild_manager $PKG_VER"

# ── Opcional: atualizar todos os .po ─────────────────────────────────────────
if [[ "$DO_MERGE" == true ]]; then
    echo ""
    echo "=== Atualizando arquivos .po com msgmerge ==="
    for po in "$PO_DIR"/*.po; do
        lang=$(basename "$po" .po)
        printf "  → %-12s" "$lang.po"
        msgmerge --quiet --update --backup=none "$po" "$OUT"
        NEW=$(grep -c '^msgstr ""' "$po" || true)
        echo " (${NEW} strings sem tradução)"
    done
    echo "✓ Todos os .po atualizados!"
else
    echo ""
    echo "Para atualizar os .po existentes, rode:"
    echo "  bash po/update-pot.sh --merge"
    echo ""
    echo "Ou manualmente por idioma:"
    for po in "$PO_DIR"/*.po; do
        lang=$(basename "$po" .po)
        echo "  msgmerge --update po/$lang.po po/pkgbuild_manager.pot"
    done
fi
