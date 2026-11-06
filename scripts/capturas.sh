#!/usr/bin/env bash
# Capturas de referencia de cada vista, en claro y en oscuro (01 §12), en
# docs/capturas/. Monta la app con el IPC simulado de src/dev/capturas.ts en un
# servidor de Vite propio y la fotografía con Chrome sin interfaz, con un perfil
# temporal: no abre la app ni toca nada del sistema.
#
# Uso: scripts/capturas.sh    (necesita Google Chrome en /Applications)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/docs/capturas"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
port=14217
# Tamaño inicial de la ventana (tauri.conf.json).
size="920,600"

if [ ! -x "$chrome" ]; then
  echo "No se encuentra Chrome en $chrome (se puede indicar con CHROME=…)." >&2
  exit 1
fi

profile="$(mktemp -d)"
cd "$root"
pnpm exec vite --port "$port" --strictPort >"$profile/vite.log" 2>&1 &
vite=$!
trap 'kill "$vite" 2>/dev/null || true; rm -rf "$profile"' EXIT

for _ in $(seq 1 50); do
  curl -sS -f -o /dev/null "http://localhost:$port/src/dev/capturas.html" 2>/dev/null && break
  sleep 0.2
done

mkdir -p "$out"
# nombre del archivo y ruta de la app
views=(
  "panel #/panel"
  "sitios #/sitios"
  "servicios #/servicios"
  "php #/php"
  "logs #/logs?range=all"
  "logs-inspector #/logs?range=all&entry=00000000000000ab-9f20"
  "ajustes-general #/ajustes/general"
  "ajustes-logs #/ajustes/logs"
  "ajustes-agentes #/ajustes/agentes"
  "asistente #/asistente"
)

for theme in claro oscuro; do
  for view in "${views[@]}"; do
    name="${view%% *}"
    route="${view#* }"
    "$chrome" --headless=new --disable-gpu --hide-scrollbars --no-first-run \
      --user-data-dir="$profile/chrome" --window-size="$size" --force-device-scale-factor=1 \
      --virtual-time-budget=5000 --screenshot="$out/$name-$theme.png" \
      "http://localhost:$port/src/dev/capturas.html?tema=$theme$route" >/dev/null 2>&1
    echo "docs/capturas/$name-$theme.png"
  done
done
