# 02 · Registros (logs): recopilación, visualización y filtro por proyecto

- Estado: **aceptada**
- Fecha: 2026-11-06
- Alcance: logs de aplicación (Laravel/Monolog), logs de nginx (Herd hoy y el nginx propio de Elephant más adelante), PHP-FPM, descubrimiento de proyectos y la API que usarán la ventana de Registros y el MCP.

## 1. Decisión

Elephant tendrá un **motor de logs propio en Rust**: una crate nueva, `elephant-logs`, en `logs/` junto a `shim/`. La usarán dos clientes, la app Tauri y el binario del MCP, y ninguno necesita que el otro esté abierto. **No habrá procesos residentes**: con la ventana de Registros cerrada no queda ningún hilo, temporizador ni descriptor del módulo. Los ficheros de log siguen siendo la única fuente de verdad: no se copian a ninguna base de datos ni se indexan en disco.

La interfaz será una **ventana nativa propia, "Registros"**, hecha con el mismo frontend TypeScript y con el sistema visual del rediseño.

Es básicamente la propuesta B, con cuatro cambios que la hacen más simple y más robusta (ver §3):

1. Lectura en directo por **sondeo con `stat`** mientras la ventana está visible, en lugar de `notify` y FSEvents.
2. **Identificadores de entrada en texto** basados en la posición dentro del fichero, porque un `u64` pierde precisión en JavaScript y una secuencia no sobrevive entre procesos.
3. **Zona horaria por proyecto**: Laravel escribe la hora sin zona y en UTC por defecto, mientras que nginx escribe la hora local.
4. Sin `regex`, `xxhash-rust` ni `notify` en la v1. Las únicas dependencias nuevas son `memchr` y `jiff`.

### Por qué

| Criterio | A · Opcodes Log Viewer | B · motor propio (elegida) |
|---|---|---|
| Reposo | 0 (no hay proceso) | 0 (no hay hub) |
| En uso | `php -S` con 2 workers (unos 60–80 MB) **más** otro WebView | hub limitado a 40 MB dentro del proceso de la app, más el WebView de la ventana |
| Disco y distribución | esqueleto Laravel de 40–60 MB, más un CI y releases propios | ~0, compilado dentro de la app y del MCP |
| Filtro por proyecto | por carpeta o alias de fichero. El `nginx-error.log` global de Herd **no se puede repartir** por proyecto | por ruta (app) y **por `host:`** (nginx), comprobado en este Mac |
| MCP | cliente HTTP de una API **sin documentar**; hay que arrancar PHP y gestionar un token | llamada directa a la misma crate, sin red |
| Diseño | estética de Log Viewer en una ventana aparte, que choca con el rediseño "tipo Apple" que pidió el usuario | integrado: mismos tokens y componentes |
| Superficie de ataque | servidor HTTP local con un middleware propio contra DNS rebinding | ninguna: solo IPC de Tauri y stdio |
| Mantenimiento | actualizar Laravel, Log Viewer y Composer y vigilar sus CVE | Rust con tests y fixtures, sin dependencias pesadas |
| Esfuerzo | días | 2–3 semanas |

Lo que inclina la balanza:

- **El MCP tiene que entender los logs de todas formas**: agrupar errores, recortar trazas para ahorrar tokens, filtrar por proyecto y por tiempo, y ocultar secretos. Con A habría dos capas de interpretación (el parser de Log Viewer y otra en el MCP sobre un JSON que no tiene contrato). Con B la lógica está en un solo sitio.
- **Hechos de este Mac** (comprobados en solo lectura):
  - `Herd/config/nginx/herd.conf` tiene `access_log off;` y un solo `error_log …/Herd/Log/nginx-error.log`. Es de root con permisos 644, así que se puede leer.
  - Cada línea de ese log trae `host: "aquaservice-api-area-cliente.test"` y `request: "POST /api/… HTTP/1.1"`. Solo un parser propio puede asignar esas líneas a su proyecto.
  - `Herd/Log/php-fpm.log` es del usuario con permisos 600, y se puede leer.
- El usuario pidió un rediseño completo con cuidado de nivel CleanMyMac o Apple. Una ventana con otra UI web lo contradice.
- El usuario valora mucho el rendimiento, y B gasta menos en uso y no añade nada en disco.

El coste de B, más código propio, se reduce con un alcance cerrado (§3) y con tests de fixtures como los que ya existen en `brew.rs` y `tools.rs`.

### Correcciones a las propuestas, tras verificarlas

- No hay workspace de Cargo: `shim/` es una crate independiente que `src-tauri` usa como dependencia por ruta. `logs/` seguirá el mismo esquema y no se crea ningún workspace.
- Los binarios propios de PHP están en `~/.elephant-stack/php/<v>/php`, no en `bin/php`. Incluyen mbstring, tokenizer, ctype, fileinfo y openssl. Este dato ya no influye en la decisión.
- `Herd/config/valet/Sites/` **ya aparece** en `paths` de `config.json`. Los sitios enlazados son entradas de esa carpeta aparcada (symlinks) y no necesitan una fuente aparte; solo cambia el `origin`.
- `Settings` es `Copy` y `get_settings` lo copia con `*settings`. Si se añaden campos con `Vec` o `String`, hay que quitar `Copy` y usar `.clone()`.
- La CSP solo permite `ipc:` y la capacidad `default` solo cubre la ventana `main`. La ventana de Registros tiene que añadirse a `capabilities`.

