// Única puerta a los comandos de Rust (00 §5.6). Ningún otro archivo importa
// `@tauri-apps/api/core`. Los tipos salen de ./bindings, que genera ts-rs.

import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import type { Appearance } from "./bindings/Appearance";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { ApplyReport } from "./bindings/ApplyReport";
import type { AuditEntry } from "./bindings/AuditEntry";
import type { Catalog } from "./bindings/Catalog";
import type { Detection } from "./bindings/Detection";
import type { EntryDetail } from "./bindings/EntryDetail";
import type { LogBatch } from "./bindings/LogBatch";
import type { LogGroup } from "./bindings/LogGroup";
import type { LogPage } from "./bindings/LogPage";
import type { LogQuery } from "./bindings/LogQuery";
import type { McpSnippets } from "./bindings/McpSnippets";
import type { McpStatus } from "./bindings/McpStatus";
import type { McpTestResult } from "./bindings/McpTestResult";
import type { Operation } from "./bindings/Operation";
import type { PhpReplaceReport } from "./bindings/PhpReplaceReport";
import type { Plan } from "./bindings/Plan";
import type { Profile } from "./bindings/Profile";
import type { ProfileInput } from "./bindings/ProfileInput";
import type { ProjectOverride } from "./bindings/ProjectOverride";
import type { ProjectSummary } from "./bindings/ProjectSummary";
import type { ScenarioCard } from "./bindings/ScenarioCard";
import type { Settings } from "./bindings/Settings";
import type { SetupFile } from "./bindings/SetupFile";
import type { SetupProgress } from "./bindings/SetupProgress";
import type { SetupStatus } from "./bindings/SetupStatus";
import type { ShellPreview } from "./bindings/ShellPreview";
import type { SiteAction } from "./bindings/SiteAction";
import type { Snapshot } from "./bindings/Snapshot";
import type { SourceCheck } from "./bindings/SourceCheck";
import type { Usage } from "./bindings/Usage";

/** Canal de Tauri que entrega cada mensaje a `cb`. */
function channel<T>(cb: (message: T) => void): Channel<T> {
  const ch = new Channel<T>();
  ch.onmessage = cb;
  return ch;
}

/** Los `Err` de Rust llegan como `string`; el resto, por si acaso. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

/** Selector de carpetas nativo. `null` si se cancela. */
export async function pickFolder(title: string): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

// ── Comunes (commands/core.rs) ──────────────────────────────────────────────

/** Al instante; si tiene más de 2 s, el nuevo llega por `state:changed`. */
export const getSnapshot = () => invoke<Snapshot>("get_snapshot");
export const refresh = () => invoke<void>("refresh");
export const serviceOperation = (name: string, operation: Operation) =>
  invoke<void>("service_operation", { name, operation });
/** Solo `restart` y `turn_off`, sobre los que están en marcha. */
export const servicesBulk = (operation: Operation) => invoke<void>("services_bulk", { operation });

export function installComponent(id: string, onLine: (line: string) => void): Promise<void> {
  return invoke<void>("install_component", { id, onLine: channel(onLine) });
}

export const setGlobalPhp = (key: string) => invoke<void>("set_global_php", { key });
export const setProjectPhp = (dir: string, key: string) => invoke<void>("set_project_php", { dir, key });
export const phpReplace = (old: string, next: string) =>
  invoke<PhpReplaceReport>("php_replace", { old, new: next });
export const processUsage = (pids: number[]) => invoke<Usage[]>("process_usage", { pids });
export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) => invoke<Settings>("set_settings", { settings });
/** Ruta pedida antes de que existiera la ventana. Se consume una vez, al montar. */
export const takePendingRoute = () => invoke<string | null>("take_pending_route");
export const appearance = () => invoke<Appearance>("appearance");
export const closeIfMinimized = () => invoke<void>("close_if_minimized");
export const openInEditor = (file: string, line: number | null = null) =>
  invoke<void>("open_in_editor", { file, line });
/** Solo `http` y `https`. */
export const openUrl = (url: string) => invoke<void>("open_url", { url });
export const revealInFinder = (path: string) => invoke<void>("reveal_in_finder", { path });
export const openTerminal = (dir: string) => invoke<void>("open_terminal", { dir });
export const siteAction = (projectId: string, action: SiteAction) =>
  invoke<void>("site_action", { projectId, action });
