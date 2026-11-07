# 06 · Servicios propios, sin Homebrew

Estado: **propuesta**, pendiente de revisión.

## Objetivo

Elephant instala y ejecuta MySQL, PostgreSQL, Redis, Mailpit, Composer y Node
sin Homebrew, nvm ni otro gestor de paquetes. Funciona como un instalador
automatizado:

1. lee la lista de versiones que publica cada proyecto;
2. el usuario elige una;
3. Elephant descarga el binario, comprueba el checksum y lo instala en su carpeta.

Puede haber varias versiones del mismo servicio a la vez. Los datos de cada
servicio son de la cuenta que lo usa, así que no puede repetirse el caso de
`/opt/homebrew/var/mysql` con otro dueño.

**Fuera del alcance:**

- **Convivir con Homebrew.** Elephant deja de leer y de manejar `brew services`.
  Lo que ya esté instalado con brew se queda como está y Elephant no lo toca.
- **Empaquetar binarios dentro de la app.**
- **Linux y Windows en esta fase.** Las fuentes elegidas sí los cubren (salvo lo
  que se indica en la tabla), así que el diseño no se cierra a ellos.

## Fuentes (comprobadas el 2026-11-06)

| Componente | Lista de versiones | Binarios | Checksum |
|---|---|---|---|
| MySQL | Página de descargas y archivo de dev.mysql.com (HTML, sin API) | Oficiales: `cdn.mysql.com`, `mysql-<v>-macos15-arm64.tar.gz`; Linux glibc x86_64/aarch64; `winx64.zip` | MD5 en la página |
| PostgreSQL | `postgresql.org/versions.json` (oficial) | `theseus-rs/postgresql-binaries`, releases de GitHub para las tres plataformas (no son oficiales) | sha256 del asset en la release de GitHub |
| Redis | Releases de `redis/redis` | No hay binarios: los compila nuestro CI | sha256 propio |
| Mailpit | Releases de GitHub (oficial) | Oficiales | sha256 del asset en la release de GitHub |
| Composer | `getcomposer.org/versions` (oficial) | `composer.phar` oficial | `.sha256sum` oficial |
| Node | `nodejs.org/dist/index.json` (oficial, marca las LTS) | Oficiales: `darwin-arm64`, `linux-x64`, `win-x64` | `SHASUMS256.txt` oficial |

MariaDB tiene una API oficial muy buena, pero no publica binarios para macOS, así
que se descarta.

### Índice de versiones generado en CI

La página de MySQL no tiene API, y leerla desde la app la haría frágil. Un job
diario, el mismo que ya publica `index.json` en php-builds, lee las fuentes de la
tabla y publica `services.json` en la release `catalog`. Ese archivo solo
contiene la versión, la URL **oficial**, el checksum y el tamaño de cada binario.

**Ventajas:**

- No redistribuimos ningún binario de terceros; la excepción es Redis, que
  compilamos nosotros.
- Si cambia una web, se arregla el job y no hace falta sacar otra versión de la
  app.
- La app solo hace una petición, que no consume el cupo de la API de GitHub.

**Respaldo:** si el índice no responde, la app lee directamente las fuentes que
tienen API (todas menos MySQL).

## Disposición en disco

```
~/.elephant-stack/
  services/<servicio>/<versión>/   binarios (mysql/8.4.7, postgresql/18.6.1…)
  data/<servicio>/<mayor>/          datos (mysql/8.4, postgresql/18…)
  node/<versión>/                   versiones de Node
  tools/composer/<versión>/         composer.phar (una sola versión)
  bin/                              selectores php, node, npm, npx, composer
  launchd/                          plists que genera Elephant (copia de referencia)
```

**Por qué los datos van por versión mayor:** el formato de los datos cambia
entre versiones mayores. Así, un parche (8.4.7 → 8.4.8) usa los mismos datos.
Pasar de una mayor a otra es una acción explícita, «Migrar datos», que queda
para una fase posterior.

## Ejecución