## 2. Qué se descarta y por qué

- **Opcodes Log Viewer (propuesta A).** Es un buen visor, pero arrastra un esqueleto Laravel con su CI, un servidor HTTP local con seguridad propia, una API sin contrato para el MCP, una estética ajena y no sabe repartir el nginx de Herd por proyecto. Su mejor idea, ocultar los frames de `vendor/`, se adopta en B.
- **Logdy.** Lleva parado desde 2025, pesa unos 45 MB y trata las trazas de Laravel línea a línea.
- **GoAccess.** Solo lee logs de acceso, y Herd no los genera.
- **lnav.** Solo funciona en terminal y no tiene API para la UI. No entra en la v1. Se podrá estudiar más adelante como acción "Abrir en Terminal" si está instalado, sin dependencia.
- **SQLite / FTS5 en la v1.** Duplicaría los logs, escribiría en disco con cada petición y obligaría a purgar y migrar. La API (`LogQuery`) queda preparada para añadir un índice detrás sin cambiar ni la UI ni el MCP.
- **`notify` / FSEvents.** Los eventos llegan agrupados, con retraso y sin orden garantizado, así que habría que reconciliar con `stat` de todas formas. El sondeo (§5.4) es más sencillo, determinista y fácil de probar, y con la ventana visible su coste es despreciable.
- **Activar `access_log` en Herd.** El fichero es de Herd, que lo regenera. Elephant **nunca** modifica la configuración de Herd.
- **Recorrer `$HOME` buscando `.php-version`.** Es caro y dispara avisos de privacidad (TCC). `.php-version` solo sirve para marcar y describir proyectos dentro de raíces que ya se conocen.
- **Leer `HerdCoreData.sqlite`.** Es un detalle interno de CoreData, sin contrato.

## 3. Alcance de la v1

Entra:

- Descubrimiento de proyectos: Herd aparcados y enlazados, raíces del usuario y, en el futuro, sitios de Elephant.
- Parsers de Laravel/Monolog (`LineFormatter` multilínea y `JsonFormatter`), nginx error (Herd), nginx access (formato JSON de Elephant y `combined` de respaldo) y PHP-FPM.
- Carga inicial desde el final del fichero, paginación hacia atrás, búsqueda binaria por tiempo, búsqueda de subcadenas y agrupación por huella.
- Directo por sondeo, ventana Registros y comandos Tauri.
- Una API pública de la crate que el MCP consume tal cual. El transporte del MCP y su registro se definen en su propia decisión.

No entra en la v1:

- Regex.
- Índice persistente.
- Lectura de `config/logging.php` (los globs extra se añaden a mano).
- Logs en `stderr`/`syslog`.
- Visor de Horizon, colas o Supervisor.
- lnav.

## 4. Descubrimiento de proyectos (`logs/src/projects.rs`)

Este módulo es la **única fuente de proyectos** de Elephant. Lo reutilizan el asistente, el menú de la barra, el MCP y el futuro nginx. Si otra decisión crea un registro de sitios propio (`~/.elephant-stack/sites.json`), entra como fuente 1 sin cambiar la API.

### 4.1 Fuentes, por prioridad

Al deduplicar por `fs::canonicalize`, gana la primera fuente que aparezca.

1. **Sitios de Elephant** (cuando existan): `~/.elephant-stack/sites.json`.
2. **Herd**: se lee `~/Library/Application Support/Herd/config/valet/config.json`, que contiene `paths: [..]` y `tld`.
   - Cada **entrada directa** de cada `paths[i]`, siguiendo symlinks, es un sitio `<nombre>.<tld>`.
   - `origin = HerdLinked` si `paths[i]` acaba en `config/valet/Sites`; si no, `HerdParked`.
   - Si el JSON no existe o está dañado, la fuente se ignora sin error.
3. **Raíces del usuario**: `~/.elephant-stack/projects.json → roots: [..]`, elegidas con `tauri-plugin-dialog`.
   - Una raíz que ya es un proyecto (por ejemplo, tiene `artisan`) cuenta como proyecto.
   - Si no, se miran sus subcarpetas con **profundidad máxima 2**, sin seguir symlinks dentro de la búsqueda y saltando `vendor`, `node_modules` y las carpetas ocultas.

### 4.2 Clasificación

Solo se usa `metadata`, sin leer contenido:

- `artisan` presente → `Laravel`.
- `composer.json`, `public/index.php` o `.php-version` presentes → `Php`.
- Ninguno de ellos → no es un proyecto, salvo los sitios de Herd, que se muestran siempre como `Other`.
- `php_version`: se obtiene con `elephant_php::Layout::resolve(root)` y se muestra el `id` de la versión resuelta. Si falla, `None`.

