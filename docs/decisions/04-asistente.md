# 04 · Asistente inicial

- Estado: aceptada
- Fecha: 2026-11-06
- Debate: propuesta A (tres escenarios Laravel opinados, todo apagado salvo lo imprescindible, plan que solo suma) contra propuesta B (escenarios por consumo, detección previa y migración reversible desde Herd).
- Relacionada con: [03 · Servidor MCP local](03-mcp.md) (`elephant-core`, `lock::brew()`, `stamp::touch`) y la decisión de sitios, nginx y dnsmasq, que aún no está escrita.

## 1. Decisión

El asistente ofrece **tres escenarios fijos más uno personalizado**. Cada escenario es un **estado deseado**. El backend lo compara con el sistema real y obtiene un **plan**, que es una función pura y por tanto idempotente. El usuario revisa el plan entero antes de aplicarlo.

| | **Laravel** (recomendado) | **Laravel con servicios** | **Solo terminal** |
|---|---|---|---|
| Frase de la tarjeta | «Proyectos con SQLite. Nada en marcha.» | «MySQL o PostgreSQL, Redis y correo de pruebas.» | «PHP y Composer en la terminal. Herd sigue sirviendo tus sitios.» |
| PHP propio | Una versión propia. Si ya tienes alguna, vale esa. Si no, la última de php-builds (hoy 8.5.12) | Igual | Las **dos últimas ramas** (hoy 8.5.12 y 8.4.27) |
| PHP por defecto | La global actual si existe; si no, la descargada | Igual | Igual |
| Composer | `composer.phar` propio | Igual | Igual |
| Selector `php` en el PATH | Sí | Sí | Sí |
| Base de datos | Ninguna: SQLite va dentro de PHP | **MySQL o PostgreSQL** (control segmentado), instalada y **apagada** | Ninguna |
| Redis | No | Instalado y **apagado** | No |
| Mailpit | Instalado y **apagado** | Instalado y **al iniciar sesión** | No |
| nginx y dnsmasq | Ninguno en esta versión (ver §4.9) | Igual | Nunca |
| Abrir Elephant al iniciar sesión | Sí, solo en la barra de menús | Sí | No |
| Xdebug y `php.ini` | Sin cambios | Sin cambios | Sin cambios |
| En reposo | ≈ 0 MB | ≈ 25 MB (Mailpit). La base de datos solo consume al encenderla | 0 MB |

**Personalizado** usa la misma tabla, editable. Además, la primera pantalla tiene siempre el botón **«Ya tengo mi entorno: no cambiar nada»**.

Principios que no se negocian:

1. **El plan solo suma.** No hay paso de desinstalar. Nada se apaga ni pierde su arranque automático salvo que el usuario marque una casilla que aparece **desmarcada** por defecto.
2. **El backend calcula el plan y lo vuelve a calcular al aplicarlo.** El frontend solo envía el escenario elegido y las claves de los pasos opcionales que acepta.
3. **No se toca lo que no es de Elephant.** No se modifica Herd, no se escribe en `/usr/local/etc/php` y no se usa `sudo`. Fuera de `~/.elephant-stack` solo se escribe en dos sitios: `~/.zshrc`, si el usuario pulsa «Añadirlo por mí» y después de ver el diff, y el LaunchAgent de inicio de sesión de la propia app.

**Base:** propuesta A. De B se incorporan la pantalla de detección, el selector de base de datos dentro del escenario, la RAM medida con `monitor.rs` cuando el servicio ya está en marcha, el botón «No cambiar nada», el archivo de estado aparte de `settings.json`, el plan con identificador para detectar que ha quedado obsoleto y la detección de puertos con `connect()` en lugar de `lsof`.

### 1.1 Motivos y comprobaciones

| Afirmación | Quién | Resultado | Cómo se comprobó |
|---|---|---|---|
| `brew install composer` instala también la fórmula `php` | A (B propone composer de brew) | **Cierto**: B queda descartado en este punto | `brew info --json=v2 composer` → `dependencies: ["php"]` |
| Los PHP de php-builds traen las extensiones compiladas (pdo_sqlite, pdo_mysql, pdo_pgsql, redis, intl, gd, imagick…) | A y B | Cierto | `php-builds/extensions.txt` |
| Solo existe 8.4.27, compilada en local, y el CI nunca se ha ejecutado | A | **Falso a día de hoy** | `gh release list -R cristiancosano/php-builds`: php-8.5.12 (Latest), php-8.3.36 y php-8.4.27, publicadas el 2026-11-05 |
| Cada release trae `<paquete>.sha256` | B | **Casi**: la macOS de 8.4.27 solo tiene `SHA256SUMS` | `gh release view php-8.4.27 --json assets` |
| Tamaño de descarga de cada PHP | — | ≈ 65 MB comprimido; ≈ 180 MB en disco (php, php-fpm y xdebug.so) | Assets de la release; `php-builds/README.md` |
| Se pueden aplicar ajustes de `php.ini` | B | **No sin cambiar antes el selector**. El PHP estático busca `/usr/local/etc/php` y `/usr/local/etc/php/conf.d`, que son rutas del sistema y compartidas | `~/.elephant-stack/php/8.4.27/php --ini` |
| El skeleton de Laravel funciona sin ningún servicio | A | Cierto: `DB_CONNECTION=sqlite`, drivers `database` y `MAIL_MAILER=log` | `.env.example` de laravel/laravel 12.x |
| Que exista `REDIS_HOST` indica que se usa Redis | A | **Falso**: el skeleton trae `REDIS_HOST=127.0.0.1` siempre. `MAIL_PORT` viene a `2525` | El mismo `.env.example` |
| `brew install` no toca otras fórmulas | implícito en A y B | **Falso**: si no se define `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK`, ejecuta `upgrade` o `reinstall` de dependientes ya instalados | `man brew`, sección `install` (Homebrew 7.0.12) |
| `lsof` sin root no ve procesos de otros usuarios | B | Se acepta. Se usa `connect()` para saber si el puerto está ocupado y `ps` para adivinar de quién es | En este Mac, `dnsmasq` de Herd corre como `nobody` y su helper como `root`. Hay otro usuario, `cristian-smw`, con un php-fpm de Herd |
| Buscar la marca de Elephant en `~/.zshrc` basta para saber si el selector está activo | B | **Insuficiente**: aquí la línea apunta a `~/.localdeck/bin`, un enlace simbólico a `~/.elephant-stack` | `~/.zshrc`, línea 27; `ls -ld ~/.localdeck` |
| Postgres consume ≈ 40-50 MB en reposo | A y B | Medido: **≈ 16 MB** de RSS sumado (postgresql@18 en marcha) | `ps -axo rss,comm` |
| `components::install("composer")` y `("php")` ya existen y usan brew | — | Cierto. Hoy la app puede arrastrar un PHP de Homebrew desde la lista de componentes | `components.rs`, `SPECS` |

