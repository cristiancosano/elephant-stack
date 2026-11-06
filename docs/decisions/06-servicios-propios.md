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