### 4.3 Estructuras

```rust
pub struct Project {
    pub id: String,              // FNV-1a 64 de la ruta canónica, en hex (16 caracteres). Estable entre sesiones y procesos.
    pub name: String,            // nombre de la carpeta
    pub root: PathBuf,           // canónica
    pub hosts: Vec<String>,      // ["tienda.test"]; vacío si no lo sirve nadie
    pub kind: ProjectKind,       // Laravel | Php | Other
    pub origin: Origin,          // Elephant | HerdParked | HerdLinked | User
    pub php_version: Option<String>,
    pub log_tz: LogTz,           // ver §5.2
}

pub fn discover(home: &Path) -> Vec<Project>;            // ordenados por nombre
pub fn project_for_host(projects: &[Project], host: &str) -> Option<&Project>;
```

- FNV-1a se implementa en unas 10 líneas. No se usa `DefaultHasher` porque su resultado no es estable entre versiones de Rust.
- `project_for_host` compara sin distinguir mayúsculas. Un host `a.b.tienda.test` también se asigna a `tienda.test`, como hace Valet con los comodines.

### 4.4 Configuración del usuario: `~/.elephant-stack/projects.json`

```json
{
  "roots": ["/Users/cristian/Code"],
  "overrides": {
    "<project.id>": {
      "log_globs": ["/ruta/absoluta/custom/*.log"],
      "timezone": "Europe/Madrid"
    }
  }
}
```

- Si el fichero no existe o está dañado, se usan los valores por defecto, igual que en `settings.rs`.
- Se escribe con `serde_json::to_string_pretty`, en un fichero temporal y después `rename`.

## 5. Motor de logs (`logs/src/`)

```
logs/
  Cargo.toml            # elephant-logs; deps: serde, serde_json, memchr, jiff
  src/lib.rs            # API pública (§6)
  src/projects.rs       # §4
  src/sources.rs        # proyecto → fuentes y ficheros
  src/parse/{laravel,nginx_error,nginx_access,php_fpm}.rs
  src/reader.rs         # lectura hacia atrás, paginación, búsqueda binaria por tiempo
  src/tail.rs           # sondeo y estado por fichero
  src/query.rs          # filtros, fusión por tiempo, agrupación
  src/redact.rs         # ocultación de secretos (uso del MCP)
  tests/fixtures/*.log  # fixtures reales anonimizados
```

### 5.1 Fuentes

```rust
pub enum SourceKind { LaravelApp, NginxAccess, NginxError, PhpFpm }

pub struct Source {
    pub kind: SourceKind,
    pub project: Option<String>,   // None = global (nginx/PHP-FPM de Herd); el proyecto se asigna por entrada
    pub dir: PathBuf,
    pub pattern: String,           // glob simple: solo `*` en el nombre del fichero
}
```

| Fuente | Ficheros | Proyecto |
|---|---|---|
| Laravel | `<root>/storage/logs/*.log` (incluye `laravel.log`, `laravel-YYYY-MM-DD.log` y canales propios), más los `log_globs` del proyecto | el del proyecto |
| nginx error de Herd | `~/Library/Application Support/Herd/Log/nginx-error.log` | se toma de `host:` en cada línea |
| PHP-FPM de Herd | `~/Library/Application Support/Herd/Log/php-fpm.log` | ninguno ("Sistema") |
| nginx access de Elephant (futuro) | `~/.elephant-stack/logs/nginx/access.log` | se toma de `"h"` en cada línea |
| nginx error de Elephant (futuro) | `~/.elephant-stack/logs/nginx/error.log` | se toma de `host:` en cada línea |

Las rutas de Herd salen de `HOME` y no se escriben a mano en el código. Si un fichero no se puede leer (`EACCES`), la fuente aparece con el estado `unreadable` y un mensaje en español, sin romper el resto.

### 5.2 Modelo de entrada

```rust
pub struct Entry {
    pub id: String,                    // "<file_key:016x>-<offset:x>": ver abajo
    pub kind: SourceKind,
    pub project: Option<String>,       // Project.id; None = "Sin proyecto" o "Sistema"
    pub host: Option<String>,          // nginx: host original, aunque no coincida con ningún proyecto
    pub ts_ms: i64,                    // UTC
    pub level: Level,                  // Debug, Info, Notice, Warning, Error, Critical, Alert, Emergency
    pub channel: Option<String>,       // canal de Monolog ("local", "production"…)
    pub message: String,               // primera línea, como mucho 2 KB (se corta respetando UTF-8)
    pub exception: Option<ExceptionInfo>, // { class, message, file, line }
    pub http: Option<HttpInfo>,        // { method, path, status, bytes, duration_ms, upstream_ms, ip, ua }
    pub fingerprint: String,           // hex de 16 caracteres
    pub len: u32,                      // bytes de la entrada completa en el fichero
}
```

