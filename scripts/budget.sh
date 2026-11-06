#!/usr/bin/env bash
# Presupuestos de 01 §11.
#
#   scripts/budget.sh [bundle]   tamaños del build de Vite (dist/), en gzip. Es lo
#                                que corre la CI. Sale con 1 si algo se pasa.
#   scripts/budget.sh ram        RSS de la app abierta y de los WebContent de
#                                WebKit. Solo lee `ps`; se ejecuta a mano antes de
#                                cada versión y el resultado va al PR.
#
# 1 KB = 1024 bytes. gzip con el nivel por defecto (`gzip -c`), como pide 01 §11.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist="$root/dist"

kb() { awk -v b="$1" 'BEGIN { printf "%.1f KB", b / 1024 }'; }
mb() { awk -v k="$1" 'BEGIN { printf "%.1f MB", k / 1024 }'; }
gz() { gzip -c "$1" | wc -c | tr -d ' '; }

failed=0

# Imprime una fila y apunta el fallo si `bytes` pasa de `limit_kb`.
row() {
  local label="$1" bytes="$2" limit_kb="$3"
  local limit=$((limit_kb * 1024)) mark="ok"
  if [ "$bytes" -gt "$limit" ]; then
    mark="SE PASA"
    failed=1
  fi
  printf "%-44s %10s  (≤ %s KB)  %s\n" "$label" "$(kb "$bytes")" "$limit_kb" "$mark"
}

# Chunks de JS que carga index.html: el script de entrada y sus modulepreload.
initial_chunks() {
  grep -oE '(src|href)="/assets/[^"]+\.js"' "$dist/index.html" | sed -E 's/^(src|href)="\/(.*)"$/\2/'
}

# Chunk de una vista diferida: el que lleva su nombre (Logs-*.js, Wizard-*.js).
lazy_chunk() {
  find "$dist/assets" -name "$1-*.js" | head -1
}

bundle() {
  if [ ! -f "$dist/index.html" ]; then
    echo "No hay build: ejecuta pnpm build." >&2
    exit 2
  fi

  local total=0 names=()
  while IFS= read -r chunk; do
    total=$((total + $(gz "$dist/$chunk")))
    names+=("$(basename "$chunk")")
  done < <(initial_chunks)
  row "JS inicial (${names[*]})" "$total" 40

  local view file
  for view in Logs Wizard; do
    file="$(lazy_chunk "$view")"
    if [ -z "$file" ]; then
      echo "No se encuentra el chunk de $view." >&2
      failed=1
      continue
    fi
    row "Chunk $view ($(basename "$file"))" "$(gz "$file")" 20
  done

  local css=0 f
  for f in "$dist"/assets/*.css; do
    css=$((css + $(gz "$f")))
  done
  row "CSS total" "$css" 15

  if [ "$failed" -ne 0 ]; then
    echo "Hay presupuestos que se pasan (01 §11)." >&2
    exit 1
  fi
}

# RSS en KB de un pid según `ps`.
rss_kb() { ps -o rss= -p "$1" | tr -d ' '; }

ram() {
  local pids
  pids="$(pgrep -f 'Elephant Stack.app/Contents/MacOS/elephant-stack' || true)"
  if [ -z "$pids" ]; then
    echo "Elephant Stack no está abierta: no hay nada que medir."
    exit 0
  fi
  local pid
  for pid in $pids; do
    printf "App      pid %-6s RSS %8s  (objetivo sin ventana: < 30 MB)  %s\n" \
      "$pid" "$(mb "$(rss_kb "$pid")")" "$(ps -o lstart= -p "$pid")"
  done

  # WebKit no deja ver de qué app es cada WebContent: se listan todos con su hora
  # de inicio. Sin ventana abierta, la app no debe tener ninguno (01 §11).
  local found=0
  while read -r wpid wrss; do
    found=1
    printf "WebContent pid %-6s RSS %8s  desde %s\n" "$wpid" "$(mb "$wrss")" "$(ps -o lstart= -p "$wpid")"
  done < <(ps -axo pid=,rss=,comm= | awk '/com\.apple\.WebKit\.WebContent/ { print $1, $2 }')
  [ "$found" -eq 1 ] || echo "Ningún proceso WebContent en marcha."
}

case "${1:-bundle}" in
  bundle) bundle ;;
  ram) ram ;;
  *)
    echo "Uso: $0 [bundle|ram]" >&2
    exit 2
    ;;
esac
