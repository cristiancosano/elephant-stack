# 01 · Interfaz: frontend, sistema de diseño, navegación y menú de la barra de menús

- **Estado:** aceptada
- **Fecha:** 2026-11-06
- **Ámbito:** todo lo que se ve. Arquitectura del frontend (`src/`), sistema de diseño, ventana y navegación (Panel, Sitios, Servicios, PHP, Logs, Asistente, Ajustes), menú de la aplicación y menú del icono de la barra de menús. También fija el contrato de IPC que la interfaz necesita del backend.
- **Fuera de ámbito:** qué componentes trae cada escenario del asistente, cómo se leen y parsean los logs, cómo se gestionan nginx y dnsmasq y qué expone el MCP. Lo deciden otros documentos. Aquí solo se fija la forma en que la interfaz los consume.

---

## 1. Decisión

1. **Svelte 5 sin SvelteKit**, con runes, en una **única ventana con barra lateral** y vibrancy nativa solo en esa barra (de la propuesta A).
2. **Rust es la única fuente de verdad.** `AppState` guarda un `Snapshot` cacheado, todas las mutaciones pasan por un `actions.rs` común y cada cambio emite `state:changed`. Lo usan igual la ventana, el menú de la barra y el MCP. Las dos propuestas coinciden en esto; la caché para pintar al instante y la vigilancia barata con `launchctl` vienen de B.
3. **El menú de la barra de menús es un NSMenu nativo construido en Rust**, sin webview. Lo describe una función pura `tray::entries(&Snapshot)` con ids tipados (`TrayAction`) y tests (de B). Se actualiza **en sitio** (`set_text`/`set_checked`/`set_enabled`) cuando solo cambian textos y se reconstruye solo cuando cambia su estructura.
4. **Sistema de diseño «Trunk»**: tokens CSS basados en las HIG de macOS, el **color de acento del sistema inyectado desde Rust** (de B) y el degradado de marca reservado al héroe del Panel y al asistente. Iconos SVG propios.
5. **Tipos generados desde Rust con `ts-rs`** (de B), nunca escritos a mano.
6. **Atajos de teclado en el menú de la aplicación nativo**, en español, definido en Rust. No se usan `keydown` sueltos en JS.

En reposo (sin ventana) la app no tiene ningún webview. Con la ventana abierta tiene exactamente uno.

---

## 2. Comprobaciones hechas antes de decidir

| Afirmación de las propuestas | Resultado | Cómo se ha comprobado |
|---|---|---|
| `transparent` y `windowEffects` exigen `macOSPrivateApi: true` | **Ya no hace falta.** En Tauri 2.12.1 y posteriores, `macOSPrivateApi` no hace nada porque esas APIs están siempre activas. `windowEffects` sigue exigiendo `transparent: true`. Las dos propuestas exageran el riesgo de configuración, aunque la dependencia de API privada sigue ahí por debajo. | `tauri-utils-2.10.2/src/config.rs`: «No-op in Tauri 2.12.1+ because the APIs are always enabled now» y «Window effects. Requires the window to be transparent». El proyecto usa tauri 2.12.5. |
| Se puede refrescar el menú al hacer clic, antes de que se abra (A) | **No es fiable.** En macOS, `tray-icon` emite `Click` en `mouseDown` y llama a `performClick` justo después, que abre el menú en un bucle anidado. El manejador de Tauri recibe el evento a través del bucle de eventos, así que no hay garantía de que se ejecute antes de que el menú se pinte. | `tray-icon-0.25.1/src/platform_impl/macos/mod.rs`, `on_mouse_down` y `on_tray_click`. |
| Los items del menú se pueden modificar sin reconstruirlo | **Sí.** `MenuItem`, `CheckMenuItem` y `Submenu` tienen `set_text`, `set_enabled` y `set_checked`. `TrayIcon` tiene `set_menu`, `set_icon_with_as_template` y `set_title`. | `tauri-2.12.5/src/menu/{normal,check,submenu}.rs` y `src/tray/mod.rs`. |
| La palabra clave CSS `AccentColor` sigue el acento del sistema (B lo pone en duda) | **No lo sigue.** En WebKit devuelve un valor fijo (azul) que no refleja Ajustes del Sistema. Hay que inyectar el acento desde Rust. | WebKit bug 254578 («css AccentColor keyword is not synced») y bug 241866. |
| `prefers-reduced-transparency` sirve como respaldo (A) | **No sirve.** Safari/WebKit no lo implementa. Tampoco hace falta: `NSVisualEffectView` ya respeta «Reducir transparencia» por sí mismo. | web-features explorer (Safari: no soportado, WebKit bug 175497). |
| Svelte 5 funciona con el Vite 8 del repo | **Sí.** `@sveltejs/vite-plugin-svelte@7.3.1` pide `vite ^8` y `svelte ^5.46.4`. La última versión de `svelte` es la 5.57.3 y la de `svelte-check`, la 4.7.8. | Registro npm, consultado hoy. El repo tiene `vite@8.3.5`. |
| Lo que pesa en RAM es el webview, no el framework (B) | **Cierto.** Un proceso `com.apple.WebKit.WebContent` medido en este Mac ocupa unos 39 MB de RSS. Los runtimes de Svelte o de un sistema de signals propio pesan KB. Por eso importa más el número de webviews que el framework. | `ps -axo rss,comm`. |
| `launchctl list` es barato | **Sí.** Tarda unos 10 ms, frente a los ~300 ms de `brew services list`. Devuelve PID y último código de salida de `homebrew.mxcl.*`. | `time launchctl list`. |
| Tauri crea un menú de aplicación por defecto en macOS | **Sí** (`enable_macos_default_menu`, en inglés). Se sustituye por uno propio en español. | `tauri-2.12.5/src/app.rs`. |
| «`replaceChildren` reconstruye todas las filas cada 5 s» (B) | **Inexacto.** El sondeo de 5 s solo llama a `renderUsage`, que cambia textos. El repintado completo lo provocan `operate`, `install` y `reloadServices` (por ejemplo, cuando desaparece un PID), y ahí sí se pierden el foco y el estado del switch. El problema es real, pero menos grave de lo que dice B. | `src/main.ts`. |
| El CSS actual ya usa `-apple-system-control-accent` en el foco | **Sí** (`styles.css`, `:focus-visible`). No es estándar y no está documentado. Se sustituye por el acento inyectado. | `src/styles.css`. |

---

## 3. Qué se toma de cada propuesta y qué se descarta

### 3.1 Svelte 5 en lugar de TypeScript sin framework con signals propios (se descarta esa parte de B)

- La propia propuesta B reconoce que su ventaja en tamaño es pequeña: el runtime de Svelte ocupa ~7-10 KB gz frente a un proceso WebContent de ~40 MB. El criterio que decide es la **mantenibilidad**: el visor de logs (filtros, lista virtual, inspector, seguir la cola), el asistente de cinco pasos con estado y progreso, y siete vistas.
- La opción B obliga a escribir y mantener un mini framework (`signal`/`computed`/`effect`, `reconcile` por clave, contrato `mount`/`unmount`, transiciones con `@starting-style` y View Transitions). Eso es exactamente lo que Svelte ya resuelve y prueba, con menos código del proyecto.
- Svelte 5 compila a JS imperativo con reactividad fina, sin DOM virtual ni hidratación. Actualizar la RAM cada 5 s toca un nodo de texto, igual que hoy `renderUsage`, pero sin selectores `[data-id]`.
- `svelte-check` comprueba la accesibilidad al compilar (etiquetas, roles, `alt`, manejo de teclado), lo que va directo al criterio de accesibilidad.
- `svelte/transition` y `svelte/animate` respetan `prefers-reduced-motion` desde un único sitio. No dependen de View Transitions ni de `@starting-style`, así que no obligan a subir la versión mínima de macOS.
- Se descartan React y Vue: suman runtime y DOM virtual sin aportar nada aquí. Se descarta SvelteKit: router, SSR y adaptadores sobran en una app de escritorio.