export const openServiceLog = (name: string) => invoke<void>("open_service_log", { name });
/** «Arrancar lo que necesita» un proyecto. Devuelve los servicios arrancados. */
export const projectStartServices = (dir: string) => invoke<string[]>("project_start_services", { dir });
/** Quita los problemas `action_failed` de un objetivo al abrir su vista (01 §9.4). */
export const clearProblems = (target: string) => invoke<void>("clear_problems", { target });

// ── Logs (commands/logs.rs) ─────────────────────────────────────────────────

export const logsProjects = () => invoke<ProjectSummary[]>("logs_projects");
/** Carpetas raíz que ha añadido el usuario (`projects.json`). */
export const logsRoots = () => invoke<string[]>("logs_roots");
export const logsQuery = (q: LogQuery) => invoke<LogPage>("logs_query", { q });
export const logsEntry = (id: string) => invoke<EntryDetail>("logs_entry", { id });
export const logsGroups = (q: LogQuery) => invoke<LogGroup[]>("logs_groups", { q });

/** Lotes cada ≤ 100 ms. Devuelve el id para `logsUnsubscribe`. */
export function logsSubscribe(q: LogQuery, onBatch: (b: LogBatch) => void): Promise<number> {
  return invoke<number>("logs_subscribe", { q, onBatch: channel(onBatch) });
}

export const logsUnsubscribe = (id: number) => invoke<void>("logs_unsubscribe", { id });
export const logsAddRoot = (path: string) => invoke<ProjectSummary[]>("logs_add_root", { path });
export const logsRemoveRoot = (path: string) => invoke<ProjectSummary[]>("logs_remove_root", { path });
export const logsSetOverride = (project: string, o: ProjectOverride) =>
  invoke<ProjectSummary[]>("logs_set_override", { project, o });
/** Vacía el archivo. Pide confirmación antes de llamarlo. */
export const logsClear = (fileKey: string) => invoke<void>("logs_clear", { fileKey });
export const logsMarkSeen = () => invoke<void>("logs_mark_seen");

// ── Asistente (commands/setup.rs) ───────────────────────────────────────────

export const setupStatus = () => invoke<SetupStatus>("setup_status");
export const setupDetect = () => invoke<Detection>("setup_detect");
export const setupScenarios = () => invoke<ScenarioCard[]>("setup_scenarios");
/** `null` = el estado actual del sistema. */
export const setupProfile = (input: ProfileInput | null) => invoke<Profile>("setup_profile", { input });
export const setupPreview = (input: ProfileInput) => invoke<Plan>("setup_preview", { input });

/** `accepted`: claves de los pasos opcionales marcados. */
export function setupApply(
  planId: string,
  accepted: string[],
  onEvent: (e: SetupProgress) => void,
): Promise<ApplyOutcome> {
  return invoke<ApplyOutcome>("setup_apply", { planId, accepted, onEvent: channel(onEvent) });
}

export const setupCancel = () => invoke<void>("setup_cancel");
export const setupSkip = () => invoke<SetupFile>("setup_skip");
export const setupShellPreview = () => invoke<ShellPreview>("setup_shell_preview");
export const setupShellApply = (hash: string) => invoke<void>("setup_shell_apply", { hash });
export const phpCatalog = () => invoke<Catalog>("php_catalog");
export const phpSourceCheck = (source: string) => invoke<SourceCheck>("php_source_check", { source });

export function phpDownload(version: string, onEvent: (e: SetupProgress) => void): Promise<ApplyReport> {
  return invoke<ApplyReport>("php_download", { version, onEvent: channel(onEvent) });
}

// ── Canal de agentes (commands/mcp.rs; conserva los nombres, C26) ───────────

export const mcpStatus = () => invoke<McpStatus>("mcp_status");
export const mcpEnable = () => invoke<McpStatus>("mcp_enable");
export const mcpDisable = () => invoke<McpStatus>("mcp_disable");
export const mcpConnectClaude = () => invoke<McpStatus>("mcp_connect_claude");
export const mcpTest = () => invoke<McpTestResult>("mcp_test");
export const mcpSnippets = () => invoke<McpSnippets>("mcp_snippets");
export const mcpActivity = (limit: number) => invoke<AuditEntry[]>("mcp_activity", { limit });
