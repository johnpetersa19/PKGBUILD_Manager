#!/bin/bash
# po/update-pot.sh
#
# Regenera po/pkgbuild_manager.pot extraindo strings de TODAS as fontes
# do projeto automaticamente e sincroniza LINGUAS <-> arquivos .po.
#
#   Fonte                              Ferramenta         Keyword
#   ──────────────────────────────── ────────────────── ─────────
#   src/**/*.rs        (Rust)          xgettext -L C      gettext()
#   src/**/*.py        (Python GTK)    xgettext -L Python _()
#   data/**/*.py       (extensões)     xgettext -L Python _()
#   bash notify_* keys                 po/bash_notify.pot.in  (estático)
#
# Sincronização bidirecional LINGUAS <-> .po:
#   • .po encontrado sem entrada no LINGUAS  → adiciona ao LINGUAS
#   • idioma no LINGUAS sem arquivo .po       → cria .po via msginit
#
# Uso:
#   cd <raiz do repo>
#   bash po/update-pot.sh            # regenera .pot e sincroniza
#   bash po/update-pot.sh --merge    # regenera, sincroniza e atualiza todos os .po

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.."
PO_DIR="$ROOT/po"
OUT="$PO_DIR/pkgbuild_manager.pot"
POTFILES="$PO_DIR/POTFILES.in"
LINGUAS_FILE="$PO_DIR/LINGUAS"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

DO_MERGE=false
[[ "${1:-}" == "--merge" ]] && DO_MERGE=true

echo "=== PKGBUILD Manager — regenerando .pot ==="
echo ""

# ── 1. Descobrir arquivos Rust ────────────────────────────────────────────────
echo "[1/6] Descobrindo arquivos Rust (.rs com gettext())..."
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
echo "[2/6] Descobrindo arquivos Python (.py com _())..."
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
echo "[3/6] Atualizando POTFILES.in..."
{
    echo "# Auto-gerado por po/update-pot.sh — não edite manualmente"
    echo ""
    for f in "${RUST_FILES[@]}" "${PY_FILES[@]}"; do
        echo "${f#"$ROOT/"}"
    done
} > "$POTFILES"
echo "   → POTFILES.in atualizado com $((${#RUST_FILES[@]} + ${#PY_FILES[@]})) entradas"

# ── 4. Mesclar todas as fontes ────────────────────────────────────────────────
echo "[4/6] Mesclando .pot extraídos + chaves bash estáticas..."
MERGE=()
[[ -f "$TMP/rust.pot" ]]   && MERGE+=("$TMP/rust.pot")
[[ -f "$TMP/python.pot" ]] && MERGE+=("$TMP/python.pot")
MERGE+=("$PO_DIR/bash_notify.pot.in")

msgcat \
    --use-first \
    --output="$TMP/merged.pot" \
    "${MERGE[@]}"

TOTAL=$(grep -c '^msgid ' "$TMP/merged.pot" || true)
echo "   → $TOTAL entradas mescladas no total"

# ── 5. Corrigir cabeçalho e gravar .pot final ─────────────────────────────────
echo "[5/6] Gravando $OUT..."
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

echo "   → $OUT gravado"

# ── 6. Sincronizar LINGUAS <-> arquivos .po ──────────────────────────────────
echo "[6/6] Sincronizando LINGUAS <-> arquivos .po..."

# Ler idiomas já registrados no LINGUAS
declare -A LINGUAS_SET
while IFS= read -r lang || [[ -n "$lang" ]]; do
    lang="$(echo "$lang" | tr -d '[:space:]')"
    [[ -z "$lang" || "$lang" == \#* ]] && continue
    LINGUAS_SET["$lang"]=1
done < "$LINGUAS_FILE"

ADDED_TO_LINGUAS=()
CREATED_PO=()

# 6a. .po encontrado mas não registrado no LINGUAS → adicionar
for po_file in "$PO_DIR"/*.po; do
    [[ -f "$po_file" ]] || continue
    lang=$(basename "$po_file" .po)
    if [[ -z "${LINGUAS_SET[$lang]:-}" ]]; then
        echo "   + LINGUAS: adicionando '$lang' (arquivo .po existe)"
        echo "$lang" >> "$LINGUAS_FILE"
        LINGUAS_SET["$lang"]=1
        ADDED_TO_LINGUAS+=("$lang")
    fi
done

# 6b. Idioma no LINGUAS sem arquivo .po → criar com msginit
for lang in "${!LINGUAS_SET[@]}"; do
    po_file="$PO_DIR/$lang.po"
    if [[ ! -f "$po_file" ]]; then
        echo "   + Criando $lang.po via msginit..."
        msginit \
            --input="$OUT" \
            --locale="$lang" \
            --output="$po_file" \
            --no-translator \
            2>/dev/null || true
        if [[ -f "$po_file" ]]; then
            echo "   → $lang.po criado ($(grep -c '^msgid ' "$po_file" || true) entradas, strings em branco para traduzir)"
            CREATED_PO+=("$lang")
        else
            echo "   ⚠ msginit falhou para '$lang' — verifique se o locale está instalado"
        fi
    fi
done

# Regravar LINGUAS ordenado e sem duplicatas
sort -u "$LINGUAS_FILE" > "$TMP/linguas_sorted"
mv "$TMP/linguas_sorted" "$LINGUAS_FILE"

# Resumo da sincronização
if [[ ${#ADDED_TO_LINGUAS[@]} -gt 0 ]]; then
    echo "   → Adicionados ao LINGUAS: ${ADDED_TO_LINGUAS[*]}"
fi
if [[ ${#CREATED_PO[@]} -gt 0 ]]; then
    echo "   → Arquivos .po criados: ${CREATED_PO[*]}"
fi
if [[ ${#ADDED_TO_LINGUAS[@]} -eq 0 && ${#CREATED_PO[@]} -eq 0 ]]; then
    echo "   → LINGUAS e .po já estão sincronizados, nada a fazer"
fi

echo ""
echo "✓ Gerado: $OUT"
echo "✓ Entradas totais: $TOTAL"
echo "✓ Versão: pkgbuild_manager $PKG_VER"
echo "✓ Idiomas registrados: $(tr '\n' ' ' < "$LINGUAS_FILE" | xargs)"

# ── Opcional: atualizar todos os .po ─────────────────────────────────────────
if [[ "$DO_MERGE" == true ]]; then
    echo ""
    echo "=== Atualizando arquivos .po com msgmerge ==="
    for po in "$PO_DIR"/*.po; do
        lang=$(basename "$po" .po)
        printf "  → %-14s" "$lang.po"
        msgmerge --quiet --update --backup=none "$po" "$OUT"
        UNTRANSLATED=$(grep -c '^msgstr ""' "$po" || true)
        echo " ($UNTRANSLATED strings sem tradução)"
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