### 3.2 Una única ventana con barra lateral en lugar de cuatro ventanas (se descarta esa parte de B)

- Cada ventana es un WebContent más. Con una sola, el máximo es **un** webview y cambiar de vista es instantáneo, sin arrancar ningún proceso.
- La memoria del visor de logs se libera igual al salir de la vista, porque el componente se desmonta y su búfer se suelta, y por completo al cerrar la ventana, que ya se destruye (`lib.rs`).
- Una ventana evita capabilities por ventana, enlaces profundos entre ventanas y la coordinación del foco y del Dock (`show_in_dock` hoy supone una sola ventana).
- Ajustes pasa a ser una vista de la barra lateral (⌘,), como en Ajustes del Sistema, y no una ventana aparte. El asistente ocupa la misma ventana sin barra lateral.
- Si más adelante se quisiera ver los logs en una ventana aparte, el componente `Logs.svelte` se monta tal cual en una segunda entrada de Vite. La arquitectura no lo impide. **No se hace ahora.**

### 3.3 Se adopta de B

- `tray.rs` con una función pura y `TrayAction` con `FromStr`/`Display` y tests. `actions.rs` como única implementación de cada acción.
- Un `Snapshot` cacheado en Rust para pintar al abrir sin esperar a `brew`.
- Vigilancia con `launchctl list` en lugar de `brew services list` para detectar caídas.
- El acento del sistema inyectado desde Rust. El menú de la aplicación completo con Edición, para que funcionen ⌘C, ⌘A y ⌘F en el visor de logs.
- Tipos con `ts-rs`. Lotes por `Channel` para los logs. Iconos propios, porque la licencia de SF Symbols no permite incrustarlos.
- Presupuestos medibles de RAM y de bundle como criterios de aceptación.

### 3.4 Se adopta de A

- Svelte 5, la barra lateral y la vibrancy `sidebar` solo en la barra lateral, con el contenido opaco.
- El router por hash con `nav:go` desde Rust. El asistente dentro de la ventana. Las vistas de Logs y del asistente tal como se describen más abajo.
- La escala tipográfica y la curva de movimiento.

### 3.5 Se descarta de las dos

- **Violeta propio como acento** (A). Las apps de Apple usan el acento del usuario. La marca solo aparece en el degradado del héroe.
- **`tauri-specta`** (A). Sigue en RC en Tauri 2 y acopla los comandos a su macro. `ts-rs` solo actúa en los tests y genera interfaces planas.
- **`tauri-plugin-notification`** (A). Pide permiso de notificaciones y añade una dependencia. Los fallos de las acciones del menú se ven en el propio menú (fila de aviso e icono con punto) y en un toast si la ventana está abierta. Se puede revisar más adelante.
- **Modo «ventana compacta»** (A). El menú de la barra ya cubre el uso rápido.
- **`prefers-reduced-transparency`** (A): no existe en WebKit.
- **View Transitions, `@starting-style`, `popover` y `<dialog>` como base de la UI** (B): Svelte ya lo cubre. Se pueden usar puntualmente si ayudan, pero nada depende de ellos.
- **Refrescar el menú al hacer clic como mecanismo principal** (A): no es fiable (ver §2).

---

## 4. Arquitectura del frontend

### 4.1 Dependencias

Solo estas, con versiones exactas en `package.json`:

| Paquete | Tipo | Para qué |
|---|---|---|
| `svelte@^5.57` | dependencia | compilador y runtime |
| `@sveltejs/vite-plugin-svelte@^7.3` | desarrollo | integración con Vite 8 |
| `svelte-check@^4.7` | desarrollo | tipos y accesibilidad (`--fail-on-warnings`) |
| `vitest` | desarrollo | tests de la lógica pura de `src/lib/` |

No se usan Tailwind, librerías de componentes, icon fonts, librerías de listas virtuales ni CSS-in-JS. La CSP actual no cambia: Svelte no usa `eval`, y los `style:` en línea ya están cubiertos por `style-src 'unsafe-inline'`.

En Rust: `ts-rs` (solo en desarrollo, para exportar tipos) y `objc2-app-kit` y `objc2-foundation` como dependencias directas para el acento y la apariencia. Tauri ya las compila en macOS, así que no alargan la compilación. Se usan las mismas versiones que resuelve `tauri` en `Cargo.lock`.

### 4.2 Estructura

```
index.html                 # solo <div id="app"> y el script; la CSS va importada desde main.ts
src/
  main.ts                  # import "./ui/tokens.css"; mount(App, { target: app })
  App.svelte               # decide Wizard o Shell; Toaster global
  Shell.svelte             # Sidebar + <main> con la vista del router
  lib/
    bindings/              # GENERADO por ts-rs. No se edita a mano
    ipc.ts                 # el ÚNICO archivo que importa invoke/Channel/listen
    events.ts              # suscripción a state:changed, nav:go, ui:command
    format.ts              # memoria, CPU, tiempo relativo, plurales (con tests)
    router.svelte.ts       # ruta actual, parse/serialize del hash (con tests)
    stores/
      app.svelte.ts        # snapshot ($state) y derivados ($derived)
      usage.svelte.ts      # sondeo de RAM/CPU solo con la ventana visible
      logs.svelte.ts       # búfer circular, filtros y suscripción
      toasts.svelte.ts
    virtual.ts             # cálculo puro de la ventana visible (con tests)
  ui/                      # sistema de diseño, sin lógica de dominio
    tokens.css  base.css
    Button  IconButton  Switch  SegmentedControl  Select  SearchField
    Group  Row  Badge  StatusDot  Gauge  ProgressRing  Skeleton
    EmptyState  Sheet  Toast  Inspector  VirtualList  Icon
    icons/                 # un .svg por icono, importados como texto por Icon.svelte
  views/
    Dashboard.svelte  Sites.svelte  Services.svelte  Php.svelte  Settings.svelte
    logs/Logs.svelte  logs/Filters.svelte  logs/Row.svelte  logs/Detail.svelte
    wizard/Wizard.svelte  wizard/Welcome.svelte  wizard/Scenario.svelte
          wizard/Customize.svelte  wizard/Apply.svelte  wizard/Done.svelte
src-tauri/src/
  state.rs                 # AppState, Snapshot, revision, emit
  actions.rs               # acciones compartidas por comandos, menú y MCP
  tray.rs                  # entries(&Snapshot), TrayAction, aplicar al NSMenu
  app_menu.rs              # menú de la aplicación en español con atajos
  appearance.rs            # acento del sistema (macOS)
  watch.rs                 # vigilancia con launchctl
```

`Logs` y `Wizard` se cargan con `import()` dinámico: no entran en el chunk inicial.

### 4.3 Reglas

1. **Rust es la fuente de verdad.** El frontend no guarda estado de dominio propio: solo una copia del último `Snapshot` y el estado de la UI (ruta, filtros, selección).
2. **Solo `lib/ipc.ts` importa `@tauri-apps/api/core` y `/event`.** Ningún componente llama a `invoke`. `@tauri-apps/api/menu` se puede usar en las vistas para los menús contextuales nativos, como hoy en `openMenu`.
3. **Los tipos salen de `lib/bindings/`.** Se generan con `cargo test export_bindings` y CI falla si hay diferencias sin commitear.
4. **Ningún sondeo con la ventana oculta.** `usage.svelte.ts` para el intervalo con `visibilitychange` y lo reanuda al volver. Se mantienen los 5 s actuales.
5. **Un componente de `ui/` no conoce el dominio.** Recibe props y emite callbacks. Las vistas componen.
6. **Nada de `:hover` que cambie el cursor**, ni `cursor: pointer`. Comportamiento nativo (§6.6).
7. **Persistencia de la UI** (última ruta, filtros de logs, barra lateral plegada) en `localStorage`, con cada acceso dentro de `try/catch`. Sobrevive a que se destruya la ventana. Si falla, se usan los valores por defecto. Nada de dominio va ahí.

