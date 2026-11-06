#!/usr/bin/env bash
# Compila el selector de PHP y lo copia a ~/.elephant-stack/bin/php.
# No toca el PATH: hasta añadir esa carpeta a tu shell no tiene ningún efecto.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="$HOME/.elephant-stack/bin"

cargo build --release --manifest-path "$root/Cargo.toml" -p elephant-php

mkdir -p "$target"
install -m 755 "$root/target/release/php" "$target/php"

echo "Instalado en $target/php"
echo "Para activarlo, añade esto al final de ~/.zshrc (después de la línea de Herd):"
# Las comillas simples son intencionadas: la línea debe copiarse sin expandir.
# shellcheck disable=SC2016
echo '  export PATH="$HOME/.elephant-stack/bin:$PATH"'