Por qué A y no B como base:

- **Rendimiento.** «Todo incluido» de B arranca las dos bases de datos al iniciar sesión (≈ 550 MB en reposo). Eso contradice el criterio principal. Quien lo quiera lo tiene en Personalizado.
- **Honestidad del alcance.** El escenario recomendado de B (Web Laravel) depende de nginx, dnsmasq, sitios y una CA local que todavía no existen. Recomendar una tarjeta marcada como «Próximamente» no tiene sentido. Los tres escenarios de A funcionan hoy.
- **Seguridad.** B detiene servicios por defecto al bajar de escenario. A solo lo propone como paso opcional desmarcado. Además, la migración de B implica leer y emular la configuración interna de Herd: es trabajo de la decisión de sitios, no del asistente.
- **Implementación.** Tres estados por servicio frente a tres columnas encadenadas; sin `osascript`, sin pasos privilegiados y sin manifiestos de migración.

## 2. Qué se descarta y por qué

- **Composer con Homebrew** (B). Arrastra `php` de brew y sus dependencias. Además, **`php` y `composer` dejan de ser `installable` en `SPECS`**: el botón «Instalar» de la fila de composer usa el instalador del phar (§4.5), y la fila de `php` remite a la sección PHP.
- **Escenario «Todo incluido» con todo al iniciar sesión** (B). Va contra el rendimiento. Se puede montar con Personalizado.
- **Migración de sitios desde Herd, CA propia, `/etc/resolver` y `osascript … with administrator privileges`** (B). Pertenecen a la decisión de sitios. Cuando llegue, el asistente ganará esos pasos a través de `SETUP_VERSION` (§4.8).
- **Ajustes de `php.ini` y Xdebug en el asistente** (B, y A en parte). Sin `PHP_INI_SCAN_DIR` en el selector no hay dónde escribirlos sin tocar `/usr/local/etc/php`. Xdebug desactivado es el estado por defecto y no requiere ningún paso. El interruptor de Xdebug y los ajustes de ini irán en la sección PHP, cuando el selector exporte `PHP_INI_SCAN_DIR=~/.elephant-stack/php/<versión>/conf.d`.
- **Opción «Usar el PHP de Herd en vez de descargar»** (A). Añade una bifurcación a la revisión, a cambio de ahorrar 65 MB. Quien no quiera PHP propio lo desmarca en Personalizado.
- **Detener el plan en el primer error con «Reintentar / Saltar / Terminar»** (B). Obliga a vigilar la pantalla. Se sigue con los pasos independientes, los dependientes se saltan y al final se ofrece «Reintentar lo que falta» (A).
- **Casilla «Desinstalar lo que ya no uso»** (B). El asistente nunca desinstala. Para eso está la lista de componentes.
- **Aviso de desviación al arrancar la app** (B). Exige llamar a brew en cada arranque. La desviación se calcula al abrir Ajustes ▸ Escenario.
- **Índices de pasos como identificador** (A). Si el plan cambia entre la revisión y la aplicación, un índice puede apuntar a otro paso. Cada paso tiene una clave estable.
- **`brew update` siempre**. Solo se ejecuta si el plan tiene algún `BrewInstall`.
- **Heurística por `REDIS_HOST` y `MAIL_PORT=1025`** tal como la escribe A. Ver §4.10.

## 3. Archivos

Siguiendo 03, la lógica vive en `elephant-core`, sin depender de Tauri. Si el workspace todavía no existe cuando se implemente, va en `src-tauri/src/setup/` con la misma estructura y se mueve después.

```
crates/elephant-core/src/setup/
├── mod.rs        # tipos públicos: Profile, Plan, Step, ApplyReport
├── presets.rs    # los tres escenarios como funciones constantes + validación de Custom
├── detect.rs     # Snapshot (solo lectura)
├── catalog.rs    # releases de php-builds y versión estable de Composer (red, caché)
├── plan.rs       # plan(&Snapshot, &Profile, &Catalog) -> Plan   ← puro, con tests
├── apply.rs      # ejecuta un Plan, emite eventos, escribe el log
├── download.rs   # curl + sha256 + tar con lista blanca + rename atómico
└── state.rs      # ~/.elephant-stack/setup.json
src-tauri/src/lib.rs          # comandos setup_* (envoltorios)
src/setup.ts                  # pantallas del asistente
src/styles.css                # estilos del asistente con los tokens del rediseño
```