**Identificador.**
- `file_key` es el FNV-1a de la ruta canónica del fichero. `offset` es el byte donde empieza la cabecera.
- Se usa texto por dos motivos: un `u64` pierde precisión en JS, y el MCP (otro proceso) tiene que poder resolver el mismo id.
- `entry(id)` busca el fichero entre las fuentes descubiertas, lee a partir de `offset` y **comprueba que ahí empieza una cabecera**. Si no, devuelve `Err("La entrada ya no existe: el fichero ha cambiado.")`.

**Cuerpos.** El stack trace **no se guarda en memoria**: se relee del disco con `read_at` (`std::os::unix::fs::FileExt`) cuando se pide el detalle.

**Niveles.** Escala de Monolog/RFC 5424. Para nginx: `debug`→Debug, `info`→Info, `notice`→Notice, `warn`→Warning, `error`→Error, `crit`→Critical, `alert`→Alert y `emerg`→Emergency. Para nginx access, el nivel sale del estado: 5xx→Error, 4xx→Warning y el resto→Info.

**Zona horaria (`LogTz`).**
- Laravel escribe la hora con `Y-m-d H:i:s`, sin zona, en `app.timezone`, que por defecto es UTC. Si la cabecera trae desfase (`±HH:MM`, en formatos personalizados), se usa ese.
- Si no lo trae, la zona del proyecto se resuelve en este orden:
  1. `overrides.timezone`;
  2. el literal `'timezone' => 'X'` en `config/app.php`;
  3. `APP_TIMEZONE` en `.env` (solo se lee esa línea y nunca se guarda ni se expone nada más del `.env`);
  4. el valor por defecto de `env('APP_TIMEZONE', 'X')` en `config/app.php`;
  5. `UTC`.
- nginx y PHP-FPM usan la zona del sistema (`jiff::tz::TimeZone::system()`).
- La UI muestra siempre la hora local del Mac.

**Huella (`fingerprint`).**
- Con excepción: FNV(clase + `file` relativo a la raíz + `line`).
- Sin excepción: FNV(nivel + mensaje normalizado). La normalización cambia secuencias de dígitos por `#`, UUIDs por `<uuid>` y hex de 16 o más caracteres por `<hex>`, y recorta a 200 bytes.
- nginx access: FNV(método + estado + ruta normalizada, sin query y con los segmentos numéricos o UUID cambiados por `{id}`).

### 5.3 Parsers

Se escriben a mano con `memchr`, sin `regex`. Cada uno es una función pura `fn parse_header(line: &[u8]) -> Option<Header>` más un acumulador de líneas de continuación, y se prueba con fixtures.

**Laravel `LineFormatter`.**
- Una cabecera es una línea que cumple **todo** esto:
  - empieza por `[`, seguido de `YYYY-MM-DD`, después `' '` o `'T'`, después `HH:MM:SS`;
  - opcionalmente, `.f{1,6}` y `±HH:MM` o `Z`;
  - después `] `, luego `canal.NIVEL: ` con el canal en `[A-Za-z0-9_-]+` y el NIVEL entre los ocho de Monolog en mayúsculas.
- Cualquier otra línea es continuación de la entrada pendiente: `[stacktrace]`, `#0 …`, `[previous exception]`, `"} `.
- **Importante**: Laravel activa los saltos de línea en línea (`allowInlineLineBreaks`), así que el contexto `{"exception":"[object] (Clase(code: N): mensaje at /ruta/F.php:123)\n[stacktrace]\n#0 …"}` lleva **saltos de línea reales y no es JSON válido**. No se pasa a `serde_json`.
- La excepción se saca buscando en bytes `"exception":"[object] (`, luego la clase hasta `(code:`, y después el mensaje y ` at <file>:<line>)`, buscando desde el final.
- Si la primera línea no vacía de un fichero empieza por `{`, el fichero se trata como `JsonFormatter`: una entrada por línea, con `serde_json` y los campos `datetime`, `channel`, `level_name`, `message` y `context.exception`.
- Los bytes que no son UTF-8 se convierten con `String::from_utf8_lossy`.
- Una línea suelta al principio del fichero, sin cabecera previa, forma una entrada `Level::Info` con `ts` igual a la `mtime`, para que no se pierda.

**nginx error.**
- Formato: `YYYY/MM/DD HH:MM:SS [nivel] pid#tid: *cid mensaje, client: …, server: …, request: "…", upstream: "…", host: "…"`.
- Los campos `host`, `request` y `upstream` se extraen buscando `, clave: ` **desde el final**, porque el mensaje puede contener comas.
- `request` se separa en método y ruta.
- Las líneas sin `*cid`, como las de arranque, también son válidas.

**nginx access.**
- JSON de Elephant (§8) con `serde_json`.
- Respaldo `combined`: `ip - user [dd/Mon/yyyy:HH:MM:SS ±zzzz] "M ruta HTTP/x" status bytes "ref" "ua"`.

