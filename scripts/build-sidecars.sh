#!/usr/bin/env bash
# Compila los binarios que viajan con la app (C21) y los deja donde Tauri los
# busca: src-tauri/binaries/{cli,php-selector}-<triple>.
#   cli           la orden `elephant` (crate elephant-cli)
#   php-selector  el selector `php` (crate elephant-php)
# Uso: scripts/build-sidecars.sh [debug|release]   (por defecto, release)
set -euo pipefail

profile="${1:-release}"
case "$profile" in
  debug) cargo_flags=() ;;
  release) cargo_flags=(--release) ;;
  *)
    echo "Uso: $0 [debug|release]" >&2
    exit 2
    ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
triple="aarch64-apple-darwin"
out="$root/src-tauri/binaries"

cargo build ${cargo_flags[@]+"${cargo_flags[@]}"} --manifest-path "$root/Cargo.toml" -p elephant-cli -p elephant-php

mkdir -p "$out"
install -m 755 "$root/target/$profile/elephant" "$out/cli-$triple"
install -m 755 "$root/target/$profile/php" "$out/php-selector-$triple"

echo "Sidecars ($profile) en $out:"
ls -l "$out/cli-$triple" "$out/php-selector-$triple"