Dependencias nuevas: `sha2` (verificación, Rust puro) y `tauri-plugin-autostart` (solo en `src-tauri`). Para descargar se usa `/usr/bin/curl` y para extraer `/usr/bin/tar`, que vienen con macOS. No se añade ningún cliente HTTP.

Cambios en el código existente:

- `components.rs`: `php` y `composer` pasan a `installable: false`.
- `paths::search_dirs()`: `~/.elephant-stack/bin` va **primero**.
- `tools::origin_of()`: una ruta que contiene `/.elephant-stack/` es `"Elephant Stack"`.
- `brew.rs`: variante `Operation::TurnOnWithAutostart` → `["start"]` (con test en `brew::plan`); nueva `run_streaming(args, lock, on_line)`, que lee stdout y stderr línea a línea, y `install_formula` con `HOMEBREW_NO_AUTO_UPDATE=1`, `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1` y `HOMEBREW_NO_ENV_HINTS=1`. Todas las escrituras toman `lock::brew()` (03).
- El selector `php` se empaqueta como sidecar (`bundle.externalBin`) para que la app pueda instalarlo sin `cargo`.

## 4. Diseño

### 4.1 Modelo de datos

```rust
// presets.rs / mod.rs
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioId { Laravel, LaravelServices, Terminal, Custom }

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Database { Mysql, Postgres }

/// Estado deseado de un servicio. `Keep` = no tocar (no significa desinstalar).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Want { Keep, Installed, Autostart }

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum PhpWant {
    /// Cualquier versión propia vale; si no hay ninguna, la última del catálogo.
    AnyOwn,
    /// Ramas concretas («8.5», «8.4»); cada una se resuelve al último parche del catálogo.
    Branches(Vec<String>),
    /// Elegido en Personalizado: no instalar PHP propio.
    None,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Profile {
    pub scenario: ScenarioId,
    pub php: PhpWant,
    pub composer: bool,
    pub shim: bool,
    /// Solo ids de SPECS con kind == Service e installable.
    pub services: BTreeMap<String, Want>,
    pub launch_at_login: bool,
}

/// Lo que puede enviar el frontend. Nunca un Profile de un preset ya resuelto.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "scenario")]
pub enum ProfileInput {
    Laravel,
    LaravelServices { database: Database },
    Terminal,
    Custom { profile: Profile },
}
```

`presets::resolve(input, &Snapshot, &Catalog) -> Result<Profile, String>`:

- `Laravel` → `php: AnyOwn`, `composer`, `shim`, `mailpit: Installed` y `launch_at_login: true`.
- `LaravelServices { database }` → como Laravel, más `redis: Installed`, `mailpit: Autostart` y la base de datos elegida en `Installed`, que se resuelve en mysql o postgresql@18.
- `Terminal` → `php: Branches([la última, la anterior])`, calculadas con el catálogo, composer, shim, sin servicios y `launch_at_login: false`.
- `Custom` → se valida:
  - cada clave de `services` es un id de `SPECS` con `installable` y `kind == Service`;
  - cada rama de `Branches` existe en el catálogo;
  - si no, devuelve `Err`.

**Base de datos preseleccionada** en Laravel con servicios: primero la que está en marcha, después la instalada y, si no hay ninguna, MySQL, porque es lo que usan por defecto Sail y Forge. Junto a cada opción se muestra su coste. En este Mac se preselecciona PostgreSQL.

**Equivalencias** (en `presets.rs`, con test): `mysql` se da por satisfecho con `mysql`, `mysql@*` o `mariadb`; `postgresql@18` con `postgresql` o cualquier `postgresql@*`. Se comprueba contra `brew list --formula`. El servicio sobre el que se actúa es el que está instalado, no el del preset.

### 4.2 Detección (`detect.rs`)

`detect() -> Snapshot`. Es de solo lectura y lanza las sondas en paralelo con `thread::scope`, como `components::list`. Objetivo: menos de 1,5 s.

```rust
pub struct Snapshot {
    pub arch_arm64: bool,
    pub formulas: Vec<String>,              // brew list --formula -1
    pub services: Vec<brew::Service>,       // brew services info --all --json
    pub php_versions: Vec<VersionInfo>,     // Layout::discover()
    pub global_php: Option<String>,         // Layout::global(), y si resuelve
    pub shim: FileState,                    // ~/.elephant-stack/bin/php: Missing | Same | Different (sha256 frente al sidecar)
    pub composer_phar: bool,                // ~/.elephant-stack/libexec/composer.phar
    pub path: PathState,                    // ver abajo
    pub ports: Vec<PortUse>,                // 3306, 5432, 6379, 1025, 8025
    pub herd: Option<HerdInfo>,             // ver abajo
    pub launch_at_login: bool,              // plugin autostart: is_enabled()
    pub data_dirs: Vec<String>,             // /opt/homebrew/var/mysql, /opt/homebrew/var/postgresql@*
}
pub enum PathState { Active, AfterHerdMissing, ShadowedByHerd, Missing }
pub struct PortUse { pub port: u16, pub busy: bool, pub owner_hint: Option<String> }
pub struct HerdInfo { pub running: bool, pub tld: Option<String>, pub sites: Option<usize> }
```

