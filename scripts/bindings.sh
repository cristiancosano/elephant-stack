#!/usr/bin/env bash
# Genera los tipos de TypeScript de src/lib/bindings a partir de Rust (ts-rs).
#
#   scripts/bindings.sh           regenera la carpeta
#   scripts/bindings.sh --check   falla si hay diferencias sin regenerar o si
#                                 aparece `bigint` (un u64/i64 sin #[ts(type = "number")])
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/src/lib/bindings"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

check=false
case "${1:-}" in
  "") ;;
  --check) check=true ;;
  *)
    echo "Uso: $0 [--check]" >&2
    exit 2
    ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Se genera siempre en una carpeta limpia: así desaparecen los tipos borrados.
# Cargo no pisa una variable que ya está en el entorno con la de .cargo/config.toml.
(
  cd "$root"
  TS_RS_EXPORT_DIR="$tmp" cargo test --quiet --workspace \
    --features elephant-core/ts,elephant-logs/ts \
    export_bindings >/dev/null
)

if [ -z "$(ls -A "$tmp")" ]; then
  echo "ts-rs no ha generado ningún tipo." >&2
  exit 1
fi

if grep -rl bigint "$tmp" >/dev/null; then
  echo "Hay tipos con bigint: añade #[ts(type = \"number\")] al campo u64/i64." >&2
  grep -rn bigint "$tmp" | sed "s|$tmp/|src/lib/bindings/|" >&2
  exit 1
fi

if $check; then
  if ! diff -r "$tmp" "$out" >/dev/null 2>&1; then
    echo "src/lib/bindings no está al día: ejecuta scripts/bindings.sh." >&2
    diff -r "$tmp" "$out" | sed "s|$tmp|(generado)|" | head -40 >&2 || true
    exit 1
  fi
  echo "Bindings al día."
  exit 0
fi

rm -rf "$out"
mkdir -p "$(dirname "$out")"
cp -R "$tmp" "$out"
echo "Bindings generados en src/lib/bindings ($(find "$out" -name '*.ts' | wc -l | tr -d ' ') tipos)."
