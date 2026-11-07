// Copia en la interfaz del último `Snapshot` de Rust, que es la única fuente de
// verdad (01 §4.3). Solo App.svelte la conecta; las vistas reciben el snapshot por
// props y usan las funciones puras de abajo para derivar lo que pintan.
//
// Svelte 5 no permite exportar `$derived` de módulo, así que aquí se exportan
// funciones que reciben el snapshot: dentro de un componente se usan con `$derived`.

import { errorMessage, getSnapshot } from "../ipc";
import { onStateChanged, type UnlistenFn } from "../events";
import type { BranchStatus } from "../bindings/BranchStatus";
import type { Busy } from "../bindings/Busy";
import type { Component } from "../bindings/Component";
import type { JobInfo } from "../bindings/JobInfo";
import type { ManagedService } from "../bindings/ManagedService";
import type { Problem } from "../bindings/Problem";
import type { Project } from "../bindings/Project";
import type { Service } from "../bindings/Service";
import type { Snapshot } from "../bindings/Snapshot";
import type { Usage } from "../bindings/Usage";
import type { VersionInfo } from "../bindings/VersionInfo";
import { formatCpu, formatMemory, shouldShowCpu } from "../format";
import { t } from "../i18n/index.svelte";

// ── Estado ──────────────────────────────────────────────────────────────────

// `raw`: el snapshot se sustituye entero y nunca se muta, así que no hace falta
// convertirlo en un proxy profundo.
let current = $state.raw<Snapshot | null>(null);
let lastError = $state<string | null>(null);

export const app = {
  get snapshot(): Snapshot | null {
    return current;
  },
  /** Error al pedir el primer snapshot; se borra con el siguiente que llegue. */
  get error(): string | null {
    return lastError;
  },
};

/** `next` sustituye a `prev` solo si es más reciente (las revisiones solo crecen). */
export function isNewer(prev: Snapshot | null, next: Snapshot): boolean {
  return prev === null || next.revision > prev.revision;
}

/** Aplica un snapshot si es más reciente que el actual. Devuelve si lo ha aplicado. */
export function applySnapshot(next: Snapshot): boolean {
  if (!isNewer(current, next)) return false;
  current = next;
  lastError = null;
  return true;
}

async function fetchSnapshot(): Promise<void> {
  try {
    applySnapshot(await getSnapshot());
  } catch (e) {
    if (current === null) lastError = errorMessage(e);
  }
}

/**
 * Se suscribe a `state:changed` y pide el snapshot cacheado. Primero escucha, para
 * no perder un cambio emitido mientras llega la respuesta. Al recuperar el foco se
 * vuelve a pedir: `get_snapshot` lanza un refresco si el que tiene es de hace más
 * de 2 s (las versiones de PHP, por ejemplo, cambian fuera de la app).
 */
export async function connect(): Promise<UnlistenFn> {
  const stopListening = await onStateChanged((s) => applySnapshot(s));
  await fetchSnapshot();
  const onFocus = () => void fetchSnapshot();
  window.addEventListener("focus", onFocus);
  return () => {
    stopListening();
    window.removeEventListener("focus", onFocus);
  };
}

/** Solo para tests. */
export function resetApp(): void {
  current = null;
  lastError = null;
}

// ── Componentes y servicios ─────────────────────────────────────────────────

/** Servicios instalados, los que están en marcha primero (orden estable). */
export function services(s: Snapshot): Component[] {
  return s.components
    .filter((c) => c.installed && c.kind === "service")
    .sort((a, b) => Number(b.service?.running ?? false) - Number(a.service?.running ?? false));
}

export function runningServices(s: Snapshot): Service[] {
  return s.components.flatMap((c) => (c.installed && c.service?.running ? [c.service] : []));
}

/** Ramas de servicios propios instaladas y cuántas están en marcha. */
export function managedCounts(s: Snapshot): { installed: number; running: number } {
  const branches = s.managed.flatMap((m) => m.branches);
  return { installed: branches.length, running: branches.filter((b) => b.state === "running").length };
}

/** Ramas propias que el usuario usa (en marcha o al iniciar sesión), para el Panel. */
export function quickManaged(s: Snapshot): { service: ManagedService; branch: BranchStatus }[] {
  return s.managed.flatMap((m) =>
    m.branches.filter((b) => b.state === "running" || b.autostart).map((branch) => ({ service: m, branch })),
  );
}

