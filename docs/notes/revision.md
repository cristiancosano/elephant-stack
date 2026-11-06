# Revisión · Correcciones tras la verificación independiente

Se trabajó en `main`. Cada problema tiene su test salvo donde se indica.

## Qué se ha cambiado

**Motor de logs (`elephant-logs`)**

- `query.rs`: un archivo que agota `HARD_BUDGET` sin ninguna cabecera se da por leído (`Pull::End`) en vez de poner un suelo `(i64::MAX, …, u64::MAX)` que vaciaba la página de todas las fuentes y dejaba un cursor que no resolvía. Lo que haya más atrás de esos 64 MB sin cabeceras no se ve.
- `reader.rs`: `BackReader` entrega como mucho `MAX_RECORD` (1 MB) por entrada y suelta lo que sobra del resto sin cabecera, así que la memoria ya no crece con el archivo.
- `sources.rs`: un archivo de una fuente solo se lista si, resuelto (`canonicalize`), queda dentro de la raíz del proyecto (`storage/logs`) o de la propia carpeta (globs propios y fuentes globales). **Efecto:** un `storage/logs` que es un enlace a una carpeta fuera del proyecto (despliegues tipo Envoyer) ya no se lee.
- `ops.rs` (`clear`): vacía la ruta ya resuelta, comprueba que su clave sigue siendo la del catálogo y abre con `O_NOFOLLOW`.

**Canal de agentes (`elephant-api`, `elephant-mcp`, `elephant-core`)**

- Tareas: `elephant __job` toma un cerrojo (`flock`) sobre `<id>.log` mientras corre (`jobs::hold`). `jobs::read` y `jobs::list_active` marcan como fallida una tarea activa cuyo cerrojo está libre (en cola, tras 60 s de gracia). `start_job` recoge al hijo en un hilo (sin zombis en `elephant mcp`) y, al terminar, lo vuelve a mirar.
- `fit.rs`: `Op::OLDEST_FIRST` (por defecto `false`, `true` en `logs_tail`) y `OpMeta::oldest_first`. En `logs tail`, el recorte a 24 KB quita las entradas más antiguas y nunca sugiere `--cursor`. El texto conserva el título y avisa antes del bloque no fiable.
- `text::log_block`: cada línea física de un elemento va en su propia línea, sangrada si no es la primera, y las marcas `--- datos de log … ---` y `--- fin ---` que traiga el contenido se neutralizan (guiones U+2010).
- `text::for_dialog` / `dialog_path`: rutas, nombre del cliente MCP y el texto final del diálogo sin caracteres de control ni de dirección, y acotados.
- MCP: el `outputSchema` de las operaciones largas es `anyOf` [su resultado, `{job_id, state: "running"}`], con `$defs` en la raíz. «Probar conexión» (cliente `elephant-stack`, `elephant_mcp::PROBE_CLIENT`) no se anota en la auditoría.
- `job show` con un id mal formado sale con 2: `jobs::check_id` pasa a ser público y lo usa `validate`.
- `setup/download.rs`: curl con `--connect-timeout 20 --speed-limit 1024 --speed-time 60 --max-time 1800`, y la descarga se cancela si el archivo no crece en 90 s.

**App (`src-tauri`)**

- `LogsState` guarda `Arc<LogHub>`: el cerrojo solo cubre el `Option` y `release` (hilo principal) no espera a una consulta en marcha.
- `nav:go`: `AppState` sabe si la interfaz ya escucha (lo marca `take_pending_route`, lo quita `Destroyed`). Con la ventana creada pero cargando, la ruta queda pendiente en vez de perderse.
- Test que comprueba que cada comando de `generate_handler!` sale en la tabla de 00 §5.6.

**Interfaz**

- Logs: una página vacía con `next_before` sigue pidiendo historial sola (hasta `MAX_EMPTY_PAGES` = 4 páginas vacías seguidas) y la vista muestra «Buscando en el historial…» o «Seguir buscando» en vez de «Nada con estos filtros». `VirtualList` pide más historial cuando el scroll no puede salir de la zona del principio (lista corta o sin barra). `load` reinicia `loadingOlder`, que una carga anterior podía dejar a `true`.
- Accesibilidad: el pie de Logs solo anuncia el estado del directo; la barra lateral solo anuncia cuántos servicios hay en marcha (no la memoria); el grid de logs expone la fila seleccionada con `aria-activedescendant` y `aria-selected` en la fila; el contenedor de toasts es la región viva permanente.
- Contraste en claro: `--lvl-*` y `--http-*` con variantes oscuras (≥ 4,5:1 sobre blanco y sobre su tinte del 14 %); en oscuro siguen los colores del sistema. La clase de la excepción de la fila pasa a `--label-2`.
- Ajustes ▸ Agentes explica que un agente que ejecuta órdenes en su propia terminal sin `CLAUDECODE` ni `AI_AGENT` pasa por persona, y recomienda MCP o Solo lectura.

**Documentación**

- 00 §5.6: añadidos `project_start_services`, `clear_problems` y `logs_roots`; en §5.5, `LogsState` con `Arc<LogHub>`.

## Para integración

- PTY sin variables de agente (05 §5.2 y §5.5): solo se documenta, como ya declara 05. Si se quiere endurecer, habría que decidir en 05 qué más variables cuentan como agente (por ejemplo, las que definen Gemini CLI o Codex) o pedir confirmación también a las personas fuera de una sesión conocida; ambas cosas cambian el contrato de 05 §5.2.
- `scripts/budget.sh`: el JS inicial ya pasaba del presupuesto antes de estos cambios (40,1 KB sobre 40 KB) y sigue igual.
- `cargo fmt --check` no pasa en el repositorio desde antes (muchos archivos de `elephant-api`, `elephant-cli`, `src-tauri`…). Aquí solo se formatearon los archivos que ya estaban limpios.

## No hecho

- No se ha probado con VoiceOver ni con el inspector de WebKit en la app real (no se abre la app del usuario): el contraste se comprueba con un test que calcula la luminancia relativa.
- La reproducción con `kill -9` de un `__job` lanzado por `elephant mcp` está automatizada en `crates/elephant-cli/tests/cli.rs` con un HOME temporal; no se ha hecho contra el `~/.elephant-stack` real.