- **PATH:** se lee `~/.zshrc` y `~/.zprofile` línea a línea. Una línea `export PATH=…` cuya primera entrada, con `$HOME` expandido y **canonicalizada**, sea `~/.elephant-stack/bin` cuenta como nuestra. El estado es `Active` si está después de la línea `Herd/bin` o si no existe línea de Herd, y `ShadowedByHerd` si está antes. No se ejecuta una shell de login: es lenta y ejecuta código del usuario. En este Mac el resultado es `Active` a través de `~/.localdeck/bin`.
- **Puertos:** `TcpStream::connect_timeout(127.0.0.1:p, 150 ms)`. Para saber de quién es, si el servicio de brew equivalente está `running`, es suyo. Si no, se busca en `ps -axo user,comm` un proceso conocido (`Herd.app`, `DBngin`, `mysqld`, `postgres`, `redis-server`, `mailpit`) y se indica el usuario cuando no es el actual.
- **Herd:** `/Applications/Herd.app` existe; `running` si `ps` muestra algún proceso bajo `Herd.app` o `Herd/config`. `tld` y `sites` se leen **en el mejor caso** de `~/Library/Application Support/Herd/config/valet/config.json` (`tld`, `paths` y las subcarpetas de cada path). Si el formato cambia, quedan en `None`. Solo se leen; nunca se escribe en Herd.

### 4.3 Catálogo (`catalog.rs`)

```rust
pub struct Catalog { pub php: Vec<PhpRelease>, pub composer: Option<ComposerRelease>, pub offline: bool }
pub struct PhpRelease { pub version: String, pub url: String, pub size: u64, pub checksum_url: String }
pub struct ComposerRelease { pub version: String, pub url: String, pub checksum_url: String }
```

- PHP: `GET https://api.github.com/repos/cristiancosano/php-builds/releases?per_page=50`, a través de `/usr/bin/curl -sS -f --proto =https`. Se usan las etiquetas `php-X.Y.Z` con un asset `php-X.Y.Z-macos-arm64.tar.gz`. `checksum_url` apunta a `<asset>.sha256` si existe y, si no, a `SHA256SUMS`. Las dos tienen formato `hash  nombre` y se buscan por nombre de archivo. «Última» es la versión más alta, y «anterior» el último parche de la rama inmediatamente inferior.
- Composer: `https://getcomposer.org/versions` → `stable[0].version`. El phar se descarga de la URL **versionada** `https://getcomposer.org/download/<v>/composer.phar`, y su checksum, de `…/composer.phar.sha256sum`.
- Caché en memoria durante una hora, y en disco en `~/.elephant-stack/cache/catalog.json` para trabajar sin red. Sin red ni caché, `offline: true`: los pasos de descarga se convierten en `Conflict` («Sin conexión: no se puede descargar PHP») y el resto del plan sigue siendo válido.

### 4.4 Plan (`plan.rs`, puro)

```rust
pub fn plan(snapshot: &Snapshot, profile: &Profile, catalog: &Catalog) -> Plan;

pub struct Plan {
    pub id: String,              // sha256 de (snapshot normalizado, profile, catalog): detecta planes obsoletos
    pub scenario: ScenarioId,
    pub steps: Vec<Step>,        // ejecutables, en orden
    pub satisfied: Vec<String>,  // «Ya lo tienes»: «PostgreSQL 18 (Homebrew)», «PHP 8.4.27»
    pub notices: Vec<String>,    // «Herd sigue sirviendo tus 12 sitios .test; no se toca»
    pub conflicts: Vec<Conflict>,// no se ejecutan; se explican
    pub totals: Totals,          // download_bytes (PHP + Composer), brew_formulas, idle_ram_mb
}

pub struct Step {
    pub key: String,             // estable: "brew-install:redis", "autostart-off:postgresql@18"
    pub action: Action,
    pub group: Group,            // Download | BrewInstall | Configure | Start | Optional
    pub title: String,           // «Instalar Redis con Homebrew»
    pub command: Option<String>, // «brew install redis», visible en «Ver detalles»
    pub optional: bool,          // los opcionales vienen desmarcados
    pub after: Vec<String>,      // claves de las que depende
    pub restarts_service: bool,  // Homebrew reinicia el servicio al cambiar el arranque (brew::plan)
}

pub enum Action {
    BrewUpdate,
    BrewInstall { formula: String },
    DownloadPhp { release: PhpRelease },
    SetGlobalPhp { version: String },
    InstallShim,
    InstallComposer { release: ComposerRelease },
    Service { name: String, op: brew::Operation },
    LaunchAtLogin { enabled: bool },
}
pub struct Conflict { pub key: String, pub reason: String }
```

Reglas, en orden:

1. **Arquitectura:** sin arm64, todo paso de PHP se convierte en `Conflict` («Los PHP de Elephant son solo para Apple Silicon»).
2. **PHP:**
   - `AnyOwn`: si hay alguna versión propia, va a `satisfied`. Si no, `DownloadPhp(última)`.
   - `Branches(bs)`: para cada rama sin versión propia de esa rama, `DownloadPhp(último parche)`.
   - **Global:** si no hay global o no resuelve, `SetGlobalPhp` con la más alta propia o descargada, `after` la descarga. Si hay global y resuelve, no se cambia: si difiere de la más reciente del perfil, se añade el opcional `php-global:<v>` («Usar PHP 8.5.12 por defecto (ahora 8.4.27)»).
