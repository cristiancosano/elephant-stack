// Datos de prueba para los tests de src/lib/logs (no entra en el build: solo lo
// importan los *.test.ts).

import type { EntryDetail } from "../bindings/EntryDetail";
import type { Frame } from "../bindings/Frame";
import type { LogEntry } from "../bindings/LogEntry";
import type { Project } from "../bindings/Project";
import type { ProjectSummary } from "../bindings/ProjectSummary";

export function entry(overrides: Partial<LogEntry> = {}): LogEntry {
  return {
    id: "00000000000000ab-10",
    kind: "laravel_app",
    project: "p1",
    host: null,
    ts_ms: Date.UTC(2026, 10, 6, 8, 5, 3, 7),
    level: "error",
    channel: "local",
    message: "Undefined variable $pedido",
    exception: null,
    http: null,
    fingerprint: "f1",
    len: 120,
    ...overrides,
  };
}

export function frame(index: number, file: string, vendor = false, framework = false): Frame {
  return { index, file, line: 10 + index, call: `fn${index}()`, vendor, framework };
}

export function detail(overrides: Partial<EntryDetail> = {}): EntryDetail {
  return { entry: entry(), body: "", frames: [], context: null, ...overrides };
}

export function project(overrides: Partial<Project> = {}): Project {
  return {
    id: "p1",
    name: "tienda",
    root: "/Users/ana/Code/tienda",
    hosts: ["tienda.test"],
    kind: "laravel",
    origin: "herd_parked",
    php_version: "8.4",
    log_tz: "UTC",
    ...overrides,
  };
}

export function summary(p: Partial<Project> = {}, extra: Partial<ProjectSummary> = {}): ProjectSummary {
  return { project: project(p), sources: [], errors_24h: 0, errors_capped: false, last_ms: null, ...extra };
}