### 4.4 Contrato de IPC

Los tipos se definen en Rust con `#[derive(Serialize, TS)]` y se exportan.

```rust
// state.rs
pub struct Snapshot {
    pub revision: u64,              // crece en cada cambio; el frontend descarta los antiguos
    pub refreshed_at_ms: u64,
    pub components: Vec<Component>, // como hoy (components.rs)
    pub php: PhpStatus,             // como hoy (php.rs)
    pub sites: Vec<Site>,           // vacío hasta que exista la gestión de sitios
    pub busy: Vec<Busy>,            // { target: "nginx", label: "Reiniciando…" }
    pub problems: Vec<Problem>,     // { id, target, message, kind: ServiceError | ActionFailed }
    pub unseen_log_errors: u32,     // lo rellena el módulo de logs; 0 si no existe
    pub settings: Settings,
}
```

`Settings` deja de ser `Copy`, porque crecerá con listas como las carpetas aparcadas. Se añade, con `#[serde(default)]`:

- `onboarded: bool` (`false` por defecto).
- `appearance: "system" | "light" | "dark"`.
- `show_php_in_menu_bar: bool` (`false`).
- `watch_services: bool` (`true`).

**Comandos que usa la interfaz:**

| Comando | Devuelve | Notas |
|---|---|---|
| `get_snapshot()` | `Snapshot` | Devuelve **al instante** el cacheado. Si tiene más de 2 s, lanza `refresh` en segundo plano, cuyo resultado llega por `state:changed`. |
| `refresh()` | `()` | Fuerza `components::list` + `php::status`. |
| `service_operation(name, operation)` | `Result<(), String>` | El estado nuevo llega por evento. El error vuelve como `Err` para mostrar un toast. |
| `services_bulk(operation)` | `Result<(), String>` | Reiniciar o detener todos, igual que en el menú. |
| `install_component(id, progress: Channel<Progress>)` | `Result<(), String>` | Progreso por canal (líneas de `brew`). |
| `set_global_php(version)` / `set_project_php(dir, version)` | `Result<(), String>` | Como hoy, pero por `actions.rs`. |
| `process_usage(pids)` | `Vec<Usage>` | Como hoy. Además guarda el último valor en `AppState` para la cabecera del menú. |
| `get_settings()` / `set_settings(settings)` | `Settings` | `set_settings` también emite `state:changed` y aplica la apariencia a la ventana. |
| `take_pending_route()` | `Option<String>` | Ruta pedida por `nav:go` antes de que existiera la ventana. Se consume una vez al montar. |
| `appearance()` | `Appearance { accent: String, accent_text: String }` | Hex `#rrggbb` de `NSColor.controlAccentColor` (y de `selectedContentBackgroundColor` para el texto sobre el acento). |

**Los comandos de otros dominios** (asistente, logs, sitios, MCP) los define su propia decisión. La interfaz espera esta forma y se adapta a los nombres finales:

- Asistente: `scenarios()` → `Vec<Scenario>`, `plan_for(selection)` → `Plan` (sin ejecutar nada) y `apply_plan(plan, progress: Channel<StepProgress>)`.
- Logs: `log_sources()`, `log_query(filter, before, limit)` → `LogPage`, `log_subscribe(filter, channel: Channel<LogBatch>)` → `u32` y `log_unsubscribe(id)`. Los lotes llegan cada ≤100 ms, nunca línea a línea.

**Eventos de Rust al frontend:**

| Evento | Carga | Emisor |
|---|---|---|
| `state:changed` | `Snapshot` completo (pocos KB) | cada mutación, venga de la ventana, del menú, del MCP o de `watch.rs` |
| `nav:go` | `{ route: string }` | menú de la barra, menú de la aplicación, MCP |
| `ui:command` | `{ command: "find" \| "refresh" \| "toggle-sidebar" }` | menú de la aplicación |
| `appearance:changed` | `Appearance` | al recuperar el foco, si el acento ha cambiado |

Se envía el snapshot completo, no deltas: es pequeño, simplifica el frontend y la `revision` evita aplicar uno antiguo.

### 4.5 Estado en el frontend

```ts
// lib/stores/app.svelte.ts
export const app = $state<{ snapshot: Snapshot | null; error: string | null }>({ snapshot: null, error: null });
export const services = $derived(/* componentes instalados de tipo service, los que están en marcha primero */);
export const runningCount = $derived(/* … */);
export function busy(target: string): Busy | undefined { /* … */ }
```

- Arranque: `get_snapshot()` pinta en el primer frame que hay datos. Hasta entonces se muestran esqueletos, nunca un spinner a pantalla completa.
- `events.ts` se suscribe a `state:changed` y sustituye el snapshot si `revision` es mayor.
- Las filas usan `{#each … (component.id)}` con clave, así que al refrescar no se pierden ni el foco ni el estado del switch.

### 4.6 Router

- Hash, sin dependencias: `#/panel`, `#/sitios`, `#/servicios`, `#/php`, `#/logs?project=acme&source=laravel&level=error`, `#/ajustes/general`, `#/asistente`.
- `router.svelte.ts` expone `route` (`$state`), `go(path)` y `parse`/`serialize` puros con tests.
- `nav:go` llama a `go`. Si la ventana no existe, Rust la crea con `open_main_window` y guarda la ruta pendiente. La ventana la pide con `take_pending_route()` al montar. Así no se pierde ningún evento emitido antes de que el webview escuche.
- Cambio de vista: `{#key route.view}` con `fade` de 120 ms y desplazamiento vertical de 4 px. Nunca deslizamientos laterales.

---

## 5. Ventana

### 5.1 `tauri.conf.json`

```json
"windows": [{
  "label": "main",
  "title": "Elephant Stack",
  "width": 920, "height": 600,
  "minWidth": 760, "minHeight": 500,
  "titleBarStyle": "Overlay",
  "hiddenTitle": true,
  "transparent": true,
  "trafficLightPosition": { "x": 18, "y": 20 },
  "windowEffects": { "effects": ["sidebar"], "state": "followsWindowActiveState" }
}]
```

- Se quitan `maxWidth` y `maximizable: false`: el visor de logs necesita poder crecer.
- `macOSPrivateApi` **no** se añade, porque en Tauri ≥ 2.12.1 no hace nada (§2).
- `lib.rs` sigue destruyendo la ventana al cerrarla. Esa decisión no se toca.

### 5.2 Fondo y vibrancy

- `html` y `body` tienen fondo `transparent`. La barra lateral es transparente y deja ver el material `sidebar` de `NSVisualEffectView`.
- `<main>` pinta `var(--window)` opaco. **La vista de logs nunca va sobre vibrancy.**
- «Reducir transparencia» lo resuelve el propio `NSVisualEffectView`, que se vuelve opaco. No hace falta CSS.
- Si la vibrancy no estuviera disponible, la barra lateral usa `--sidebar-fallback` mediante la clase `no-vibrancy`, que Rust añade si `set_effects` falla.

### 5.3 Apariencia y acento