3. **Selector:** `shim: true` y `FileState != Same` → `InstallShim`. Si es `Different`, el título lo dice: «Actualizar el selector php».
4. **Composer:** `composer: true` sin phar → `InstallComposer`, `after` el selector y una descarga de PHP si la hay. Si existe composer de Herd o de brew, se añade a `notices`: «Tienes Composer de Herd; Elephant instala el suyo, que usa el PHP de cada proyecto».
5. **Servicios**, para cada `(id, want)`, con `id` resuelto por equivalencias:
   - Sin instalar y `want != Keep` → `BrewInstall`. Si hay `data_dirs` de ese motor: aviso «Hay datos de MySQL de una instalación anterior: se reutilizarán».
   - `Autostart`:
     - si ya está `registered`, va a `satisfied`;
     - si está en marcha, `Service(EnableAutostart)`;
     - si no lo está, `Service(TurnOnWithAutostart)`, una variante nueva de `brew::Operation` que en `brew::plan` es un solo `["start"]` en cualquier estado. Encadenar `TurnOn` y `EnableAutostart` costaría tres llamadas a brew y un reinicio inútil;
     - en los dos casos, el paso va `after` la instalación;
     - si el puerto está ocupado por otro, `Conflict` en vez de arrancar: «El 3306 lo usa mysqld de cristian-smw: no arranco el MySQL de Homebrew».
   - `Installed`:
     - si está `registered`, opcional `autostart-off:<id>` («PostgreSQL 18 arranca al iniciar sesión. ¿Quitar el arranque automático? Sigue encendido»), con `restarts_service = running`;
     - nunca se apaga.
   - `Keep`, y los servicios que no están en `SPECS`, no generan nada.
6. **`BrewUpdate`** al principio, solo si hay algún `BrewInstall`.
7. **Elephant al iniciar sesión:** si `launch_at_login` difiere del estado actual y es `true`, `LaunchAtLogin(true)`. Si es `false` y está activado, el paso es opcional y desmarcado.
8. **Herd presente** → `notices`: «Herd está instalado. Elephant convive con él: no toca sus sitios, su PHP ni sus servicios».
9. **PATH:**
   - `Missing` o `ShadowedByHerd` → `notices` con el texto exacto y la acción de interfaz «Añadirlo por mí» (§4.7). No es un `Step`: necesita su propia confirmación con diff.
   - `Active` → `satisfied`.

`totals.idle_ram_mb` suma la estimación de los servicios en `Autostart`: mailpit 25, redis 10, postgresql 20 y mysql 400, constantes en `presets.rs`. Si el servicio ya está en marcha, se usa su medida real con `monitor::usage_for`. Siempre se presenta con «≈».

### 4.5 Ejecución (`apply.rs` y `download.rs`)

```rust
pub enum StepState { Pending, Running, Ok, Failed, Skipped }
#[derive(Serialize, Clone)]
pub struct ProgressEvent {
    pub key: String, pub state: StepState,
    pub line: Option<String>,                 // última línea de brew o curl
    pub bytes: Option<(u64, u64)>,            // descargas: hecho/total
    pub reason: Option<String>,               // Failed/Skipped, ya traducido
}
pub struct ApplyReport { pub done: Vec<String>, pub failed: Vec<(String, String)>,
                         pub skipped: Vec<(String, String)>, pub log_path: PathBuf }
pub fn apply(plan: &Plan, accepted: &BTreeSet<String>, cancel: &AtomicBool,
             on_event: impl Fn(ProgressEvent) + Sync) -> ApplyReport;
```

- **Dos carriles:**
  - Homebrew: `BrewUpdate`, los `BrewInstall` uno a uno y luego los `Service`. Cada uno toma `lock::brew()`.
  - Archivos: las descargas de PHP, después el selector, Composer y la versión global.
  - Los dos carriles van en paralelo, y cada paso espera a sus `after`. `LaunchAtLogin` va al final.
- **Fallos:** un paso que falla marca como `Skipped` todos los que dependen de él, con motivo («No se instaló Mailpit»), y el resto continúa. No se aborta nada.
- **Cancelar:** cuando termina el paso en curso, los pendientes pasan a `Skipped («Cancelado»)`. Nunca se mata un `brew` a medias.
- **`DownloadPhp`:**
  1. `curl -fL --proto =https --tlsv1.2 -o ~/.elephant-stack/cache/<asset>.part`. El progreso se calcula sondeando el tamaño del archivo cada 250 ms contra `size`.
  2. Verificar sha256 con `sha2` contra el checksum. Si no coincide: `Failed («La descarga no coincide con su firma; no se ha instalado nada»)` y se borra el `.part`.
  3. `tar -tzf`: solo se aceptan `./`, `php`, `php-fpm`, `xdebug.so` y `SHA256SUMS`. Cualquier otra entrada, o una con `/` inicial o `..`, aborta el paso.
  4. Extraer en `~/.elephant-stack/php/.tmp-<v>`, comprobar que `php -n -r 'echo PHP_VERSION;'` devuelve `<v>` y renombrar a `~/.elephant-stack/php/<v>`. Si el destino ya existe, el paso cuenta como `Ok`.
- **`InstallComposer`:** misma verificación sobre `~/.elephant-stack/libexec/composer.phar`, más un envoltorio `~/.elephant-stack/bin/composer` (0755):

  ```sh
  #!/bin/sh
  exec "$HOME/.elephant-stack/bin/php" "$HOME/.elephant-stack/libexec/composer.phar" "$@"
  ```

  Así Composer usa siempre el PHP que resuelve el selector para la carpeta actual. `composer self-update` sigue funcionando sobre el phar. El asistente nunca actualiza Composer.