- **Agentes de launchd propios:**
  - Cada versión mayor de cada servicio tiene su propio agente, con la etiqueta
    `sh.elephant.<servicio>.<mayor>`, en `~/Library/LaunchAgents`.
  - **Encender:** `launchctl bootstrap`. **Apagar:** `bootout`.
  - **Arranque al iniciar sesión:** `RunAtLoad`.
  - **Arranque bajo demanda:** se mantiene como ahora.
- **Primer arranque:** Elephant prepara la carpeta de datos y comprueba que puede
  escribir en ella.
  - MySQL: `mysqld --initialize-insecure`, con el usuario `root` sin contraseña,
    igual que en Herd y en brew.
  - PostgreSQL: `initdb -U postgres --auth=trust`.
- **Puertos:** un puerto por servicio, por defecto el estándar. Solo puede estar
  encendida una versión de cada servicio a la vez, que es la regla más sencilla;
  al encender otra, Elephant apaga la anterior y lo avisa.
- **Comprobar que de verdad arranca:**
  - Tras encender un servicio, Elephant comprueba durante unos segundos que el
    proceso sigue vivo y que el puerto responde.
  - Si falla, muestra la última línea útil del log de error y pone el servicio
    en «error», en vez de dejarlo «parado» en silencio.
  - Esto también arregla el bucle de reintentos que hemos visto en el MySQL de
    brew.

## Node y Composer

**Node: gestor propio en vez de nvm.**

- **Por qué no nvm:**
  - es una función de bash que hay que cargar en cada shell, y eso ralentiza la
    apertura de la terminal;
  - la app y launchd no pueden usarla;
  - no existe en Windows (nvm-windows es otro proyecto).
- **Cómo lo hace Elephant:** lee `nodejs.org/dist/index.json` e instala en
  `~/.elephant-stack/node/<versión>`.
- **Selectores `node`, `npm` y `npx`:** son el mismo binario que el selector de
  PHP, generalizado. Eligen la versión según el archivo del proyecto:
  `.nvmrc`, o `.node-version` si no lo hay.
- **Compatibilidad con nvm:** los proyectos que ya usan `.nvmrc` siguen
  funcionando, y Elephant no toca `~/.nvm`.

**Composer:** el asistente ya lo descarga con su checksum. Falta:

- ponerlo en la vista de componentes, para que se pueda instalar y actualizar
  sin pasar por el asistente;
- dejar un selector `composer` en `bin/` que lo ejecute con el PHP del proyecto.

## Datos que ya existen

Si Elephant encuentra datos de brew, ofrece **copiarlos** y nunca los mueve:

- **MySQL:** `/opt/homebrew/var/mysql`.
- **PostgreSQL:** `/opt/homebrew/var/postgresql@<n>`.

La copia solo se ofrece si la versión mayor coincide y el servicio de origen está
parado. El original no se toca. En este Mac, los 429 MB de MySQL se pueden leer
desde la cuenta `cristian` gracias al grupo `admin`, sin `sudo`.

## Fases

1. **Base y MySQL.**
   - En `elephant-core`, una interfaz `Catalog` por componente (`releases()`, que
     devuelve la versión, la URL, el checksum y el tamaño) y un instalador común,
     que sale de generalizar `install_php`: descarga, comprobación,
     descompresión y una tarea con su progreso.
   - El supervisor de launchd propio y la comprobación de arranque.
   - El job `services.json` en php-builds.
   - La vista de Servicios con lista única (instaladas y disponibles), igual que
     la de PHP.
   - La importación de los datos de MySQL.
2. **PostgreSQL, Redis y Mailpit.**
   - El job que compila Redis en php-builds.
   - La importación de los datos de PostgreSQL.
3. **Node y Composer.**
   - Selector generalizado y vista de herramientas.
4. **Quitar Homebrew.**
   - Borrar `brew.rs` y los usos de `launchd::homebrew_jobs`.
   - Adaptar el asistente, el panel, el menú de la barra, la CLI y el MCP, y los
     textos en los dos idiomas.
   - Actualizar el README.

Cada fase deja la app funcionando, con sus tests y capturas en los dos idiomas.

### Fase 1: cómo ha quedado

- **Código:** `elephant-core::services` (`catalog`, `install`, `mysql` y el
  supervisor en `mod.rs`). Sin traits: un `enum ServiceKind` con un `match` por
  servicio. La caché del índice comparte `MemoryCache` con el de PHP.