- `appearance.rs` lee `NSColor.controlAccentColor` y `selectedContentBackgroundColor` convertidos a sRGB.
- `open_main_window` añade un `initialization_script` que, antes de pintar, fija `--system-accent`, `--system-accent-text` y `data-theme` en `<html>`. Así no hay parpadeo.
- Al recuperar el foco, Rust vuelve a leer el acento y emite `appearance:changed` si ha cambiado. No hace falta observar `NSNotification`.
- Apariencia forzada (Ajustes → General → Apariencia): `window.set_theme(Some(Theme::Light|Dark))` para que el material nativo coincida, más `data-theme` para los tokens. «Sistema» = `set_theme(None)` sin `data-theme`.

---

## 6. Sistema de diseño «Trunk»

Base de Apple (listas agrupadas tipo Ajustes del Sistema, separadores finos, SF Pro, acento del usuario) y un único gesto de marca al estilo CleanMyMac: el héroe degradado del Panel y del asistente.

### 6.1 Tokens (`src/ui/tokens.css`)

```css
:root {
  color-scheme: light dark;
  --font: -apple-system, BlinkMacSystemFont, sans-serif;
  --mono: ui-monospace, "SF Mono", Menlo, monospace;

  /* Acento: lo inyecta Rust; el valor de respaldo es systemBlue */
  --accent: var(--system-accent, #007aff);
  --accent-text: #fff;
  --accent-soft: color-mix(in srgb, var(--accent) 16%, transparent);
  --focus-ring: color-mix(in srgb, var(--accent) 50%, transparent);

  /* Texto y rellenos (label colors de AppKit) */
  --label: rgb(0 0 0 / .85);  --label-2: rgb(0 0 0 / .55);  --label-3: rgb(0 0 0 / .28);
  --fill: rgb(0 0 0 / .05);   --fill-2: rgb(0 0 0 / .08);   --fill-3: rgb(0 0 0 / .12);
  --separator: rgb(0 0 0 / .10);

  /* Superficies */
  --window: #f5f5f7;          /* contenido, opaco */
  --group: #ffffff;           /* grupos y tarjetas sobre --window */
  --sidebar-fallback: #e8e8ed;
  --sidebar-selected: rgb(0 0 0 / .08);   /* selección inactiva */

  /* Estado (colores del sistema) */
  --green: #34c759; --orange: #ff9f0a; --red: #ff3b30; --blue: #007aff; --purple: #af52de;

  /* Niveles de log */
  --lvl-debug: var(--label-3); --lvl-info: var(--blue); --lvl-notice: #5ac8fa;
  --lvl-warning: var(--orange); --lvl-error: var(--red); --lvl-critical: var(--purple);
  /* HTTP */
  --http-2xx: var(--green); --http-3xx: var(--label-2); --http-4xx: var(--orange); --http-5xx: var(--red);

  /* Marca: solo en el héroe del Panel y en el asistente */
  --hero: linear-gradient(135deg, #5e5ce6 0%, #8e5cf0 55%, #bf5af2 100%);
  --hero-warn: linear-gradient(135deg, #ff9f0a 0%, #ff6b3d 100%);
  --hero-bad: linear-gradient(135deg, #ff453a 0%, #d70050 100%);
  --on-hero: #fff;

  /* Espaciado (rejilla de 4) */
  --s-1: 4px; --s-2: 8px; --s-3: 12px; --s-4: 16px; --s-5: 20px; --s-6: 24px; --s-8: 32px; --s-10: 40px;
  /* Radios */
  --r-xs: 4px; --r-sm: 6px; --r-md: 10px; --r-lg: 14px; --r-pill: 999px;
  /* Elevación */
  --hairline: 0 0 0 .5px var(--separator);
  --shadow-1: var(--hairline), 0 1px 2px rgb(0 0 0 / .05);
  --shadow-pop: 0 0 0 .5px rgb(0 0 0 / .12), 0 10px 30px rgb(0 0 0 / .18);
  /* Movimiento */
  --ease: cubic-bezier(.32, .72, 0, 1);
  --d-1: 120ms; --d-2: 200ms; --d-3: 340ms;
}

@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { /* bloque oscuro */ } }
:root[data-theme="dark"] { /* el mismo bloque oscuro */ }
/* Bloque oscuro:
   --label: rgb(255 255 255 / .88); --label-2: rgb(255 255 255 / .55); --label-3: rgb(255 255 255 / .28);
   --fill: rgb(255 255 255 / .06); --fill-2: rgb(255 255 255 / .10); --fill-3: rgb(255 255 255 / .14);
   --separator: rgb(255 255 255 / .10); --window: #1e1e20; --group: #2a2a2d;
   --sidebar-fallback: #252528; --sidebar-selected: rgb(255 255 255 / .10);
   --green: #30d158; --orange: #ff9f0a; --red: #ff453a; --blue: #0a84ff; --purple: #bf5af2;
   --hero: linear-gradient(135deg, #4b49c9 0%, #7444d6 55%, #9b3fd1 100%); */

@media (prefers-contrast: more) {
  :root { --separator: rgb(0 0 0 / .35); --label-2: rgb(0 0 0 / .75); --label-3: rgb(0 0 0 / .55); }
  /* y su equivalente en oscuro */
}
@media (prefers-reduced-motion: reduce) { :root { --d-1: 0ms; --d-2: 0ms; --d-3: 0ms; } }
```

El bloque oscuro se escribe **dos veces literalmente** (`@media` y `[data-theme="dark"]`), o se genera con un `@mixin` de PostCSS si algún día se añade. No se introduce PostCSS solo para esto.

### 6.2 Tipografía (tamaños de AppKit)

| Token | Tamaño/interlínea | Peso | Uso |
|---|---|---|---|
| `--t-hero` | 26/32 | 700 | estado del héroe y título del asistente |
| `--t-title` | 17/22 | 600 | título de vista en la barra superior |
| `--t-headline` | 13/16 | 600 | cabecera de grupo, nombre en fila |
| `--t-body` | 13/16 | 400 | texto general |
| `--t-callout` | 12/15 | 400 | secundarios |
| `--t-caption` | 11/13 | 500 | badges, secciones de la barra lateral (`--label-2`) |
| `--t-log` | 11.5/20 | 400 mono | filas de log |

Todas las cifras (RAM, CPU, contadores, horas) llevan `font-variant-numeric: tabular-nums`.

### 6.3 Métricas de layout

- Barra lateral: **200 px**, plegable con ⌃⌘S (atajo estándar de macOS). Los items miden 28 px, con 10 px de margen horizontal, radio `--r-sm` y un icono de 16 px.
- Barra superior de cada vista: 52 px, `data-tauri-drag-region`. Lleva el título a la izquierda y las acciones de la vista a la derecha. Si la barra lateral está plegada, se deja a la izquierda un hueco de 78 px para los botones de ventana.
- Contenido: margen de 24 px y ancho máximo de 760 px centrado en todas las vistas salvo Logs, que ocupa todo el ancho.
- Grupos: fondo `--group`, radio `--r-md` y `--shadow-1`. Filas de 36 px (44 px con subtítulo) con separadores de 0,5 px sangrados hasta el texto. Separación entre grupos de 20 px.

### 6.4 Componentes de `ui/` (contrato mínimo)

