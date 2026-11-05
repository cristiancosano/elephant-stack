# Elephant Stack

Panel nativo para el entorno de desarrollo local en macOS, hecho con Tauri 2. No usa Docker ni máquinas virtuales.

## Qué hace

Muestra una sola lista, con cada componente una vez, lo haya instalado quien lo haya instalado:

- **Servicios:** interruptor para arrancar o detener, con la RAM de cada uno. La CPU solo aparece a partir del 5 %. El menú «⋯» permite reiniciar y elegir si arranca al iniciar sesión.
- **Herramientas:** versión y origen, si no es Homebrew (Herd, nvm). Elephant Stack no gestiona lo que instalan otras herramientas.
- **Sin instalar:** sección plegada con botón para instalar con Homebrew.

Arrancar o detener no cambia el arranque al iniciar sesión. Cambiar ese ajuste sí reinicia el servicio si está en marcha, o lo arranca un momento si está parado, porque Homebrew no permite hacerlo de otra forma.

El consumo se mide cada 5 segundos y solo con la ventana visible. La RAM es la suma de RSS del servicio y sus procesos hijos, así que la memoria compartida (por ejemplo, en Postgres) puede contarse más de una vez.

Los componentes conocidos están en `SPECS`, en `src-tauri/src/components.rs`.

## Versiones de PHP

La sección PHP lista las versiones instaladas, y permite elegir la de por defecto y la que usa cada proyecto. Detecta los PHP de Herd (`~/Library/Application Support/Herd/bin/phpNN`) y los propios (`~/.elephant-stack/php/<versión>/php`).

Un pequeño ejecutable `php` decide qué versión usar y se sustituye por ella, sin dejar procesos intermedios. El orden es:

1. `.php-version` en la carpeta actual o en una superior (`8.4`, `8.4.27` o `8.4@Herd`, que fija el binario de Herd).
2. La versión global elegida en la app (`~/.elephant-stack/global-php`).
3. La versión más reciente disponible.

Si un proyecto pide una versión que no tienes, da error en vez de usar otra sin avisar.

Para usarlo en la terminal:

```sh
./scripts/install-shim.sh
# Después, al final de ~/.zshrc, tras la línea de Herd:
export PATH="$HOME/.elephant-stack/bin:$PATH"
```

Solo cubre `php` en la terminal. Los sitios servidos por Herd siguen usando su propia configuración.

## Requisitos

- macOS con Homebrew
- Node y pnpm
- Rust. Con `brew install rustup`, añade `/opt/homebrew/opt/rustup/bin` al `PATH`.

## Uso

```sh
pnpm install
pnpm tauri dev      # desarrollo
pnpm tauri build    # genera Elephant Stack.app
```

## Tests

```sh
cd src-tauri && cargo test
```

Los tests no arrancan ni paran servicios reales. Para ver lo que detecta en tu Mac, sin modificar nada:

```sh
cargo test system_tests -- --ignored --nocapture
```