- **Rutas:** binarios en `services/mysql/<versión>`, datos en
  `data/mysql/<rama>` (la rama de MySQL es `X.Y`: 8.4, 9.7, 26.10), socket y
  pid en `run/`, log en `logs/mysql-<rama>.err` (también la salida de launchd).
- **mysqld:** directo, con `--no-defaults` (no lee el `my.cnf` de brew ni de
  `/etc`), `--bind-address=127.0.0.1` y `--mysqlx=OFF` (el puerto 33060 y el
  socket de `/tmp` chocarían con otras versiones y con brew).
- **launchd:** `KeepAlive` solo si termina mal (`SuccessfulExit = false`).
  Encender = `bootstrap` + `kickstart`; si la comprobación de arranque falla,
  el agente se descarga (si no, launchd lo reintentaría cada 10 s) y el motivo
  queda en `run/mysql-<rama>.error` hasta el siguiente intento o apagado.
- **Importar de brew:** copia sin `*.pem` (mysqld los regenera; en este Mac
  las claves de brew son `-rw-------` de otra cuenta). Si la rama ya tenía
  datos, se apartan como `<rama>.pre-import-<fecha>`. Una marca
  `data/mysql/<rama>.imported` evita volver a ofrecerla.
- **Superficies:** comandos `service_*` (00 §5.6), `Snapshot.managed`, la vista
  Servicios y las órdenes `elephant stack list|control|install`.

### Fase 2: cómo ha quedado

- **Código:** `services::{postgresql, redis, mailpit, ports, tools}`. El
  instalador admite paquetes sin carpeta raíz (Mailpit) y enlaces duros
  (PostgreSQL). Ramas: PostgreSQL y Mailpit por mayor (`18`, `1`), Redis
  por `X.Y` (lo que publica php-builds).
- **PostgreSQL:** `initdb -U postgres --auth=trust -E UTF8
  --locale=en_US.UTF-8` y `postgres -D -p -k run/postgresql-<rama>`, con
  `listen_addresses=127.0.0.1`. La carpeta de datos queda en 0700 (se
  corrige al arrancar si no). Apagar usa antes `pg_ctl stop -m fast`: el
  SIGTERM de launchd es un apagado «smart» que espera a los clientes.
- **Importar de brew:** la mayor más reciente de `var/postgresql@N`, sin
  `postmaster.pid`, en 0700. Si algún archivo no se puede leer (otra
  cuenta), no hay botón: se explica el volcado con `pg_dumpall` (de brew) y
  `psql` (de Elephant). Ojo: el superusuario de unos datos copiados de brew
  es la cuenta que los creó, no `postgres`.
- **Redis:** `config/redis-<rama>.conf` generado en cada arranque.
- **Mailpit:** `--smtp 127.0.0.1:1025 --listen 127.0.0.1:8025 --database
  data/mailpit/mailpit.db --disable-version-check`; los correos no dependen
  de la versión. `mailpit::info` cuenta con el propio y con el de brew.
- **Puerto ocupado:** `lsof` da el proceso; si es un servicio de brew, el
  error dice «detenlo con `brew services stop <fórmula>`»; si es de
  Elephant, lo nombra; si no, da el proceso y su pid.
- **phpMyAdmin:** zip oficial en `tools/phpmyadmin/<v>` con
  `config.inc.php` (root sin contraseña en 127.0.0.1:3306, sin servidores
  arbitrarios, `TempDir` propio, sin comprobar versiones). Lo sirve
  `php -S 127.0.0.1:8081` con el PHP de Elephant más reciente que cumpla
  `php` del índice, como agente `sh.elephant.phpmyadmin` sin `RunAtLoad`
  (se enciende al abrirlo).