- **`InstallShim`:** copia el sidecar a `~/.elephant-stack/bin/php.tmp`, `chmod 0755` y `rename`.
- **`Service`:** `brew::apply(name, op)` en modo streaming.
- **`LaunchAtLogin`:** `tauri-plugin-autostart` con `MacosLauncher::LaunchAgent` y el argumento `--hidden`. Con ese argumento, `lib.rs` no crea la ventana y deja la app solo en la barra de menús, en política `Accessory`.
- **Log:** se escribe todo stdout y stderr en `~/.elephant-stack/logs/setup-AAAAMMDD-HHMMSS.log`, el mismo directorio que leerá la herramienta de logs.
- **Mensajes:** una tabla `translate(stderr) -> Option<&str>` traduce los errores conocidos:
  - sin red (`Could not resolve host`, `Failed to connect`, curl 6 o 7): «Sin conexión a Internet»;
  - `xcrun: error`: «Faltan las Command Line Tools de Xcode: ejecuta xcode-select --install»;
  - `already locked`: «Homebrew está ocupado con otra operación»;
  - `No space left`: «No queda espacio en disco».

  Si no se reconoce, se muestra la última línea de stderr y el log.
- Al terminar: `stamp::touch("components")` y `stamp::touch("php")`, para que la app, el tray y el MCP refresquen su estado (03).

### 4.6 Comandos Tauri

| Comando | Entrada | Salida | Notas |
|---|---|---|---|
| `setup_status` | — | `{ needed: bool, state: Option<SetupFile> }` | Solo lee `setup.json`. Barato; se llama al abrir la ventana |
| `setup_detect` | — | `Snapshot` (serializable) | Alimenta la pantalla de bienvenida |
| `setup_preview` | `ProfileInput` | `Plan` | `detect` + `catalog` + `resolve` + `plan`. Guarda el `Plan` en `State<PendingPlan>` |
| `setup_apply` | `plan_id`, `accepted: Vec<String>`, `on_event: Channel<ProgressEvent>` | `ApplyReport`, o `Err(Stale(Plan))` | Vuelve a detectar y a planificar. Si el `id` no coincide, no ejecuta nada y devuelve el plan nuevo para revisarlo otra vez. `accepted` solo puede contener claves de pasos `optional` de ese plan. Rechaza una segunda ejecución si ya hay una en marcha (`Mutex<bool>`) |
| `setup_cancel` | — | — | Activa el `AtomicBool` |
| `setup_skip` | — | `SetupFile` | «No cambiar nada»: guarda `scenario: null` |
| `setup_shell_preview` | — | `{ path: String, diff: String }` | Diff del bloque que se añadiría |
| `setup_shell_apply` | `diff_hash` | — | Solo si el hash coincide con el último `preview` |

El frontend no envía nunca pasos, rutas, fórmulas ni versiones fuera de un `Profile` de Personalizado, y ese `Profile` se valida en `resolve`.

### 4.7 PATH en `~/.zshrc`

`setup_shell_preview` construye este bloque, que se añade **al final** del archivo (por tanto, después de la línea de Herd):

```sh
# >>> elephant-stack >>>
export PATH="$HOME/.elephant-stack/bin:$PATH"
# <<< elephant-stack <<<
```

- La interfaz muestra el diff, un botón «Copiar» y otro «Añadirlo por mí».
- Al aplicarlo, se copia el archivo a `~/.zshrc.elephant-bak-AAAAMMDD-HHMMSS`, se escribe en un temporal del mismo directorio y se hace `rename`, conservando los permisos.
- Si el bloque ya existe con otra ruta, se sustituye el bloque y no se añade otro.
- **Nunca** se borra ni se modifica la línea de Herd ni una línea antigua como la de LocalDeck. Si hay una línea antigua equivalente, el estado es `Active` y no se propone nada.
- Después se avisa: «Abre una terminal nueva para que surta efecto».

### 4.8 Estado y nueva ejecución

`~/.elephant-stack/setup.json`, separado de `settings.json` para que `Settings` siga siendo `Copy`:

```json
{
  "version": 1,
  "scenario": "laravel_services",
  "database": "postgres",
  "custom": null,
  "completed_at": "2026-11-06T10:20:00Z"
}
```

- `scenario: null` significa «no cambiar nada». Un archivo dañado equivale a uno ausente, como hace `settings::load`.
- **Primera vez:** si `setup.json` no existe y la ventana se abre (no con `--hidden`), se muestra el asistente. El botón «No cambiar nada» está siempre visible, así que quien ya tiene su entorno sale con un clic.
- **Ajustes ▸ Escenario:**
  - muestra el escenario actual y, **solo al abrir esa sección**, calcula `setup_preview` con lo guardado;
  - «Al día» si `steps` está vacío; «N diferencias» si no lo está;
  - botones: «Revisar y aplicar» (va directo a la revisión) y «Cambiar de escenario» (va a la elección, con el actual marcado).
- **Bajar de escenario** (por ejemplo, de Laravel con servicios a Laravel) no desinstala ni apaga nada. Solo aparecen opcionales desmarcados para quitar el arranque automático.
- **`SETUP_VERSION`:** cada preset marca sus pasos con `since: u32`. Si `setup.json.version` es menor que la constante de la app, se muestra una hoja «Novedades para tu escenario» con un plan filtrado a los pasos `since > version`. Al aceptar o al descartar, se guarda la versión nueva. Así entrarán nginx y dnsmasq cuando la decisión de sitios lo permita.
- El menú de la barra puede incluir «Asistente de configuración…» y abrir directamente la elección de escenario. Lo decide la decisión del tray.

### 4.9 nginx, dnsmasq y Herd

Ningún escenario instala ni arranca nginx o dnsmasq en esta versión. La revisión muestra siempre uno de estos avisos:

- con Herd: «Herd sigue sirviendo tus sitios .test»;
- sin Herd: «Elephant todavía no sirve sitios .test; llegará en una próxima versión».