| Componente | Props clave | Comportamiento |
|---|---|---|
| `Button` | `variant: "primary" \| "secondary" \| "plain" \| "destructive"`, `size: "regular" \| "large"`, `busy` | Alto de 22 px en regular y 28 en large, radio `--r-sm`. Primary usa `--accent` sobre `--accent-text`. Con `busy`, pone un `ProgressRing` de 12 px y desactiva el botón sin cambiar su ancho. |
| `IconButton` | `icon`, `label` (obligatoria, va a `aria-label` y a `title`) | Bezel `--fill-2` solo al pasar el ratón, como los botones de la barra de herramientas de macOS. |
| `Switch` | `checked`, `disabled`, `label` | `<input type="checkbox" switch>` nativo, como hoy. |
| `SegmentedControl` | `options`, `value` | `role="radiogroup"` y flechas ← → para moverse. El segmento activo es una píldora blanca (gris en oscuro) con `--shadow-1` y animación de posición en 200 ms. |
| `Select` | `options`, `value` | Abre un menú **nativo** (`@tauri-apps/api/menu`). El botón tiene aspecto de pop-up de macOS. |
| `SearchField` | `value`, `placeholder`, `regex` | Lupa, botón de borrar y ⌘F (vía `ui:command`). Con `regex` inválida pinta el borde en `--red` y avisa con `aria-invalid`. |
| `Group` / `Row` | `title`, `footer` / `title`, `subtitle`, `leading`, `trailing` | Las listas agrupadas de Ajustes del Sistema. |
| `Badge`, `StatusDot` | `tone` | `StatusDot`: verde en marcha, gris parado, rojo error, ámbar ocupado (con pulso, que desaparece con reducir movimiento). |
| `Gauge` | `value`, `max`, `label` | Anillo SVG de 120 px con trazo de 10 y extremos redondeados. Al cambiar de valor, el arco se interpola con `tweened` en 340 ms. |
| `EmptyState` | `icon`, `title`, `message`, `action` | Icono de 40 px en `--label-3`, título `--t-headline` y una sola acción. |
| `Skeleton` | `lines` | Brillo suave, que se desactiva con reducir movimiento. |
| `Sheet` | `open`, `title` | Panel modal que baja desde la barra superior con `--ease` en 340 ms, con foco atrapado y Esc para cerrar. |
| `Toast` | `tone`, `message`, `action` | HUD abajo y al centro. Info y éxito se cierran solos a los 4 s. Los errores se quedan con un botón de cerrar. `role="status"` o `role="alert"`. |
| `Inspector` | `open`, `width` | Panel derecho plegable de 320 px, redimensionable entre 260 y 520. |
| `VirtualList` | `count`, `rowHeight`, `row` (snippet), `follow` | Ver §7.5. |
| `Icon` | `name`, `size: 16 \| 20` | SVG en línea con `currentColor`. |

### 6.5 Iconos

- Juego propio en `src/ui/icons/`, en rejilla de 16 y 20 px con trazo de 1,5 y extremos redondeados, para emparejar con SF Symbols regular sin copiarlos.
- Hacen falta unos 30: panel, sitios, servicios, php, logs, ajustes, play, stop, restart, more, search, filter, open, folder, terminal, editor, copy, check, x, warning, error, info, lock, globe, mail, database, cache, chevron-right, chevron-down, sidebar.
- No se incrustan SF Symbols, porque su licencia no lo permite.

### 6.6 Comportamiento nativo (`base.css`)

- `cursor: default` en todo. `user-select: none` salvo en `.selectable` (mensajes de log, rutas, versiones).
- Foco solo con `:focus-visible`: `outline: 3px solid var(--focus-ring); outline-offset: 1px`.
- Las filas de lista **no** se iluminan al pasar el ratón. Solo hay estado de selección.
- Scrollbars overlay finas (`::-webkit-scrollbar` de 8 px, pulgar `--fill-3`).
- Menús contextuales siempre nativos (clic derecho y botón ⋯).
- Sin `title=` como tooltip decorativo. Solo en iconos sin texto y en rutas recortadas.
- Las cifras que cambian no hacen saltar el layout, gracias a las cifras tabulares y anchos mínimos.

---

## 7. Navegación y vistas

### 7.1 Barra lateral

```
[● ● ●]
 Elephant Stack
 3 en marcha · 412 MB                     (--t-callout, --label-2)

 Panel                                ⌘1
 Sitios                     12        ⌘2
 Servicios                 3/5        ⌘3
 PHP                       8.4        ⌘4
 Logs                    ● 2          ⌘5   (punto rojo: errores sin ver)

 Ajustes                              ⌘,
 ──────────────────────── (pie)
 MCP · activo                                (solo si el MCP existe)
```

- `<nav aria-label="Secciones">`, con items `<a href="#/…" aria-current="page">`. Se navega con ↑ y ↓ cuando la lista tiene el foco.
- Selección: `--accent-soft` con el icono en `--accent` cuando la ventana está activa, y `--sidebar-selected` cuando no lo está (`:root[data-window-inactive]`, que Rust marca con el evento de foco).
- Los atajos no se pintan en la barra lateral: están en el menú Visualización, como en las apps de Apple.

### 7.2 Panel (`#/panel`)

- **Héroe** (alto de 168 px, radio `--r-lg`, fondo `--hero`, que pasa a `--hero-warn` si algo está ocupado o detenido de forma inesperada, o a `--hero-bad` si hay un `problem`):
  - A la izquierda, el estado en `--t-hero` («Todo en marcha», «mysql se ha detenido» o «Nada en marcha») y debajo «3 servicios · PHP 8.4 · 412 MB».
  - A la derecha, un `Gauge` con la RAM total del stack en blanco sobre el degradado.
  - Una sola acción contextual: «Ver problema» o «Arrancar servicios».
- **Rejilla de 2×2 tarjetas** (`--group`, radio `--r-md`):
  - PHP por defecto, con `SegmentedControl` hasta 4 versiones y `Select` a partir de 5.
  - Servicios, con switches rápidos de los que tienen autoarranque o están en marcha.
  - Errores recientes: las 3 últimas entradas de nivel error o superior, cada una enlazada a `#/logs?…`. Si no hay ninguna, un `EmptyState` compacto: «Sin errores en la última hora».
  - Mailpit: abrir, y número de correos si su API lo da.
- **Sitios recientes**, solo cuando existan sitios.

### 7.3 Servicios (`#/servicios`)

Es la vista actual pulida, en un `Group`:

- Cada fila: `StatusDot`, nombre (`--t-headline`), secundario con versión, origen si no es Homebrew, «En marcha · 84 MB · 12 % CPU» o «Al iniciar sesión», el `Switch` y un `IconButton` ⋯ con menú nativo (Reiniciar, Arrancar al iniciar sesión, Ver logs, Mostrar en Finder, más la línea informativa de versión y procesos, como hoy).
- Mientras hay una operación en curso, la fila pone «Aplicando…» y un `ProgressRing`, y el resto de switches se desactivan (Homebrew admite una sola operación a la vez).
- Los que están en marcha van primero, con `animate:flip` de 200 ms al reordenar.
- Grupo «Herramientas» (php, composer, node…) y un grupo plegable «Sin instalar (n)» con botón «Instalar» y progreso por canal.
- La CPU solo se muestra a partir del 5 %, como hoy.

### 7.4 PHP (`#/php`)

- Grupo de versiones: radio de versión por defecto, `8.4.27`, origen («Elephant Stack», «Herd»), badge «Por defecto» y botón «Usar en proyecto…» (abre el diálogo de carpeta, como hoy).
- Grupo «Proyectos con .php-version», cuando el backend los detecte en las carpetas aparcadas.
- Acción de la barra superior: «Descargar versión…» (php-builds), cuando exista.

### 7.5 Logs (`#/logs`)

Tres zonas, como Console.app pero más amable. Ocupa todo el ancho y tiene fondo opaco.

1. **Barra de filtros** (44 px, bajo la barra superior):
   - `SearchField` con texto o regex (⌘F).
   - `Select` de proyecto (Todos o cada sitio, con un contador rojo de errores).
   - `SegmentedControl` de fuente: Todas · HTTP · Laravel · PHP-FPM · Servicios.
   - Chips de nivel con su color: Debug, Info, Warning, Error+.
   - Rango temporal: 15 min · 1 h · Hoy · Todo.
   - A la derecha, un botón conmutable «En vivo» y otro «Limpiar vista».
   - Los filtros se reflejan en el hash, así que el menú de la barra y el MCP pueden abrir una vista filtrada.