- **pgAdmin:** se monta el DMG en `tmp/`, se copia la app con `ditto
  --noqtn` a `~/Applications/.pgAdmin 4.app.elephant-tmp`, se le quitan
  `com.apple.FinderInfo` y la cuarentena (el DMG oficial trae un FinderInfo
  que hace fallar `codesign --strict`; no forman parte de la firma), se
  comprueba `codesign --verify --deep --strict`, el Team ID `TCHGL2R7C5` y
  `spctl`, y solo entonces se coloca. La marca `tools/pgadmin/installed.json`
  permite actualizar y desinstalar; un pgAdmin que no instaló Elephant no se
  toca.
- **Superficies:** comandos `tool_*`, `Snapshot.managed_tools`, la sección
  Herramientas de la vista Servicios, interruptores de las ramas propias en
  el Panel y en el menú de la barra, y `elephant stack tool`.

### Fase 3: cómo ha quedado

- **Un solo binario para todos los selectores.** `bin/php` es el binario
  del crate `elephant-php` (el sidecar `php-selector`, como antes) y
  `node`, `npm`, `npx`, `corepack`, `composer` y los paquetes globales de
  npm son enlaces simbólicos relativos a él (`node -> php`). Decide por
  `argv[0]`: `php` (como siempre), `composer` (el phar con el PHP que
  resuelve el selector de PHP para la carpeta, así que respeta
  `.php-version`) y cualquier otro nombre es un ejecutable del `bin/` de
  la versión de Node elegida. Actualizar `bin/php` actualiza todos. Antes
  de crear enlaces, Elephant deja en `bin/php` el binario de la app si
  falta o es otro (`selectors::ensure_shim`): un `php` antiguo con un
  enlace `node` ejecutaría PHP. Los mensajes del selector salen en los dos
  idiomas (ahora depende de `elephant-i18n`; pasa de 402 a 506 KB).
- **Elegir Node** (`elephant_php::node`, compartido con la app): en cada
  carpeta, subiendo hasta la raíz, `.nvmrc` y después `.node-version`;
  si no hay, `global-node`; si tampoco, la más reciente. Formatos: `20`,
  `v20.11.1`, `20.11`, `20.x`, `lts/*`, `lts/iron`, `node` (también
  `latest`, `current`, `stable`) y comentarios `#`. `system`, rangos
  (`>=18`) o `lts/-1` dan un error que dice qué formatos valen. Lo pedido
  que no está instalado es un error con la orden para instalarlo, nunca
  otra versión en silencio. La LTS de cada versión se apunta al instalar
  en `node/<v>/.elephant-lts` (de `index.json`); sin esa marca, una
  tabla por mayor.
- **PATH del hijo:** el `bin/` de la versión va delante y se quitan las
  otras carpetas de `node/`, así que `npm run`, los `#!/usr/bin/env node`
  y los scripts usan la misma versión sin pasar otra vez por el selector.
- **Paquetes globales de npm:** se instalan en el `bin/` de la versión
  (prefijo por defecto de npm) y Elephant crea un selector en
  `~/.elephant-stack/bin` por cada ejecutable de cualquier versión
  instalada (`reshim`). Se regeneran al instalar, actualizar y
  desinstalar, y el propio selector lo hace tras `npm -g …` y `corepack
  enable|disable|install` (en esos casos espera al hijo en vez de hacer
  `exec`). `elephant node reshim` lo hace a mano. Un selector de un
  paquete que no tiene la versión de esa carpeta da un error que dice qué
  versiones lo tienen. Nunca se pisa un archivo de `bin/` que no sea un
  enlace de Elephant. Se descartó un prefijo global común (como hace
  volta): los módulos nativos se compilan para una versión concreta.
- **Catálogo de Node:** `nodejs.org/dist/index.json` directo (no pasa por
  php-builds), con las versiones estables `vX.Y.Z` que tienen
  `osx-arm64-tar`; una hora en memoria y `cache/node-index.json`. Se
  destacan la más reciente de cada LTS con soporte (fin aproximado: abril
  del tercer año tras su primera versión, con un mes de margen) y la
  Current (la mayor más reciente si aún no es LTS); hoy, 26 (Lithium), 24
  y 22. El resto, con «Mostrar N versiones más» o `node list --all`.
