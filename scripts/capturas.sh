#!/usr/bin/env bash
# Capturas de referencia de cada vista, en claro y en oscuro (01 §12), en
# docs/capturas/. Monta la app con el IPC simulado de src/dev/capturas.ts en un
# servidor de Vite propio y la fotografía con Chrome sin interfaz, con un perfil
# temporal: no abre la app ni toca nada del sistema.
#
# Uso: scripts/capturas.sh [es|en|todos]    (necesita Google Chrome en /Applications)
#
# El idioma también se puede dar con LANG_CAPTURAS=es|en|todos. Por defecto,
# los dos: las de español se llaman <vista>-<tema>.png, como siempre, y las de
# inglés llevan el sufijo -en (<vista>-<tema>-en.png). CAPTURAS_DIR cambia la
# carpeta de salida.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${CAPTURAS_DIR:-$root/docs/capturas}"
case "${1:-${LANG_CAPTURAS:-todos}}" in
  es) langs=(es) ;;
  en) langs=(en) ;;
  todos) langs=(es en) ;;
  *)
    echo "Uso: $0 [es|en|todos]" >&2
    exit 2
    ;;
esac
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
# nombre del archivo, ruta de la app y, si no es el de la ventana, tamaño
views=(
  "panel #/panel"
  "sitios #/sitios"
  "servicios #/servicios"
  "servicios-todo #/servicios 920,1560"
  "php #/php"
  "logs #/logs?range=all"
  "logs-inspector #/logs?range=all&entry=00000000000000ab-9f20"
  "ajustes-general #/ajustes/general"
  "ajustes-logs #/ajustes/logs"
  "ajustes-agentes #/ajustes/agentes"
  "asistente #/asistente"
)

for lang in "${langs[@]}"; do
  suffix=""
  [ "$lang" = es ] || suffix="-$lang"
  for theme in claro oscuro; do
    for view in "${views[@]}"; do
      read -r name route view_size <<<"$view"
      file="$out/$name-$theme$suffix.png"
      "$chrome" --headless=new --disable-gpu --hide-scrollbars --no-first-run \
        --user-data-dir="$profile/chrome" --window-size="${view_size:-$size}" --force-device-scale-factor=1 \
        --virtual-time-budget=5000 --screenshot="$file" \
        "http://localhost:$port/src/dev/capturas.html?tema=$theme&idioma=$lang$route" >/dev/null 2>&1
      echo "${file#"$root"/}"
    done
  done
done