/** La rama de Mailpit propia que se enciende desde el Panel: la que arranca al iniciar sesión o la más reciente. */
export function ownMailpitBranch(s: Snapshot): BranchStatus | null {
  const own = s.managed.find((m) => m.service === "mailpit");
  return own?.branches.find((b) => b.autostart) ?? own?.branches[0] ?? null;
}

/** PIDs de los servicios en marcha (de brew y propios), para medir su consumo. */
export function runningPids(s: Snapshot): number[] {
  const own = s.managed.flatMap((m) => m.branches.flatMap((b) => (b.pid ? [b.pid] : [])));
  return [...runningServices(s).flatMap((service) => (service.pid ? [service.pid] : [])), ...own];
}

/** Herramientas instaladas. Con versiones detectadas, PHP tiene su propia vista. */
export function tools(s: Snapshot): Component[] {
  const hasPhp = s.php.versions.length > 0;
  return s.components.filter(
    (c) => c.installed && c.kind === "tool" && !(c.id === "php" && hasPhp),
  );
}

export function missing(s: Snapshot): Component[] {
  return s.components.filter((c) => !c.installed);
}

export function busyFor(s: Snapshot, target: string): Busy | undefined {
  return s.busy.find((b) => b.target === target);
}

/** Objetivo de `busy` de las acciones sobre PHP (actions.rs). */
export const PHP_TARGET = "php";

/**
 * Hay algo en marcha en Homebrew: arrancar, parar, instalar o reiniciar todos.
 * Homebrew admite una sola operación a la vez, así que el resto de controles esperan.
 */
export function brewBusy(s: Snapshot): boolean {
  return s.busy.some((b) => b.target !== PHP_TARGET);
}

export function problemFor(s: Snapshot, target: string): Problem | undefined {
  return s.problems.find((p) => p.target === target);
}

/** Nombre legible de un objetivo de `busy` o de un problema: el del proyecto, si lo es. */
export function targetName(s: Snapshot, target: string): string {
  for (const m of s.managed) {
    const branch = m.branches.find((b) => b.label === target);
    if (branch) return `${m.name} ${branch.branch}`;
  }
  const tool = s.managed_tools.find((tool) => tool.target === target);
  if (tool) return tool.name;
  return s.projects.find((p) => p.id === target)?.name ?? target;
}

/** Vista donde se ve un problema, como `problem_route` del menú de la barra. */
export function problemRoute(s: Snapshot, target: string): "#/php" | "#/sitios" | "#/servicios" {
  if (target === PHP_TARGET) return "#/php";
  return s.projects.some((p) => p.id === target) ? "#/sitios" : "#/servicios";
}

/**
 * Objetivos de los problemas `action_failed` que se quitan al abrir `view` (01 §9.4).
 * Los `service_error` describen el estado real y no se quitan así.
 */
export function problemsClearedBy(s: Snapshot, view: string): string[] {
  const targets = s.problems
    .filter((p) => p.kind === "action_failed" && problemRoute(s, p.target) === `#/${view}`)
    .map((p) => p.target);
  return [...new Set(targets)];
}

/** Lo que se lee bajo el nombre del servicio: «En marcha · 84 MB · 12 % CPU · Al iniciar sesión». */
export function serviceStatusText(service: Service, usage?: Usage): string {
  const parts: string[] = [];
  if (service.running) {
    parts.push(t("services.row.running"));
    if (usage) {
      parts.push(formatMemory(usage.memory_kb));
      if (shouldShowCpu(usage.cpu_percent)) parts.push(`${formatCpu(usage.cpu_percent)} CPU`);
    }
  } else if (service.status === "error") {
    parts.push(
      service.exit_code === null || service.exit_code === undefined
        ? t("services.row.exitUnknown")
        : t("services.row.exitCode", { code: String(service.exit_code) }),
    );
  } else {
    parts.push(t("services.row.stopped"));
  }
  if (service.registered) parts.push(t("services.row.atLogin"));
  return parts.join(" · ");
}

export type ServiceState = "running" | "stopped" | "error";

export function serviceState(service: Service): ServiceState {
  if (service.running) return "running";
  return service.status === "error" ? "error" : "stopped";
}

/** Versión y origen. Homebrew es el origen por defecto, así que solo se nombra cualquier otro. */
export function installedText(c: Component): string {
  const origin = c.origin === "Homebrew" ? null : c.origin;
  return [c.version, origin].filter(Boolean).join(" · ") || t("services.row.installed");
}

// ── Tareas de agentes ───────────────────────────────────────────────────────