- **Instalar Node:** `node-v<v>-darwin-arm64.tar.gz`, `SHASUMS256.txt`,
  el tamaño con un `HEAD` (para el progreso) y el instalador común de los
  servicios generalizado (`services::install::unpack`: sha256, entradas
  dentro de una carpeta raíz, `bin/node --version` y `rename`).
  Actualizar = instalar la más reciente de la mayor y `node::replace_in`
  (global y `.nvmrc`/`.node-version` que fijaban la exacta, conservando la
  `v`), como `php::replace_own`. Desinstalar vuelve a «la más reciente»
  si la global fijaba esa versión exacta.
- **La app elige la mayor:** «Por defecto» y «Usar en proyecto…»
  escriben `24`, que sigue valiendo tras actualizar, salvo que haya otra
  24 más reciente instalada; entonces, la exacta. El proyecto recibe
  `.nvmrc`, o `.node-version` si ya usaba ese.
- **Composer:** `tools/composer/<v>/composer.phar` con su `.sha256sum`,
  una versión a la vez (al instalar otra se borran las demás). Canales de
  `getcomposer.org/versions`: `stable` y `2.2` (LTS, para PHP < 7.2.5);
  actualizar sigue en el canal instalado. El asistente usa lo mismo; el
  `libexec/composer.phar` y el envoltorio `#!/bin/sh` de antes se migran
  al instalar (el selector también sabe ejecutar el phar antiguo).
  `component install composer` llama a lo mismo.
- **Dónde está en la app:** vista Node propia (barra lateral, ⌘5; Logs
  pasa a ⌘6) y Composer como sección de la vista PHP, no en Servicios ▸
  Herramientas: no es un servicio ni se abre, y lo que importa de él es con
  qué PHP se ejecuta.
- **PATH:** la app no ejecuta la shell del usuario (04 §4.2); lee
  `~/.zprofile` y `~/.zshrc` y avisa si nvm (`nvm.sh`) o Herd se cargan
  después de la línea de Elephant (`detect::path_shadow`), con la línea
  para copiar. `elephant doctor` mira el `PATH` real de la terminal y dice,
  para `php`, `composer`, `node`, `npm` y `npx`, si son los de Elephant y
  qué línea mover. Elephant nunca edita `~/.zshrc` por esto. Con nvm
  instalado, la vista Node explica que Elephant no lo usa ni lo toca. No
  se copian versiones de `~/.nvm`: sin su checksum no se pueden
  comprobar, y reinstalar desde nodejs.org es un clic.
- **Superficies:** `node_*` y `composer_*` (00 §5.6), `Snapshot.node`
  (con la versión de cada proyecto conocido) y `Snapshot.composer`;
  órdenes `elephant node list|install|use|isolate|reshim` y `elephant
  composer list|install` (sin desinstalar, como manda 00 §5.9).
- **Pruebas reales** (`#[ignore]`, HOME temporal): `node::tests::
  real_node_in_a_temporary_home` (LTS de nodejs.org, `node -v` con
  `.nvmrc`, `npm -v`, `npx -v`, `npm i -g semver` y `semver` por su
  selector, y el error de una versión no instalada) y `composer::
  system_tests::real_composer_runs_with_the_project_php` (Composer
  estable con el PHP 8.4.27 de Elephant enlazado en solo lectura y un PHP
  7.4 falso para comprobar `.php-version`).

## Riesgos

- **Página de MySQL:** si cambia, el job falla y se avisa en GitHub. La app sigue
  con el último índice bueno.
- **Nombre del archivo de MySQL:** incluye la versión de macOS (`macos15`). El job
  toma el más reciente que haya para arm64.
- **theseus-rs:** no es oficial. Se verifica el sha256 de su release, pero
  dependemos de que ese proyecto siga publicando.
- **Gatekeeper:** lo que descarga `curl` no lleva el atributo de cuarentena, así
  que macOS no bloquea los binarios. Ya funciona así con PHP.

## Decisiones abiertas

- **Dónde publicar el índice:** la propuesta es php-builds (su release `catalog`
  ya existe). Quizá convenga renombrar el repo a `elephant-builds`, ya que no
  tendría solo PHP.
- **Redis:** compilarlo nosotros o cambiarlo por Valkey. Valkey tampoco publica
  binarios para macOS, así que no ahorra trabajo.