2. **Lista virtual** (`VirtualList`):
   - Filas de **altura fija de 20 px**. Las entradas multilínea (excepciones de Laravel) ocupan **una** fila con el mensaje y un indicador «+42 líneas». La traza completa se ve en el inspector. Por eso la altura fija no tiene excepciones.
   - Implementación propia de unas 100 líneas. `lib/virtual.ts` calcula `first`, `last` y `offset` de forma pura y con tests. El componente pinta las filas visibles más 20 de margen arriba y abajo, posicionadas con `transform: translateY` dentro de un contenedor del alto total.
   - Columnas: hora (`HH:mm:ss.SSS`, tabular), pastilla de nivel, proyecto, y el mensaje. En HTTP, el mensaje se sustituye por método, ruta, estado coloreado por `--http-*` y duración.
   - **Seguir la cola** solo si estás abajo del todo, como en Terminal. Al subir, se desactiva y aparece la píldora flotante «N nuevas ↓».
   - Búfer en JS: un **anillo de 10 000 entradas** con las más antiguas descartadas. Al llegar arriba se pide más historial con `log_query(before)` y se mantiene la posición de scroll.
   - Al salir de la vista se llama a `log_unsubscribe` y se suelta el búfer.
   - Accesibilidad: `role="grid"` con `aria-rowcount` total y `aria-rowindex` por fila. ↑ y ↓ mueven la selección, Intro abre el inspector, ⌘C copia la entrada seleccionada y Esc cierra el inspector.
3. **Inspector** (320 px, plegable con ⌥⌘I):
   - Entrada completa con texto seleccionable.
   - Contexto JSON formateado con resaltado propio mínimo (claves, cadenas y números con tokens). Sin librería.
   - Traza plegable, con los marcos `vendor/` atenuados.
   - Acciones «Abrir en el editor» (`archivo:línea` mediante `phpstorm://` o `vscode://` según el ajuste), «Copiar» y «Copiar para el asistente de IA» (texto plano con el contexto).

La interfaz no depende de si los logs los lee un tail propio o una herramienta externa: solo del contrato de §4.4.

### 7.6 Sitios (`#/sitios`)

- Lista agrupada: dominio `.test`, ruta (seleccionable y recortada por el centro), PHP del proyecto (un `Select` que escribe `.php-version`) y candado HTTPS.
- Menú ⋯: Abrir en el navegador, Abrir en el editor, Abrir terminal, Copiar URL, Ver logs (`#/logs?project=…`) y Dejar de servir.
- Mientras no exista la gestión de sitios, `EmptyState`: «Tus sitios los sirve Herd por ahora», con la acción «Configurar con el asistente».

### 7.7 Ajustes (`#/ajustes/<pestaña>`)

- `SegmentedControl` arriba: General · PHP · Sitios · Logs · MCP · Avanzado. Las filas son de tipo Ajustes del Sistema y los cambios se guardan **al instante**, sin botón «Guardar».
- **General**: Seguir en la barra de menús al cerrar (el ajuste actual), Apariencia (Sistema/Claro/Oscuro), Mostrar la versión de PHP junto al icono, Vigilar servicios en segundo plano, Editor para abrir archivos y «Volver a ejecutar el asistente…».
- El resto de pestañas las rellenan las decisiones de cada dominio con los mismos componentes.

### 7.8 Asistente (`#/asistente`)

- Se muestra a ventana completa (sin barra lateral) si `settings.onboarded == false`, o desde Ajustes. Fondo `--window` con el `--hero` en una banda superior de 200 px, que se desvanece hacia abajo.
- Indicador de cinco puntos arriba. Atrás (secundario) y Continuar (primario, ↵) abajo a la derecha. «Saltar» abajo a la izquierda, solo en el primer paso.
- Transición entre pasos: `fly` horizontal de 24 px más fundido en 340 ms con `--ease`. Con reducir movimiento, solo fundido.

Pasos:

1. **Bienvenida**: elefante, título en `--t-hero` y un resumen de lo detectado («Hemos encontrado Herd con PHP 8.3 y 8.4, y MySQL de Homebrew»), sacado de `get_snapshot()`.
2. **Escenario**: tarjetas grandes de selección única, generadas desde `scenarios()`. Se esperan 3 recomendadas más «Personalizado». Su contenido lo fija la decisión del asistente.
   - Cada tarjeta lleva icono, nombre, una frase, la lista corta de componentes, la **RAM estimada en reposo** y el disco.
   - La recomendada lleva un badge «Recomendado».
   - La seleccionada tiene borde de 2 px `--accent` y un check que aparece con escala.
   - `role="radiogroup"`, con flechas para moverse.
3. **Ajustar**: un `Group` por categoría. Cada componente tiene dos switches: «Instalar» y «Arrancar al iniciar sesión». También se eligen la versión de PHP por defecto y las opciones de sitios cuando existan.
   - Si se toca algo, el escenario pasa a «Personalizado (basado en X)».
   - Abajo, un resumen fijo en vivo: «6 componentes · ~1,2 GB · ~180 MB en reposo».
4. **Revisar y aplicar**: el plan que devuelve `plan_for`, **visible antes de ejecutar**.
   - Una fila por paso, con estado pendiente, en curso, hecho o error, y «Ver salida» plegable con la salida de `brew` en mono.
   - Si algún paso pide contraseña de administrador, un `Sheet` lo explica **antes** de empezar.
   - Ante un error, «Reintentar» u «Omitir».
5. **Listo**: resumen, «Abrir el panel», «Copiar configuración del MCP» (si existe) y una nota de que el elefante está en la barra de menús.
   - Al terminar, `set_settings({ onboarded: true })`.

Ir hacia atrás conserva las elecciones. Cerrar la ventana a mitad no marca `onboarded`.

---

## 8. Menú de la aplicación y atajos (`app_menu.rs`)

Sustituye al menú por defecto de Tauri, que está en inglés. Solo se ve con la ventana abierta, en modo `Regular`.

- **Elephant Stack**: Acerca de, Ajustes… ⌘, · Ocultar ⌘H, Ocultar otros ⌥⌘H · Salir ⌘Q.
- **Edición**: Deshacer, Rehacer, Cortar, Copiar, Pegar, Seleccionar todo (`PredefinedMenuItem`) · Buscar ⌘F (envía `ui:command find`).
- **Visualización**: Panel ⌘1, Sitios ⌘2, Servicios ⌘3, PHP ⌘4, Logs ⌘5 · Mostrar/ocultar barra lateral ⌃⌘S · Mostrar/ocultar inspector ⌥⌘I · Actualizar ⌘R.
- **Ventana**: Minimizar ⌘M, Zoom, Cerrar ⌘W.

Los items de navegación emiten `nav:go`. Así los atajos funcionan aunque el foco esté en un campo, sin listeners de teclado en JS.

---

## 9. Menú del icono de la barra de menús (`tray.rs`)

### 9.1 Estructura

