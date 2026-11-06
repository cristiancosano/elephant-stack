#!/usr/bin/env bash
# Desarrollo: compila el selector de PHP y la orden `elephant` y los copia a
# ~/.elephant-stack/bin/{php,elephant}.
# No toca el PATH: hasta añadir esa carpeta a tu shell, `php` no cambia. Los
# agentes encuentran `elephant` por el plugin de Claude Code (Ajustes > Agentes).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="$HOME/.elephant-stack/bin"

cargo build --release --manifest-path "$root/Cargo.toml" -p elephant-php -p elephant-cli

mkdir -p "$target"
install -m 755 "$root/target/release/php" "$target/php"
# La app enlaza aquí su propio sidecar; en desarrollo se usa una copia. Si ya
# hay un enlace, se sustituye por la copia recién compilada.
rm -f "$target/elephant"
install -m 755 "$root/target/release/elephant" "$target/elephant"

echo "Instalado en $target/php y $target/elephant"
echo "Para activar el selector de PHP, añade esto al final de ~/.zshrc (después de la línea de Herd):"
# Las comillas simples son intencionadas: la línea debe copiarse sin expandir.
# shellcheck disable=SC2016
echo '  export PATH="$HOME/.elephant-stack/bin:$PATH"'