Cuando exista la gestión de sitios, sus pasos (y, si procede, la migración desde Herd) entrarán con `since: 2`. El asistente no lee la configuración de Herd salvo para el aviso informativo de §4.2, y nunca la escribe.

### 4.10 «Arrancar lo que necesita» (fase 3, por qué los servicios pueden ir apagados)

`project_needs(dir: &Path) -> Needs { services: Vec<String>, reasons: Vec<String> }` lee el `.env` del proyecto **sin escribir nunca**:

- **Base de datos:** `DB_CONNECTION` ∈ {mysql, mariadb} → mysql; `pgsql` → postgresql. Solo si `DB_HOST` está vacío o es `127.0.0.1`/`localhost`.
- **Redis:** alguno de `CACHE_STORE`, `QUEUE_CONNECTION`, `SESSION_DRIVER` o `BROADCAST_CONNECTION` vale `redis`, con `REDIS_HOST` local. **`REDIS_HOST` por sí solo no cuenta**, porque el skeleton lo trae siempre.
- **Mailpit:** `MAIL_MAILER=smtp`, `MAIL_HOST` local y `MAIL_PORT=1025`.

`start_project_services(dir)` aplica `Operation::TurnOn` (`brew services run`, que no registra arranque automático) a lo que falte. La consumen la app, el tray y el MCP (03, que añadirá la herramienta). Si no hay `.env` legible, devuelve `Needs` vacío con el motivo.

### 4.11 Interfaz

Una hoja a ventana completa con cuatro pasos y un indicador de puntos:

1. **Bienvenida y detección.** La lista se rellena según llegan los resultados («Homebrew ✓», «Herd en marcha · 12 sitios .test», «PHP 8.4.27 propio · PHP 8.4 de Herd», «PostgreSQL 18 en marcha»). Si Herd está instalado, una banda arriba avisa de que Elephant convive con él. Debajo, «Empezar» y «No cambiar nada».
2. **Escenario.**
   - Tres tarjetas grandes con icono, la frase, «≈ N MB en reposo» y la etiqueta «Recomendado» en Laravel. En Laravel con servicios, el control segmentado MySQL · PostgreSQL va dentro de la tarjeta, con el coste junto a cada opción.
   - Debajo, el enlace «Personalizado»: una tabla con un control de tres estados por servicio (*No tocar · Instalado y apagado · Al iniciar sesión*), las ramas de PHP con su casilla, el radio de la versión por defecto, interruptores para Composer, selector y Elephant al iniciar sesión, y el total de RAM y descarga en directo. **Se abre con el estado actual del sistema** (`Installed` o `Autostart` según lo detectado), así que no tocar nada da un plan vacío.
3. **Revisión.**
   - Grupos: *Se descargará* (con MB), *Se instalará con Homebrew*, *Arrancará al iniciar sesión*, *Ya lo tienes* (en gris, con ✓), *Cambios opcionales* (casillas desmarcadas; si `restarts_service`, avisa «se reiniciará un momento»), *No se hará* (conflictos) y los avisos.
   - Un enlace «Ver comandos» muestra el texto de `command`.
   - El botón «Aplicar (N pasos)». Si el plan está vacío: «Tu Mac ya está como pide *Laravel*» y el botón «Hecho».
4. **Progreso y resultado.**
   - Una fila por paso con su estado animado, barra en las descargas y la última línea de brew en texto secundario; cada fila se puede desplegar para ver su log.
   - Al final: «Hecho», «Ha fallado» con el mensaje traducido y «Saltado» con el motivo, más «Reintentar lo que falta» (un nuevo `setup_preview`) y «Ver log completo».

Todo el texto está en español y habla de consecuencias, no de comandos. La estética sale de la decisión de rediseño. Esta decisión solo fija la estructura y el contenido.

## 5. Fases

1. **Núcleo:** `catalog`, `download`, `detect`, `presets` y `plan` con sus tests, los cambios en `SPECS`, `paths` y `origin_of`, el `brew` streaming con variables de entorno y el sidecar del selector. Sin interfaz: un test `#[ignore]` imprime el plan real de este Mac.
2. **Asistente:** comandos `setup_*`, `apply`, las pantallas, `setup.json`, el PATH y el arranque al iniciar sesión.
3. **Bajo demanda:** `project_needs` y `start_project_services`, conectados a la app, al tray y al MCP.
4. **Sitios:** pasos de nginx y dnsmasq con `since: 2`, cuando exista la decisión de sitios.

## 6. Criterios de aceptación

Tests unitarios en `plan.rs`, con `Snapshot` y `Catalog` sintéticos como en `components::merge`:

- [ ] **Idempotencia:** para cada preset y para varios snapshots (vacío, este Mac, todo instalado), `plan(simulate(s, plan(s,p)), p, c).steps` queda vacío. `simulate` es un helper de test que aplica los efectos de cada `Action` al `Snapshot`.
- [ ] Ningún `Step` es `BrewInstall { formula: "php" | "composer" }` en ningún preset ni en un Custom válido.
- [ ] `postgresql@17` instalado satisface `postgresql@18`, y `mariadb` satisface `mysql`. En ambos casos, el `Service` actúa sobre el instalado.
- [ ] Un servicio `registered` con `want == Installed` solo genera un paso `optional`, nunca uno obligatorio. Ningún paso obligatorio tiene `op` igual a `TurnOff` o `DisableAutostart`.
- [ ] Un servicio que no está en `SPECS` (rabbitmq) no aparece en ningún paso.
- [ ] Con el puerto ocupado y el servicio de brew parado, un `want == Autostart` produce un `Conflict` y no un `Service`.
- [ ] Con un global existente y resoluble, no hay `SetGlobalPhp` obligatorio.
- [ ] `Catalog { offline: true }` convierte las descargas en `Conflict` y deja el resto del plan intacto.
- [ ] `resolve(Custom)` rechaza ids que no estén en `SPECS`, ids `installable: false`, `kind == Tool` y ramas que no existan en el catálogo.
- [ ] `checksum_url` usa `SHA256SUMS` cuando falta `<asset>.sha256` (caso real de php-8.4.27).