**PHP-FPM.**
- Formato: `[DD-Mon-YYYY HH:MM:SS] NIVEL: mensaje`.
- Las líneas `WARNING: [pool x] child N said into stderr: "…"` se conservan como mensaje.

**Marca de vendor.** Al obtener el detalle, cada frame `#N /ruta(línea): llamada` se clasifica:

- `vendor: true` si la ruta contiene `/vendor/`;
- además, `framework: true` si contiene `/vendor/laravel/framework/`, `/vendor/symfony/` o `/vendor/barryvdh/laravel-debugbar/`, la misma lista por defecto que usa Log Viewer.

### 5.4 Lectura (`reader.rs`) y directo (`tail.rs`)

**Carga inicial.**
- Se lee **hacia atrás** desde el final, en bloques de 64 KB, hasta tener 500 entradas por fichero o haber leído 4 MB.
- Cuando una entrada cruza el límite de un bloque, se arrastra el resto al bloque siguiente.
- Nunca se lee un fichero entero para la primera página.

**Paginación hacia atrás.**
- Cursor `before: String` (un `Entry.id`): cada fichero sigue leyendo desde su offset más bajo cargado.
- Las entradas de varios ficheros se **fusionan por `ts_ms`** de forma descendente (k-way merge con `BinaryHeap`).

**Búsqueda binaria por tiempo** (`since`/`until`):
1. saltar a un offset;
2. alinear a la siguiente cabecera, explorando como mucho 256 KB;
3. comparar la fecha y repetir.

Cada fichero se considera ordenado por fecha. Si se detecta desorden, por ejemplo un cambio de zona, se cae a búsqueda lineal en ese fichero.

**Texto.** Se busca la subcadena sin distinguir mayúsculas (ASCII): se pasa a minúsculas en un búfer reutilizado y se busca con `memchr::memmem::Finder`. La búsqueda cubre la entrada completa, traza incluida, leída del disco por bloques.

**Directo, por sondeo.** Solo funciona mientras hay suscripciones activas.
- Un **único hilo** del hub hace `metadata()` de todos los ficheros suscritos cada **400 ms**. Si pasan 30 s sin cambios, el intervalo sube a 1 s, y vuelve a 400 ms en cuanto algo crece.
- El estado de cada fichero es `{ ino, size, offset, partial: Vec<u8>, pending: Option<EntryBuilder>, idle_polls }`.
  - Si `size` es menor que `offset` → **truncado**: se vuelve a 0 y la UI recibe el marcador `truncated`.
  - Si cambia `ino` → **rotado o recreado**: se termina de leer el fichero viejo si sigue abierto, y después se abre el nuevo.
  - Si aparece un fichero nuevo que encaja con el patrón, por ejemplo `laravel-2026-11-07.log` a medianoche, se añade. Los directorios se listan cada 5 s, no en cada sondeo.
- La entrada pendiente se **cierra** cuando llega la siguiente cabecera o tras **un sondeo sin crecimiento**. Así el último error aparece en menos de 1 s.
- Las entradas nuevas se mandan en **lotes** como mucho cada 100 ms, con un máximo de 500 por lote. Si la UI no las consume (el canal falla o hay más de 5.000 en cola), se descartan y se manda `{ dropped: N }`.

**Memoria.**
- Un anillo `VecDeque<Entry>` de como mucho 5.000 entradas por fichero, con un tope global de 40 MB estimado como `size_of::<Entry>()` más la longitud de los `String`.
- Lo que sale del anillo se vuelve a leer del disco por `id`.

**Ciclo de vida.**
- `LogHub` vive en `Mutex<Option<LogHub>>`, dentro del estado de Tauri.
- Se crea con el primer `logs_*` y se suelta (hilo terminado y ficheros cerrados) en `WindowEvent::Destroyed` de la ventana `logs`.
- Cuando la vista deja de verse (`visibilitychange` en el frontend), la UI cancela su suscripción y el hilo de sondeo se detiene. La ventana minimizada no gasta CPU.

## 6. API pública de la crate (la comparten Tauri y el MCP)

```rust
pub struct LogQuery {
    pub projects: Vec<String>,        // vacío = todos; "none" = sin proyecto; "system" = PHP-FPM
    pub kinds: Vec<SourceKind>,       // vacío = todas
    pub min_level: Option<Level>,
    pub text: Option<String>,
    pub status: Option<StatusFilter>, // Class(4) | Class(5) | AtLeast(u16)
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub before: Option<String>,       // cursor
    pub limit: u32,                   // por defecto 200, con tope de 2000
}
pub struct Page { pub entries: Vec<Entry>, pub next_before: Option<String>, pub scanned_bytes: u64 }

pub struct EntryDetail {
    pub entry: Entry,
    pub body: String,                 // entrada completa, como mucho 1 MB
    pub frames: Vec<Frame>,           // { index, file, line, call, vendor, framework }
}
pub struct Group {
    pub fingerprint: String, pub count: u32,
    pub first_ms: i64, pub last_ms: i64,
    pub sample: Entry, pub projects: Vec<String>,
}
pub struct ProjectSummary {
    pub project: Project, pub sources: Vec<SourceStatus>,
    pub errors_24h: u32, pub last_ms: Option<i64>,
}

pub fn summaries(home: &Path) -> Vec<ProjectSummary>;
pub fn query(home: &Path, q: &LogQuery) -> Result<Page, String>;
pub fn entry(home: &Path, id: &str) -> Result<EntryDetail, String>;
pub fn groups(home: &Path, q: &LogQuery) -> Result<Vec<Group>, String>;
pub fn redact(text: &str) -> String;  // para el MCP; §7
pub struct LogHub { /* reutiliza anillos y estado de sondeo entre llamadas */ }
```