/** Nombre legible del cliente de un agente (`audit.rs`: «claude-code 2.1.318», «mcp:cursor 1.2»…). */
export function clientName(client: string | null): string | null {
  if (!client) return null;
  if (client === "tty") return t("services.job.terminal");
  if (client === "script") return t("services.job.script");
  const name = client.replace(/^mcp:/, "").split(" ")[0] ?? "";
  if (name === "claude-code") return "Claude Code";
  if (name === "claude-ai" || name === "claude-desktop") return "Claude";
  if (!name) return null;
  return name.charAt(0).toUpperCase() + name.slice(1);
}

function jobTarget(job: JobInfo): string | null {
  const args = job.args;
  if (args && typeof args === "object" && !Array.isArray(args)) {
    const value = (args as Record<string, unknown>).id ?? (args as Record<string, unknown>).name;
    return typeof value === "string" ? value : null;
  }
  return null;
}

/** La tarea de un agente que está instalando este componente, si la hay. */
export function jobForComponent(s: Snapshot, id: string): JobInfo | undefined {
  return s.jobs.find((job) => job.kind === "component_install" && jobTarget(job) === id);
}

/** «Instalando redis… · pedido por Claude Code». */
export function jobText(job: JobInfo): string {
  const target = jobTarget(job);
  let what: string;
  if (job.message) what = job.message;
  else if (job.kind === "component_install")
    what = target ? t("services.job.installing", { target }) : t("services.job.installingAny");
  else if (job.kind === "project_create")
    what = target ? t("services.job.creating", { target }) : t("services.job.creatingAny");
  else what = job.state === "queued" ? t("services.job.queued") : t("services.job.running");
  const who = clientName(job.client);
  return who ? t("services.job.requestedBy", { what, who }) : what;
}

// ── PHP ─────────────────────────────────────────────────────────────────────

/** Rama de la versión por defecto («8.4»), para la barra lateral. */
export function phpBranch(s: Snapshot): string | null {
  const id = s.php.active?.id;
  if (!id) return null;
  const match = /^(\d+)\.(\d+)/.exec(id);
  return match ? `${match[1]}.${match[2]}` : id;
}

/**
 * Opciones para elegir una versión. Si dos instalaciones comparten versión (la
 * propia y la de Herd), se distinguen por el origen.
 */
export function phpOptions(versions: VersionInfo[]): { value: string; label: string }[] {
  const count = new Map<string, number>();
  for (const v of versions) count.set(v.id, (count.get(v.id) ?? 0) + 1);
  return versions.map((v) => ({
    value: v.key,
    label: (count.get(v.id) ?? 0) > 1 ? `${v.id} · ${v.source}` : v.id,
  }));
}

/**
 * La instalación que corresponde a la versión de un `.php-version` («8.3» o
 * «8.3.14»): primero la coincidencia exacta y luego la misma rama.
 */
export function versionForProject(versions: VersionInfo[], wanted: string | null): VersionInfo | undefined {
  if (!wanted) return undefined;
  return (
    versions.find((v) => v.id === wanted) ?? versions.find((v) => v.id.startsWith(`${wanted}.`))
  );
}

// ── Sitios ──────────────────────────────────────────────────────────────────

/** Proyectos con dirección web: lo que sirve Herd hoy (C17). */
export function sites(s: Snapshot): Project[] {
  return s.projects.filter((p) => p.hosts.length > 0);
}

export function siteUrl(p: Project): string | null {
  return p.hosts[0] ? `http://${p.hosts[0]}` : null;
}

/** `/Users/ana/Herd/acme` → `~/Herd/acme`. */
export function tildePath(path: string): string {
  return path.replace(/^\/Users\/[^/]+(?=\/|$)/, "~");
}

/** Recorta por el centro para que se vean el principio y el final de una ruta. */
export function middleTruncate(text: string, max: number): string {
  if (text.length <= max) return text;
  if (max <= 1) return "…";
  const keep = max - 1;
  const head = Math.ceil(keep / 2);
  const tail = keep - head;
  return `${text.slice(0, head)}…${tail > 0 ? text.slice(-tail) : ""}`;
}

// ── Panel ───────────────────────────────────────────────────────────────────

export type HeroTone = "ok" | "warn" | "bad";

export interface Hero {
  tone: HeroTone;
  title: string;
  detail: string;
  action: { kind: "problem" | "start" | "services"; label: string } | null;
}

/** Servicios que arrancan al iniciar sesión pero ahora están parados. */
export function startableServices(s: Snapshot): string[] {
  return services(s).flatMap((c) =>
    c.service && c.service.registered && !c.service.running ? [c.id] : [],
  );
}