Tests de `download.rs` y `apply.rs`, con un `curl` y un `brew` falsos en un directorio temporal:

- [ ] Un tar con `../x` o `/etc/x` aborta el paso sin escribir nada fuera de `.tmp-<v>`.
- [ ] Un checksum que no coincide deja `~/.elephant-stack/php` sin cambios.
- [ ] Si falla `BrewInstall(mailpit)`, su `Service` pasa a `Skipped` con motivo y `DownloadPhp` termina en `Ok`.
- [ ] `setup_apply` con un `plan_id` obsoleto no ejecuta nada (el `brew` falso no registra llamadas) y devuelve el plan nuevo.
- [ ] `accepted` con una clave que no es opcional se ignora y queda registrado en el log.
- [ ] `install_formula` pasa `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1` y `HOMEBREW_NO_AUTO_UPDATE=1`.

Detección y shell:

- [ ] `PathState` es `Active` con la línea `~/.localdeck/bin` de este Mac (enlace simbólico) y `ShadowedByHerd` si la línea va antes de la de Herd.
- [ ] `setup_shell_apply` deja la copia `.elephant-bak-*` y un `~/.zshrc` idéntico más el bloque. Aplicarlo dos veces no duplica el bloque.
- [ ] `detect()` en este Mac tarda menos de 1,5 s (`cargo test system_tests -- --ignored`).

Manuales:

- [ ] En este Mac, «Laravel» produce un plan con:
  - pasos: Composer y Elephant al iniciar sesión;
  - en «Ya lo tienes»: PHP 8.4.27, Mailpit y el selector en el PATH;
  - los avisos de Herd y de Composer de Herd;
  - ninguna descarga de PHP;
  - ningún paso sobre PostgreSQL, que el escenario no menciona.
- [ ] En este Mac, «Laravel con servicios» preselecciona PostgreSQL y añade a lo anterior: `brew install redis`, «Mailpit al iniciar sesión» y el opcional desmarcado «Quitar el arranque automático de PostgreSQL 18 (sigue encendido; se reiniciará un momento)». No toca MySQL.
- [ ] «No cambiar nada» no ejecuta ningún comando y no vuelve a mostrar el asistente.
- [ ] Tras aplicar, Ajustes ▸ Escenario muestra «Al día».

## 7. Riesgos

- **Bajo demanda no es automático.** Hasta la fase 3, quien elige Laravel con servicios y abre un sitio sin encender la base de datos verá `Connection refused`. Mitigación: la tarjeta lo dice («se encienden cuando las necesitas, desde el menú del elefante») y la fase 3 va justo después. El apagado por inactividad queda fuera.
- **El asistente no sirve sitios todavía.** Depende de Herd hasta la fase 4. Se dice de forma explícita en la revisión.
- **Autenticidad de las descargas.** El checksum viene del mismo origen que el binario: protege contra la corrupción, no contra una release comprometida. Para php-builds, el repo es del propio usuario. Para Composer, más adelante se puede verificar `composer.phar.asc` con la clave pública de Composer incluida en la app.
- **php-builds cambia de formato.** Hoy conviven `<asset>.sha256` y `SHA256SUMS`, y la release de 8.4.27 se publicó distinto que las demás. El catálogo ignora las releases sin asset macOS y sin checksum, y lo deja escrito en el log.
- **Cifras de RAM.** Son estimaciones (MySQL depende de `innodb_buffer_pool_size`; en Postgres, el RSS cuenta varias veces la memoria compartida). Siempre llevan «≈», y la medida real sustituye a la estimación cuando el servicio está en marcha.
- **`brew install` arrastra dependencias.** Con la variable de entorno no se tocan los dependientes, pero las dependencias directas de la fórmula sí pueden actualizarse. La revisión lo advierte: «Homebrew puede actualizar librerías que esta fórmula necesita».
- **Heurística del `.env`.** Falla con Sail, con hosts remotos, con varios `.env` y con valores que vienen de `config/*.php`. Por eso devuelve sus motivos y nunca arranca nada sin que el usuario lo pida.
- **Puertos de otros usuarios del Mac.** `connect()` detecta el puerto ocupado, pero `ps` solo adivina el dueño por el nombre. El mensaje dice «parece» cuando no hay coincidencia segura.
- **`tauri-plugin-autostart`.** Usa un LaunchAgent, no `SMAppService`, así que en Ajustes del Sistema ▸ Ítems de inicio puede aparecer con un nombre poco claro. Hay que comprobar el nombre del plist y la opción `--hidden` con la versión exacta del plugin antes de fijarla.
- **Ajustes de `php.ini` y Xdebug.** Siguen sin poder configurarse hasta que el selector exporte `PHP_INI_SCAN_DIR`. Es un cambio aparte, en la sección PHP, y hay que coordinarlo con php-fpm cuando llegue nginx.
- **Archivos internos de Herd.** `valet/config.json` puede cambiar entre versiones. Solo se usa para un texto informativo, y si falla, el aviso sale sin el número de sitios.