- Las funciones libres sirven al MCP sin estado. `LogHub` envuelve las mismas operaciones con caché y añade `subscribe`.
- Los mensajes de error van en español, como en el resto de la app.
- `errors_24h` cuenta las entradas de nivel Error o superior de las últimas 24 h. Usa la búsqueda binaria por tiempo y lee como mucho 4 MB por fichero; si llega al tope, la UI muestra "500+".

### 6.1 Comandos Tauri (`src-tauri/src/logs.rs`, registrados en `lib.rs`)

Siguen el estilo actual: `#[tauri::command] async`, `Result<_, String>` y el trabajo dentro de `blocking(...)`.

| Comando | Devuelve |
|---|---|
| `logs_projects()` | `Vec<ProjectSummary>` |
| `logs_query(q: LogQuery)` | `Page` |
| `logs_entry(id: String)` | `EntryDetail` |
| `logs_groups(q: LogQuery)` | `Vec<Group>` |
| `logs_subscribe(q: LogQuery, on_batch: tauri::ipc::Channel<Batch>)` | `u32` (id de la suscripción) |
| `logs_unsubscribe(id: u32)` | `()` |
| `logs_add_root(path: String)` / `logs_remove_root(path: String)` | `Vec<ProjectSummary>` |
| `logs_set_override(project: String, o: ProjectOverride)` | `Vec<ProjectSummary>` |
| `logs_clear(file_key: String)` | `()`. Vacía el fichero con `set_len(0)`. **Solo** para ficheros de proyectos o de Elephant, nunca para los de Herd. La UI pide confirmación antes. |
| `open_in_editor(file: String, line: u32)` | `()`. Abre `phpstorm://open?file=…&line=…`, `vscode://file/…:line` o `cursor://file/…:line` con `open`, según `settings.editor`, que por defecto es `vscode`. |

`Batch` es `{ entries: Vec<Entry>, dropped: u32, truncated: Vec<String> }`.

## 7. Contrato con el MCP

El binario y el transporte del MCP los define su propia decisión. Aquí se fija **qué expone sobre logs**. Todas estas herramientas son de **solo lectura** y no hay `clear`.

| Herramienta | Llama a | Notas |
|---|---|---|
| `list_projects` | `summaries` | nombre, hosts, tipo, versión de PHP y `errors_24h` |
| `search_logs {project?, level?, text?, since?: "15m"/"2h"/"1d", status?, limit? (20 por defecto, 200 como máximo)}` | `query` | `project` acepta nombre, host o id |
| `recent_errors {project?, since? = "1h"}` | `groups` con `min_level = Error` | la respuesta más útil para un LLM |
| `get_log_entry {id}` | `entry` | traza recortada |

- **Recorte para ahorrar tokens**:
  - mensaje de 500 caracteres como máximo;
  - en el detalle, los 8 primeros frames que **no** son de vendor, más el primero de vendor;
  - el resto se resume como `"+N frames de vendor"`.
- **Ocultación** con `redact()`, aplicada **siempre** antes de devolver texto al LLM:
  - valores de claves JSON o `clave=valor` que contengan `password`, `passwd`, `secret`, `token`, `api_key`, `apikey`, `authorization`, `cookie` o `session`, sin distinguir mayúsculas;
  - `Bearer <...>`;
  - URLs con credenciales `scheme://user:pass@`;
  - todo eso se sustituye por `«oculto»`.
- El MCP **no necesita la app abierta**: usa las funciones libres, sin `LogHub`.

## 8. Contrato con el futuro nginx de Elephant

Cuando Elephant gestione su propio nginx de Homebrew, el que lo implemente debe generar esto:

```nginx
log_format elephant escape=json '{"t":"$time_iso8601","h":"$host","m":"$request_method","u":"$request_uri","s":$status,"b":$body_bytes_sent,"rt":$request_time,"urt":"$upstream_response_time","ip":"$remote_addr","ua":"$http_user_agent"}';
access_log /Users/<usuario>/.elephant-stack/logs/nginx/access.log elephant buffer=32k flush=1s;
error_log  /Users/<usuario>/.elephant-stack/logs/nginx/error.log warn;
```