/** El héroe del Panel (01 §7.2): estado, una línea de resumen y una sola acción. */
export function hero(s: Snapshot, memoryKb: number): Hero {
  const own = managedCounts(s);
  const running = runningServices(s).length + own.running;
  const installed = services(s).length + own.installed;
  const detail = [
    running > 0
      ? t("panel.hero.running", { count: running })
      : installed > 0
        ? t("panel.hero.noneRunning")
        : t("panel.hero.noServices"),
    s.php.active ? `PHP ${phpBranch(s)}` : null,
    memoryKb > 0 ? formatMemory(memoryKb) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  const problem = s.problems[0];
  if (problem) {
    const more = s.problems.length - 1;
    // Los fallos de acción traen la salida cruda de la orden (por ejemplo, de
    // launchctl): el titular dice qué ha fallado y «Ver problema» enseña el
    // detalle. Los errores de servicio ya son una frase corta.
    const name = problem.target === PHP_TARGET ? "PHP" : targetName(s, problem.target);
    return {
      tone: "bad",
      // El mensaje empieza a veces por el nombre del servicio: no se capitaliza.
      title:
        problem.kind === "action_failed" ? t("panel.hero.actionFailed", { name }) : problem.message,
      detail: more > 0 ? `${detail} · ${t("panel.hero.moreProblems", { count: more })}` : detail,
      action: { kind: "problem", label: t("panel.hero.viewProblem") },
    };
  }

  const busy = s.busy[0];
  const job = s.jobs[0];
  if (busy || job) {
    const title = busy
      ? busy.target === "services" || busy.target === PHP_TARGET
        ? busy.label
        : `${targetName(s, busy.target)}: ${lowerFirst(busy.label)}`
      : jobText(job!);
    return { tone: "warn", title, detail, action: null };
  }

  if (running > 0) {
    return { tone: "ok", title: t("panel.hero.allRunning"), detail, action: null };
  }
  const startable = startableServices(s);
  return {
    tone: "ok",
    title: t("panel.hero.nothingRunning"),
    detail,
    action:
      startable.length > 0
        ? { kind: "start", label: t("panel.hero.startServices") }
        : installed > 0
          ? { kind: "services", label: t("panel.hero.viewServices") }
          : null,
  };
}

/** Escala del anillo de RAM: el primer escalón que deja la cifra por debajo del 100 %. */
export function gaugeMaxKb(kb: number): number {
  const mb = 1024;
  for (const step of [512, 1024, 2048, 4096, 8192, 16384, 32768]) {
    if (kb <= step * mb) return step * mb;
  }
  return Math.ceil(kb / (16384 * mb)) * 16384 * mb;
}

function lowerFirst(text: string): string {
  return text.charAt(0).toLowerCase() + text.slice(1);
}

// ── Menús nativos ───────────────────────────────────────────────────────────

export type MenuEntry =
  | { text: string; enabled?: boolean; checked?: boolean; action?: () => void }
  | "separator";

// El menú anterior se suelta al abrir otro, no antes: sus acciones pueden llegar
// después de que `popup` resuelva.
let lastMenu: { close(): Promise<void> } | null = null;

/**
 * Menú contextual nativo (01 §6.6), en `at` (coordenadas de la ventana) o bajo el
 * puntero. `@tauri-apps/api/menu` se carga solo al usarlo.
 */
export async function popupMenu(entries: MenuEntry[], at?: { x: number; y: number }): Promise<void> {
  const [{ Menu }, { LogicalPosition }] = await Promise.all([
    import("@tauri-apps/api/menu"),
    import("@tauri-apps/api/dpi"),
  ]);
  const previous = lastMenu;
  lastMenu = null;
  void previous?.close().catch(() => {});
  const menu = await Menu.new({
    items: entries.map((e) =>
      e === "separator"
        ? { item: "Separator" as const }
        : e.checked === undefined
          ? { text: e.text, enabled: e.enabled ?? true, action: e.action }
          : { text: e.text, enabled: e.enabled ?? true, checked: e.checked, action: e.action },
    ),
  });
  lastMenu = menu;
  await menu.popup(at ? new LogicalPosition(at.x, at.y) : undefined);
}

/** Punto bajo el borde inferior izquierdo del botón pulsado, para colgar un menú de él. */
export function below(event: MouseEvent): { x: number; y: number } | undefined {
  const el = event.currentTarget;
  if (!(el instanceof Element)) return undefined;
  const rect = el.getBoundingClientRect();
  return { x: rect.left, y: rect.bottom + 2 };
}