```
● 3 servicios en marcha · 412 MB                 (deshabilitado, informativo)
⚠ mysql se ha detenido (código 1)                (solo con problemas; abre #/servicios)
───────────
Abrir Elephant Stack
Logs                                    ▸  Abrir visor
                                           Errores recientes (2)        → #/logs?level=error
                                           ──
                                           Laravel · acme               (los 5 proyectos con actividad más reciente)
───────────
PHP 8.4 por defecto                     ▸  ✓ 8.4.27 · Elephant Stack
                                             8.3.14 · Herd
                                           ──
                                           Usar en un proyecto…         → abre la ventana en #/php
Servicios                               ▸  Reiniciar todos
                                           Detener todos
                                           ──
                                           ● nginx                    ▸ Reiniciar · Detener · ✓ Al iniciar sesión · Ver logs
                                           ● mysql                    ▸ …
                                           ○ redis                    ▸ Arrancar · ✓ Al iniciar sesión · Ver logs
Sitios                                  ▸  acme.test                  ▸ Abrir en el navegador · Abrir en el editor ·
                                                                         Abrir terminal · Copiar URL · Ver logs
                                           ──
                                           Ver todos…                   (solo si hay sitios)
Abrir Mailpit                                                            (solo si está instalado)
───────────
Ajustes…
Salir de Elephant Stack                 ⌘Q
```

- Los submenús vacíos no se muestran (Sitios hasta que existan, Logs si no hay módulo de logs).
- Los servicios aparecen en el mismo orden que en la ventana: primero los que están en marcha.
- En «PHP», la versión por defecto es un `CheckMenuItem` marcado a mano, porque Tauri no tiene radios. Al pulsar otra versión, se desmarca la anterior en el mismo despacho.
- Los puntos ● y ○ forman parte del texto, no son iconos, para que funcionen igual en claro y oscuro.

### 9.2 Ids y acciones

```rust
pub enum TrayAction {
    Open { route: Option<String> },        // "nav:#/logs?level=error" · "open"
    PhpDefault(String),                    // "php:default:8.4@Herd"  (la key de PhpStatus)
    Service(String, Operation),            // "svc:restart:mysql"     (turn_on|turn_off|restart|enable_autostart|disable_autostart)
    ServicesAll(Operation),                // "svc-all:restart" · "svc-all:turn_off"
    Site(String, SiteAction),              // "site:open:acme" · "site:editor:acme" · "site:terminal:acme" · "site:copy:acme"
    OpenMailpit,                           // "mailpit:open"
    Quit,                                  // "quit"
}
impl FromStr for TrayAction { /* … */ }
impl Display for TrayAction { /* el inverso exacto */ }
```

- `on_menu_event` hace `id.parse::<TrayAction>()` y llama a `actions::*`, las mismas funciones que usan los comandos y el MCP. Un id desconocido se ignora y se registra.
- Las acciones lentas corren en `spawn_blocking`. Antes de empezar, marcan `busy` en `AppState` y emiten el estado. El item cambia en sitio a «Reiniciando mysql…», deshabilitado. Al terminar, vuelven a emitir el estado.

### 9.3 Construcción y actualización

```rust
pub struct Entry { id: Option<TrayAction>, text: String, enabled: bool, checked: Option<bool>, accel: Option<&'static str>, children: Vec<Entry>, separator: bool }
pub fn entries(snapshot: &Snapshot, usage_kb: Option<u64>) -> Vec<Entry>;   // pura, con tests
fn shape(entries: &[Entry]) -> u64;                                          // hash de ids + jerarquía + separadores
pub fn sync(app: &AppHandle, entries: Vec<Entry>);                           // aplica al NSMenu
```

- `sync` compara `shape` con la anterior.
  - **Si la estructura no cambia**, recorre los handles guardados (`HashMap<String, MenuItemKind>`) y llama a `set_text`, `set_enabled` o `set_checked` solo donde haya diferencias. No hay parpadeo y funciona aunque el menú esté abierto.
  - **Si cambia**, construye un `Menu` nuevo y llama a `tray.set_menu`.
- `sync` se llama desde el **mismo** punto que emite `state:changed`. No hay otra vía.
- **Vigilancia** (`watch.rs`), activa si `settings.watch_services`:
  - Cada **20 s**, `launchctl list`: unos 10 ms y ningún proceso de Ruby. Se compara PID y código de salida de `homebrew.mxcl.*` con el snapshot.
  - Si algo cambia, se lanza un `refresh` completo, que acaba en `state:changed` y `sync`.
  - En el mismo tick se actualiza la cabecera de RAM con `ps` sobre los PIDs en marcha, **solo con `set_text`**.
  - Sin servicios en marcha, el tick solo ejecuta `launchctl`.
- Al recibir `TrayIconEvent::Click` se lanza también un tick, sin depender de que llegue antes de pintar el menú. Solo garantiza que la siguiente apertura esté al día.
- **Icono**: `tray.png` (plantilla) en estado normal y `tray-warn.png` (plantilla con punto) si hay `problems` o `unseen_log_errors > 0`, mediante `set_icon_with_as_template`.
- `set_title(Some("8.4"))` solo si `show_php_in_menu_bar`.

### 9.4 Errores

- Si una acción del menú falla, se añade un `Problem { kind: ActionFailed }`. Aparece como fila ⚠ en el menú, pone el icono de aviso y, si la ventana está abierta, también un toast.
- El problema desaparece cuando se abre su vista o cuando la siguiente operación sobre ese mismo objetivo sale bien.
- No hay notificaciones del sistema (§3.5).

---

## 10. Accesibilidad

- Todo se puede usar solo con el teclado: barra lateral, filas, switches, menús ⋯ (con Intro o espacio), segmented (con flechas), lista de logs (con flechas, Intro y Esc), asistente (con Tab y ↵).
- `svelte-check --fail-on-warnings` en CI: ninguna advertencia de accesibilidad se queda.
- Cada `IconButton` lleva una etiqueta obligatoria, que el tipo exige.
- Los switches tienen etiquetas con verbo y objeto («Detener mysql»), como hoy.
- El estado nunca se transmite solo con color: el `StatusDot` siempre va acompañado de texto («En marcha», «Error · código 1»), y el nivel de log lleva su nombre en la pastilla.
- Contraste AA en `--label-2` sobre `--group` en claro y en oscuro. `prefers-contrast: more` sube separadores y textos secundarios.
- `prefers-reduced-motion`: todas las duraciones a 0, sin pulsos ni brillos, y transiciones de vista solo con fundido.
- Toasts con `role="status"` o `role="alert"`, y el progreso del asistente con `aria-live="polite"`.

---

## 11. Rendimiento y presupuestos

Se miden con `scripts/budget.sh` (`ps -o rss` de la app y de sus `WebContent`, y `gzip -c dist/assets/*.js | wc -c`). En CI corre la parte del bundle. La de RAM se ejecuta a mano antes de cada versión y su resultado se apunta en la descripción del PR.

| Métrica | Objetivo |
|---|---|
| Sin ventana (solo barra de menús) | 0 procesos WebContent de la app. RSS del proceso principal < 30 MB. |
| Chunk JS inicial (Shell + Panel + Servicios + PHP + Ajustes + Svelte) | ≤ 40 KB gz |
| Chunk Logs / chunk Asistente | ≤ 20 KB gz cada uno |
| CSS total | ≤ 15 KB gz |
| Abrir la ventana desde el menú hasta el primer pintado con datos | < 200 ms (snapshot en caché) |
| Logs con 10 000 entradas en el búfer y 100 entradas/s en vivo | scroll sin tirones y WebContent ≤ 150 MB |
| Coste de la vigilancia | < 0,1 % de CPU de media con 5 servicios |

---

## 12. Plan de entrega

Cada fase se puede publicar por separado y deja la app funcionando.

1. **Base.**
   - Svelte, `tokens.css`, `base.css` y los componentes que necesita la vista actual (Button, IconButton, Switch, Group, Row, StatusDot, Toast, Skeleton, Icon).
   - `state.rs`, `actions.rs`, `Snapshot`, `get_snapshot` y `state:changed`. `ts-rs` y `lib/bindings`.
   - El shell con barra lateral, más Servicios, PHP y Ajustes (General) con **paridad funcional** respecto a `main.ts`. Ventana nueva con vibrancy y acento.
   - Al final se borra `src/main.ts` antiguo y `styles.css`. No conviven dos estilos en la misma rama más allá del PR.