- Habrá **un solo fichero global**: el proyecto sale de `h`. Así hay menos descriptores y la configuración de cada sitio no tiene que tocar los logs.
- `rt` y `urt` van en segundos con decimales y se convierten a `duration_ms` y `upstream_ms`. `urt` puede ser `"-"` o una lista separada por comas; se toma la suma, o `None`.
- **Rotación**: al arrancar o reiniciar nginx desde Elephant, si un fichero pasa de 50 MB se renombra a `*.1` (pisando el anterior) y se ejecuta `nginx -s reopen`. El sondeo lo ve como un cambio de `ino`.

## 9. Interfaz: ventana "Registros"

- **Ventana propia** con etiqueta `logs`:
  - se crea bajo demanda con `WebviewWindowBuilder` y `WebviewUrl::App("logs.html")`;
  - mide 1100×720, con un mínimo de 760×480, `titleBarStyle: Overlay` y `hiddenTitle`;
  - se destruye al cerrarla, igual que `main`.
- La ventana principal no cambia de tamaño. Hay que hacer estos cambios:
  - añadir `logs.html` como segunda entrada en `vite.config.ts` (`build.rollupOptions.input`);
  - añadir `"logs"` a `windows` en `capabilities/default.json`;
  - mantener la CSP como está (solo `ipc:`).
- La ventana de logs **no** debe cambiar la política del Dock. Cerrarla no debe ocultar la app si `main` sigue abierta. `show_in_dock(false)` solo se llama cuando no queda ninguna ventana.
- **Distribución:**
  - **Barra lateral** con "Todos", la lista de proyectos, "Sin proyecto" (hosts de nginx sin sitio, agrupados por host) y "Sistema" (PHP-FPM). Cada proyecto muestra un punto y un contador si tiene errores en las últimas 24 h. Al pie, "Añadir carpeta…".
  - **Barra de herramientas**:
    - fuente: App · HTTP · nginx · PHP-FPM;
    - niveles: Debug · Info · Aviso · Error+;
    - búsqueda, con 150 ms de espera;
    - periodo: 15 min · 1 h · Hoy · 7 días · todo;
    - interruptor "En directo";
    - vista "Agrupados" o "Cronológico".
  - **Lista virtualizada propia** (TS sin framework):
    - filas de 28 px fijos, unas 60 filas en el DOM como máximo y nodos reciclados;
    - al desplazarse hacia el final, se carga la página siguiente con `before`;
    - la fila de app muestra hora, pastilla de nivel, canal, primera línea y clase de la excepción;
    - la fila HTTP muestra método, estado coloreado (2xx neutro, 3xx azul, 4xx ámbar, 5xx rojo), ruta y duración (en negrita si pasa de 1 s).
  - **Detalle**, en un panel inferior o lateral redimensionable:
    - la traza con los frames propios resaltados;
    - el bloque de vendor plegado, con el texto "N frames de vendor";
    - cada frame propio con un botón "Abrir en el editor";
    - "Copiar" (texto) y "Copiar como Markdown";
    - el contexto JSON, si se puede parsear, con formato.
  - **Estados vacíos** explicativos. En concreto: "Herd no guarda logs de acceso (access_log off). Verás peticiones cuando Elephant gestione nginx."
- **Accesos directos**:
  - "Ver registros" en la tarjeta o fila de cada proyecto abre la ventana filtrada (`logs.html?project=<id>`, o un evento `logs:focus` si ya estaba abierta);
  - en el menú de la barra, "Registros…" y, si hay errores, "Últimos errores (N)";
  - el contador del menú se recalcula en `TrayIconEvent::Enter`. No hay ningún temporizador.
- **Diseño**: usa los tokens, la tipografía y los componentes que fije la decisión del rediseño, con claro y oscuro (`prefers-color-scheme`) y sin estilos propios que contradigan ese sistema.
- Teclado:
  - `⌘F` lleva a la búsqueda;
  - `↑`/`↓` mueven entre entradas;
  - `↩` abre el detalle;
  - `⌘L` activa o desactiva el directo;
  - `⌘K` vacía, con confirmación.

## 10. Criterios de aceptación

**Funcionales**, cada uno con test de fixture salvo que se indique otra cosa:

1. Un `laravel.log` con una excepción, una `[previous exception]`, un contexto con saltos de línea y una entrada final a medio escribir da exactamente N entradas, con la clase, el mensaje, el fichero y la línea correctos. La entrada a medias se completa cuando llega el resto.
2. Con `laravel-2026-11-06.log` y `laravel-2026-11-07.log` en `storage/logs/`, ambos aparecen y se fusionan por fecha. Un fichero `daily` nuevo creado con la vista abierta aparece en menos de 6 s.
3. Una línea de `nginx-error.log` de Herd con `host: "tienda.test"` se asigna al proyecto cuyo `hosts` contiene `tienda.test`. Un host desconocido va a "Sin proyecto" y conserva el host.
4. Truncar (`: > laravel.log`) con el directo activo no duplica ni pierde entradas posteriores. Rotar (renombrar y crear de nuevo) tampoco.
5. Un `laravel.log` de 2 GB generado abre la primera página leyendo 4 MB como máximo (`scanned_bytes`). `since: 1h` usa la búsqueda binaria, y se comprueba que lee menos de 10 MB.
6. `entry(id)` sobre un fichero que ha cambiado devuelve el error en español, no contenido equivocado.
7. Un proyecto con `config/app.php` en `'timezone' => 'UTC'` y el Mac en Europe/Madrid muestra la hora local correcta, y se ordena bien frente a nginx.
8. El descubrimiento con un fixture de `valet/config.json` (aparcados, `Sites/` con symlinks y `tld` distinto de `test`) da los proyectos, hosts y orígenes esperados, sin duplicados por symlink.
9. `redact()` oculta los casos de §7 y no altera un mensaje sin secretos.
10. Ninguna función de la crate escribe fuera de `~/.elephant-stack/`, salvo `logs_clear`. Se comprueba con un test que ejecuta descubrimiento y consultas sobre un `HOME` temporal de solo lectura.

**Rendimiento**: tests `#[ignore]` de rendimiento sobre fixtures generados y medición manual con `monitor.rs`.

| Situación | Objetivo |
|---|---|
| Ventana de Registros cerrada | 0 hilos, 0 descriptores y 0 temporizadores del módulo |
| Ventana minimizada u oculta | sin hilo de sondeo |
| Abrir la vista con 20 proyectos | menos de 150 ms hasta la primera página |
| En directo, a 100 líneas/s | menos del 1 % de CPU en el backend |
| Directo sin actividad | menos del 0,2 % de CPU |
| Memoria del hub | menos de 40 MB |
| Texto sobre 100 MB | menos de 150 ms (M1 o superior, caché de disco caliente) |
| MCP `recent_errors` en frío, con 20 proyectos | menos de 300 ms |
| Lista con 5.000 entradas | desplazamiento a 60 fps sin tirones en WebKit |

## 11. Fases

1. **Crate `elephant-logs`**: `projects`, parsers y `reader`, con fixtures y tests 1, 5, 6, 7, 8 y 10.
2. **Comandos Tauri y ventana Registros** sin directo: lista, detalle, filtros, agrupados y "Abrir en el editor".
3. **Directo**: sondeo, `Channel`, truncado y rotación (tests 2 y 4) y contador en el menú de la barra.
4. **MCP**: herramientas de §7 y `redact` (test 9).
5. **nginx propio de Elephant**: `log_format elephant`, rotación y fuente `NginxAccess`. Depende de la decisión de nginx.

## 12. Riesgos y mitigaciones

- **Más código propio que mantener.** Se mitiga con el alcance cerrado (§3), parsers como funciones puras con fixtures reales, sin dependencias pesadas, y la lista de aceptación como test.
- **Formatos personalizados de Monolog** (otro `format` o `dateFormat` en `config/logging.php`, `tap`). El reconocedor de cabecera es tolerante con la fecha y la zona, pero un formato muy distinto no se reconocerá. En ese caso el fichero aparece como "formato no reconocido": no se pierde nada, cada línea se muestra como una entrada `Info`.
- **Canales fuera de `storage/logs`** o en `stderr`/`syslog`. No se detectan solos; se añaden con `log_globs`. Leer `config/logging.php` con artisan queda fuera por coste.
- **Zona horaria mal deducida.** Es heurística (`config/app.php` y `.env`) y puede fallar si la zona se calcula en código. Se mitiga con `overrides.timezone` editable desde la UI ("Zona horaria de los logs").
- **Sin índice**: buscar texto sobre semanas de varios GB es lineal y puede tardar segundos. Se mitiga con la búsqueda binaria por tiempo, un periodo por defecto de "Hoy" y un indicador de progreso (`scanned_bytes`). FTS5 queda como opción futura detrás de la misma API.
- **Avisos de privacidad (TCC)** con proyectos en `~/Documents`, `~/Desktop` o `~/Downloads`. La primera lectura dispara el aviso del sistema. Se explica en el estado vacío y, si se deniega, la fuente sale como `unreadable` sin bloquear el resto.
- **Herd cambia de formato o de rutas.** Todo lo de Herd está aislado en `projects.rs` y en la tabla de fuentes; si falta, se ignora sin error. Los fixtures del `config.json` real fijan el contrato.
- **No hay logs HTTP de acceso con Herd.** Es un límite de Herd (`access_log off`) y Elephant no lo cambia. La UI lo dice claramente; los logs de acceso llegan en la fase 5.
- **La ocultación del MCP es heurística** y puede dejar pasar datos sensibles del contexto. Se aplica siempre, hay tests de casos frecuentes y la documentación del MCP lo advierte.
- **`TrayIconEvent::Enter` puede llegar tarde** respecto a la apertura del menú. En ese caso el contador muestra el valor de la consulta anterior. Se acepta para no tener un temporizador en segundo plano.
- **`logs_clear` es destructivo**: requiere confirmación, solo actúa sobre ficheros de proyectos o de Elephant y no existe en el MCP.
