# Elephant Stack

Panel nativo para el entorno de desarrollo PHP/Laravel local en macOS, hecho con Tauri 2 y Svelte 5. No usa Docker ni máquinas virtuales: gestiona los servicios de Homebrew, las versiones de PHP y los logs de tus proyectos, y convive con Herd sin cambiar su configuración.

Las decisiones de diseño están en [`docs/decisions/`](docs/decisions/): el plan de implementación ([00](docs/decisions/00-plan.md)) manda sobre las demás.

## Qué hace

- **Barra de menús.** Siempre disponible, también sin ventana: PHP por defecto, servicios (arrancar, detener, reiniciar todos), sitios de Herd («Abrir en el navegador», «Arrancar lo que necesita»…), Mailpit y «Errores recientes». El icono lleva un punto cuando un servicio falla o hay errores sin ver.
- **Ventana única** con barra lateral (vibrancy y color de acento del sistema):
  - **Panel**: estado general, RAM y accesos rápidos.
  - **Sitios**: los proyectos que sirve Herd y los de tus carpetas, en solo lectura (navegador, editor, terminal, URL, logs y PHP del proyecto).
  - **Servicios**: interruptor, RAM, CPU a partir del 5 %, arranque al iniciar sesión y logs. Homebrew hace una cosa cada vez, así que el resto de controles esperan.
  - **PHP**: la versión por defecto y la de cada proyecto (`.php-version`), y descarga de versiones propias.
  - **Logs**: los de Laravel, nginx y PHP-FPM de todos los proyectos en una sola lista, en directo, con filtros, agrupados por huella e inspector con la traza.
  - **Ajustes**: General, Escenario, Logs y Agentes.
- **Asistente de configuración** en la primera ejecución: elige un escenario (Laravel, Laravel con servicios, terminal…), revisa el plan antes de aplicar nada y deja Elephant listo. «No cambiar nada» no ejecuta ningún comando.
- **Canal de agentes.** La orden `elephant` da a Claude Code (u otro agente) el estado del entorno, los logs y los errores 500, y le deja arrancar servicios o cambiar de PHP pidiendo confirmación con un diálogo de Elephant.

Arrancar o detener un servicio no cambia el arranque al iniciar sesión. Cambiar ese ajuste sí reinicia el servicio si está en marcha, o lo arranca un momento si está parado, porque Homebrew no permite hacerlo de otra forma. El consumo se mide cada 5 s y solo con la ventana visible; la RAM es la suma de RSS del servicio y sus hijos.

## Idiomas

Elephant Stack está en **español** e **inglés**: la ventana, el asistente, el menú de la barra, el de la aplicación, el diálogo de confirmación de los agentes, los errores y la salida de texto de la orden `elephant`.

- **Cómo se elige.** Por defecto sigue a macOS (Ajustes del Sistema ▸ General ▸ Idioma y región): español si el primer idioma preferido es español; inglés con cualquier otro.
- **Cómo se cambia.** En Ajustes ▸ General ▸ Idioma: Sistema, Español o English. Se aplica al momento, también a los menús, y se guarda en `~/.elephant-stack/settings.json` (`"language": "system" | "es" | "en"`). La orden `elephant` lee ese mismo ajuste en cada ejecución.
- **Qué no cambia de idioma.** La salida `--json` y el `structuredContent` de MCP (ids, códigos y campos), y las descripciones del catálogo de operaciones y del plugin de Claude Code, que se quedan en español y los agentes leen igual en cualquier idioma. Por eso `elephant --help` en inglés deja en español la línea de cada operación.

Cómo añadir textos o un idioma nuevo, y el glosario: [`docs/i18n.md`](docs/i18n.md).

## Versiones de PHP

Un pequeño ejecutable `php` (el selector, en `shim/`) decide qué versión usar y se sustituye por ella, sin procesos intermedios:

1. `.php-version` en la carpeta actual o en una superior (`8.4`, `8.4.27` o `8.4@Herd`, que fija el binario de Herd).
2. La versión global elegida en la app (`~/.elephant-stack/global-php`).
3. La versión más reciente disponible.

Si un proyecto pide una versión que no tienes, da error en vez de usar otra sin avisar. Detecta los PHP de Herd y los propios (`~/.elephant-stack/php/<versión>/php`). Solo cubre `php` en la terminal: los sitios servidos por Herd siguen usando su configuración. El asistente añade la línea del `PATH` a `~/.zshrc` si se lo pides, enseñándote antes el cambio.

**De dónde salen.** Las versiones que se instalan desde la vista PHP vienen de [php-builds](https://github.com/cristiancosano/php-builds), que compila PHP estático con static-php-cli. En Ajustes ▸ General se puede usar otra fuente:

- un repositorio de GitHub (`usuario/repo`) que publique igual que php-builds: un `index.json` en la release `catalog` o, como respaldo, releases `php-X.Y.Z` con `php-X.Y.Z-macos-arm64.tar.gz` y su `.sha256` (o `SHA256SUMS`);
- la URL https de un `index.json` con esa misma forma (la de la API de releases de GitHub: `[{tag_name, draft, prerelease, assets: [{name, browser_download_url, size}]}]`).

«Probar» comprueba la fuente antes de guardarla. Esos binarios se ejecutan como tu `php` y el SHA-256 viene de la misma fuente, así que solo protege de descargas corruptas: usa fuentes de confianza.

Con una versión instalada, una más reciente de su misma rama (8.4.27 → 8.4.28) aparece como «Actualizar»: se descarga, pasa a ser la versión por defecto y la de los proyectos que fijaban la anterior, y la anterior se borra. Las versiones de Herd no se listan, aunque el selector las sigue resolviendo.

## Canal de agentes

Hay un único catálogo de operaciones (`crates/elephant-api`, contrato en [`docs/agent-api.json`](docs/agent-api.json)) con dos fachadas:

- **CLI `elephant`**, el canal principal. `elephant <recurso> <verbo>`: `status`, `component list|install`, `service start|stop|restart …`, `php list|use|isolate`, `project list|needs|start-services|create`, `logs sources|tail|search|errors|entry|watch`, `job show`, `doctor` y `guide`. Texto compacto por defecto y `--json` para máquinas. Ninguna salida pasa de 24 KB y los secretos se ocultan (`DB_PASSWORD=••••`).
- **`elephant mcp`**, servidor MCP por stdio generado desde el mismo catálogo, para clientes que solo hablan MCP.

En **Ajustes ▸ Agentes** se activa la orden (`~/.elephant-stack/bin/elephant`, enlazada al binario que viaja con la app) y «Conectar» instala el **plugin de Claude Code** `elephant`: una skill que usa la orden por Bash, sin servidor residente. Su coste fijo es de unos 130 tokens por sesión. La pestaña también enseña los fragmentos de permisos y de configuración MCP para copiarlos; la app nunca escribe la configuración de Claude Code.

Modos: `read_only` (solo lecturas), `confirm` (por defecto: cada cambio que pide un agente abre un diálogo de Elephant, con «Denegar» por defecto) y `trusted` (sin diálogo). Todo queda en `~/.elephant-stack/logs/agent-audit.jsonl`.

## Requisitos

- macOS en Apple Silicon (los binarios se compilan para `aarch64-apple-darwin`) con Homebrew.
- Node 24 y pnpm.
- Rust estable. Con `brew install rustup`, añade `/opt/homebrew/opt/rustup/bin` al `PATH`.

## Desarrollo

```sh
pnpm install
pnpm tauri dev      # compila los sidecars en debug y abre la app
pnpm tauri build    # genera target/release/bundle/macos/Elephant Stack.app
```

**Sidecars.** La app lleva dos binarios en `Contents/MacOS/` (`bundle.externalBin`): `cli` (la orden `elephant`, crate `elephant-cli`) y `php-selector` (el selector `php`, crate `elephant-php`). `scripts/build-sidecars.sh [debug|release]` los compila y los deja en `src-tauri/binaries/`; `pnpm tauri dev` y `pnpm tauri build` lo ejecutan solos. En debug, si faltan, `src-tauri/build.rs` crea un marcador que avisa y sale con 1, y la app usa entonces el binario de cargo de `target/debug/`. En release, que falten es un error.

**Sin la app**, para usar el selector y la orden en la terminal:

```sh
./scripts/install-shim.sh
# Después, al final de ~/.zshrc, tras la línea de Herd:
export PATH="$HOME/.elephant-stack/bin:$PATH"
```

## Estructura

| Ruta | Qué es |
|---|---|
| `crates/elephant-i18n` | Idioma (`Locale`, `locale()`) y las macros `tr!` y `tr_str!`, que comparten core y el motor de logs. |
| `crates/elephant-core` | Núcleo síncrono: Homebrew, componentes, PHP, ajustes, tareas, cerrojo de Homebrew, ocultación de secretos y el asistente (`setup/`). Sin Tauri ni tokio. |
| `crates/elephant-logs` | Motor de logs: descubre proyectos (Herd y tus carpetas), lee hacia atrás, busca, agrupa y sigue el directo. |
| `crates/elephant-api` | Catálogo de operaciones del canal de agentes, política de confirmación y plantillas del plugin. |
| `crates/elephant-cli` | La orden `elephant`. |
| `crates/elephant-mcp` | Fachada MCP (`elephant mcp`), la única que depende de `rmcp`. |
| `shim/` | El selector `php`. |
| `src-tauri/` | La app: estado, barra de menús, menú de la aplicación, vigilancia y comandos. |
| `src/` | Interfaz en Svelte 5: `ui/` (sistema de diseño), `views/`, `lib/` (IPC, router y stores) y `lib/bindings/` (tipos generados desde Rust). |

## Tests y comprobaciones

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
scripts/bindings.sh --check        # los tipos de TS (ts-rs) están al día
pnpm check && pnpm test && pnpm build
scripts/budget.sh                  # presupuestos del bundle (01 §11)
target/debug/elephant schema --check
```

`cargo test` vuelve a escribir `src/lib/bindings/` (sale igual si no cambian los tipos); `scripts/bindings.sh` los regenera desde cero. La CI (`.github/workflows/ci.yml`, macOS arm64) ejecuta todo lo anterior y, si `claude` está instalado, `claude plugin validate --strict` sobre el plugin generado.

Los tests no arrancan ni paran servicios reales ni escriben en tu carpeta personal: usan carpetas temporales y herramientas falsas (`ELEPHANT_STACK_BREW`, `ELEPHANT_STACK_OSASCRIPT`…, solo en debug). Para ver lo que detecta en tu Mac, sin modificar nada:

```sh
cargo test --workspace system_tests -- --ignored --nocapture
```

Más medidas, a mano:

- `scripts/budget.sh ram`, con la app abierta: RAM de la app y de los procesos WebContent (solo lee `ps`).
- `cargo test --release -p elephant-logs --test perf -- --ignored --nocapture --test-threads=1`: presupuestos del motor de logs (02 §10) sobre logs generados.
- `scripts/capturas.sh [es|en|todos]`: capturas de referencia de cada vista, en claro y en oscuro, en [`docs/capturas/`](docs/capturas/); las de inglés llevan el sufijo `-en`. Usa Chrome sin interfaz con el IPC simulado (`src/dev/capturas.ts`), así que los controles nativos de WebKit (interruptores, menús) se ven como los de Chrome.

## Datos

Todo lo de Elephant está en `~/.elephant-stack/`: `settings.json`, `setup.json`, `projects.json` (carpetas y ajustes de logs), `global-php`, `php/`, `bin/`, `jobs/`, `logs/` y `share/claude-plugin/`. Elephant no escribe en la configuración de Herd.