2. **Menú de la barra.** `tray.rs` con tests, `watch.rs`, `app_menu.rs` y el router con `nav:go`.
3. **Panel**, con el héroe y las tarjetas.
4. **Asistente**, en cuanto existan `scenarios`/`plan_for`/`apply_plan`. Puede empezar con servicios y PHP solamente.
5. **Logs**, en cuanto exista su backend. **Sitios**, en cuanto exista la gestión de nginx y dnsmasq.
6. **Transversal**: `svelte-check`, `vitest` y la parte del bundle de `budget.sh` en CI. Capturas de referencia en claro y oscuro de cada vista en `docs/capturas/`, que se actualizan en cada PR visual.

---

## 13. Criterios de aceptación

**Arquitectura**
- [ ] `rg "invoke\(|listen\(" src --glob '!src/lib/ipc.ts' --glob '!src/lib/events.ts'` no devuelve nada.
- [ ] `src/lib/bindings/` se genera con `cargo test export_bindings` y CI falla si hay diferencias.
- [ ] `pnpm build`, `svelte-check --fail-on-warnings`, `vitest run` y `cargo test` pasan.
- [ ] Ninguna dependencia aparte de las de §4.1.

**Sincronización**
- [ ] Cambiar la versión de PHP desde el menú de la barra con la ventana abierta actualiza la vista PHP en menos de 500 ms, sin recargar.
- [ ] Detener un servicio desde la ventana actualiza el submenú Servicios. Abrir el menú justo después muestra el estado nuevo.
- [ ] Parar un servicio desde la terminal (`brew services stop`) se refleja en el menú y en la ventana en ≤ 25 s.
- [ ] Al cambiar solo la RAM, la cabecera del menú se actualiza sin reconstruirlo, lo que se comprueba en un test de `sync` con `shape` igual.

**Menú de la barra**
- [ ] Tests de `tray.rs`: `TrayAction` ida y vuelta (`to_string().parse()`) para todas las variantes. `entries` con un snapshot vacío, con un servicio en error (aparece la fila ⚠ y el icono de aviso) y con dos versiones de PHP (solo una marcada).
- [ ] Abrir el menú no crea ningún webview, lo que se comprueba con `ps`.
- [ ] Un id `nav:` con la ventana cerrada la abre directamente en la ruta pedida.

**Ventana y diseño**
- [ ] Vibrancy en la barra lateral, contenido opaco y botones de ventana alineados con el título de la barra lateral.
- [ ] Cambiar el acento en Ajustes del Sistema y volver a la ventana cambia la selección, los botones primarios y el foco.
- [ ] Apariencia Claro/Oscuro/Sistema cambia también el material nativo.
- [ ] Con «Reducir transparencia», la barra lateral queda opaca y legible. Con «Aumentar contraste», los separadores se ven. Con «Reducir movimiento», no hay desplazamientos.
- [ ] Capturas de referencia de cada vista en claro y oscuro, revisadas.
- [ ] Ningún `cursor: pointer` en el CSS (`rg "cursor:\s*pointer" src` vacío).

**Accesibilidad**
- [ ] Recorrido completo solo con teclado: asistente de principio a fin, cambiar PHP, parar un servicio y filtrar logs y abrir una entrada.
- [ ] VoiceOver lee el nombre y el estado de cada servicio y el nivel y mensaje de cada fila de log.

**Rendimiento**
- [ ] Se cumplen los presupuestos de §11, medidos y apuntados en el PR de la fase 1 (sin logs) y en el de la fase 5 (con logs).

---

## 14. Riesgos

| Riesgo | Impacto | Mitigación |
|---|---|---|
| La vibrancy y la ventana transparente dependen de API privada de WebKit/AppKit, siempre activa desde Tauri 2.12.1. Apple podría cambiarla. | Barra lateral sin material o con artefactos. | El contenido es opaco, y existe la clase `no-vibrancy` con `--sidebar-fallback`. La app no va a la Mac App Store. |
| Coste de composición GPU de la transparencia. | Algo más de energía. | Solo la barra lateral es transparente. Logs y contenido son opacos. |
| El menú de la barra puede mostrar datos de hasta 20 s si un servicio cae solo. | Información desactualizada un momento. | Vigilancia con `launchctl` cada 20 s, un tick extra al hacer clic y `sync` tras cada acción. Se puede bajar a 10 s si se ve necesario, porque cuesta ~10 ms. |
| La inyección del acento usa `objc2` con `unsafe`. | Un fallo deja el acento por defecto. | Si falla, `#007aff`. Código aislado en `appearance.rs`, solo con `cfg(target_os = "macos")`. |
| Svelte 5 añade un compilador y dependencias de build a un repo que no tenía. | Actualizaciones periódicas. | Versiones exactas fijadas y solo cuatro paquetes. La lógica pura vive en `lib/*.ts` sin Svelte, con tests. |
| Una sola ventana no permite ver logs y panel a la vez. | Menos flexibilidad. | El menú de la barra cubre las acciones rápidas. `Logs.svelte` puede montarse después en una segunda ventana sin reescribirse. |
| La altura fija de 20 px en logs obliga a no expandir las entradas en la lista. | Hay que ir al inspector para ver la traza. | Indicador «+N líneas», el inspector se abre con Intro y la traza se puede copiar. |
| La calidad «nivel CleanMyMac» depende de la disciplina de diseño, no del framework. | Que parezca una web. | Tokens obligatorios (prohibido usar colores o tamaños sueltos, comprobado en revisión), capturas de referencia por PR, comportamiento nativo en `base.css` y criterios de §13. |
| Los presupuestos de §11 son objetivos, todavía no medidos. | Pueden resultar optimistas. | Se miden en la fase 1. Si no se cumplen, se ajusta el objetivo con datos y se documenta el motivo en este archivo. |
| Los contratos de asistente, logs y sitios dependen de otras decisiones. | Cambios de nombre en la API. | La interfaz los consume solo a través de `ipc.ts` y `bindings`. Un cambio toca un archivo. |

---

## 15. Fuentes

- Código del repo: `src/main.ts`, `src/styles.css`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json` y `src-tauri/src/components.rs`.
- Tauri 2.12.5 y sus crates, en el registro local de Cargo: `tauri-utils-2.10.2/src/config.rs` (`transparent`, `window_effects`, `macos_private_api`), `tray-icon-0.25.1/src/platform_impl/macos/mod.rs` (orden de `Click` y `performClick`), `tauri-2.12.5/src/menu/*.rs` (`set_text`/`set_checked`/`set_enabled`) y `tauri-2.12.5/src/app.rs` (menú por defecto de macOS).
- [WebKit bug 254578: «css AccentColor keyword is not synced»](https://lists.webkit.org/pipermail/webkit-unassigned/2023-March/1095385.html)
- [WebKit bug 241866: Implement AccentColor/AccentColorText](https://bugs.webkit.org/show_bug.cgi?id=241866)
- [web-features: prefers-reduced-transparency (Safari sin soporte)](https://web-platform-dx.github.io/web-features-explorer/features/prefers-reduced-transparency)
- Registro npm (consultado el 2026-11-06): `svelte` 5.57.3, `@sveltejs/vite-plugin-svelte` 7.3.1 (peer `vite ^8`) y `svelte-check` 4.7.8.
- Medidas locales: `launchctl list` ≈ 10 ms; un proceso `WebContent` ≈ 39 MB de RSS.
