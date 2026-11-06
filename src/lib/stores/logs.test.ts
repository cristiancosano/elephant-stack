import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LogBatch } from "../bindings/LogBatch";
import type { LogEntry } from "../bindings/LogEntry";
import type { LogPage } from "../bindings/LogPage";
import * as ipc from "../ipc";
import { DEFAULT_FILTERS, type Filters } from "../logs/filters";
import { entry } from "../logs/fixtures";
import { NOT_AVAILABLE } from "../logs/model";
import { LogsStore, MAX_EMPTY_PAGES } from "./logs.svelte";

vi.mock("../ipc", () => ({
  errorMessage: (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e)),
  logsQuery: vi.fn(),
  logsGroups: vi.fn(),
  logsSubscribe: vi.fn(),
  logsUnsubscribe: vi.fn(() => Promise.resolve()),
  logsEntry: vi.fn(),
  logsProjects: vi.fn(),
}));

const query = vi.mocked(ipc.logsQuery);
const groups = vi.mocked(ipc.logsGroups);
const subscribe = vi.mocked(ipc.logsSubscribe);
const unsubscribe = vi.mocked(ipc.logsUnsubscribe);

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function e(n: number, overrides: Partial<LogEntry> = {}): LogEntry {
  return entry({ id: `f-${n}`, ts_ms: n * 1000, fingerprint: `fp${n % 3}`, ...overrides });
}

/** Página como la da Rust: de más reciente a más antigua. */
function page(ns: number[], nextBefore: string | null = null): LogPage {
  return { entries: ns.map((n) => e(n)).reverse(), next_before: nextBefore, scanned_bytes: 1000 };
}

function batch(ns: number[], extra: Partial<LogBatch> = {}): LogBatch {
  return { subscription: 1, entries: ns.map((n) => e(n)), dropped: 0, truncated: [], ...extra };
}

/** El último `onBatch` que recibió `logsSubscribe`. */
function lastSink(): (b: LogBatch) => void {
  const call = subscribe.mock.calls.at(-1);
  if (!call) throw new Error("No hay suscripción");
  return call[1];
}

const ids = (s: LogsStore) => s.ring.toArray().map((x) => x.id);

beforeEach(() => {
  vi.clearAllMocks();
  subscribe.mockResolvedValue(7);
});

describe("carga", () => {
  it("pone la página en orden cronológico y se suscribe con los mismos filtros", async () => {
    query.mockResolvedValueOnce(page([1, 2, 3], "f-1"));
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    await flush();
    expect(s.state).toBe("ready");
    expect(ids(s)).toEqual(["f-1", "f-2", "f-3"]);
    expect(s.count).toBe(3);
    expect(s.historyDone).toBe(false);
    expect(s.streaming).toBe(true);
    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(subscribe.mock.calls[0][0].before).toBeNull();
  });

  it("descarta una respuesta que llega después de otra carga", async () => {
    let resolveFirst: (p: LogPage) => void = () => {};
    query.mockImplementationOnce(() => new Promise((r) => (resolveFirst = r)));
    query.mockResolvedValueOnce(page([10]));
    const s = new LogsStore();
    const first = s.load(DEFAULT_FILTERS);
    await s.load({ ...DEFAULT_FILTERS, level: "error" });
    resolveFirst(page([1, 2]));
    await first;
    await flush();
    expect(ids(s)).toEqual(["f-10"]);
    expect(subscribe).toHaveBeenCalledTimes(1);
  });

  it("el error de un stub deja la vista como «no disponible»", async () => {
    query.mockRejectedValueOnce(NOT_AVAILABLE);
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    expect(s.state).toBe("unavailable");
    expect(subscribe).not.toHaveBeenCalled();
  });

  it("otros errores se muestran tal cual", async () => {
    query.mockRejectedValueOnce("Permiso denegado");
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    expect(s.state).toBe("error");
    expect(s.error).toBe("Permiso denegado");
  });

  it("sin directo en el motor, la lista sigue funcionando", async () => {
    query.mockResolvedValueOnce(page([1]));
    subscribe.mockRejectedValueOnce(NOT_AVAILABLE);
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    await flush();
    expect(s.state).toBe("ready");
    expect(s.streaming).toBe(false);
    expect(s.streamError).toBeNull();
  });
});

describe("directo", () => {
  async function ready(ns = [1, 2, 3]): Promise<LogsStore> {
    query.mockResolvedValueOnce(page(ns));
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    await flush();
    return s;
  }

  it("los lotes se añaden al final, ordenados y sin repetir", async () => {
    const s = await ready();
    lastSink()(batch([5, 3, 4]));
    expect(ids(s)).toEqual(["f-1", "f-2", "f-3", "f-4", "f-5"]);
  });

  it("sin seguir la cola cuenta las nuevas y compensa lo expulsado", async () => {
    const s = await ready();
    const shifts: number[] = [];
    s.onShift = (n) => shifts.push(n);
    s.scrolled(false);
    lastSink()(batch([4, 5]));
    expect(s.follow).toEqual({ following: false, unseen: 2 });
    expect(shifts).toEqual([]); // no cabe expulsar nada con tres entradas
    s.jumpToEnd();
    expect(s.follow.following).toBe(true);
  });

  it("cuenta lo descartado y avisa de los truncados", async () => {
    const s = await ready();
    const truncated: string[][] = [];
    s.onTruncated = (keys) => truncated.push(keys);
    lastSink()(batch([], { dropped: 12, truncated: ["abc"] }));
    expect(s.dropped).toBe(12);
    expect(truncated).toEqual([["abc"]]);
  });

  it("pausar cancela la suscripción y reanudar trae lo que faltaba", async () => {
    const s = await ready([1, 2]);
    s.setLive(false);
    expect(unsubscribe).toHaveBeenCalledWith(7);
    expect(s.streaming).toBe(false);
    query.mockResolvedValueOnce(page([2, 3, 4]));
    s.setLive(true);
    await flush();
    await flush();
    expect(ids(s)).toEqual(["f-1", "f-2", "f-3", "f-4"]);
    expect(subscribe).toHaveBeenCalledTimes(2);
  });

  it("con la ventana oculta no hay suscripción (C3)", async () => {
    const s = await ready();
    s.setVisible(false);
    expect(unsubscribe).toHaveBeenCalledWith(7);
    query.mockResolvedValueOnce(page([3]));
    s.setVisible(true);
    await flush();
    await flush();
    expect(subscribe).toHaveBeenCalledTimes(2);
    expect(s.streaming).toBe(true);
  });

  it("un lote de una suscripción antigua se ignora", async () => {
    const s = await ready([1]);
    const oldSink = lastSink();
    query.mockResolvedValueOnce(page([20]));
    await s.load({ ...DEFAULT_FILTERS, source: "app" });
    oldSink(batch([2]));
    expect(ids(s)).toEqual(["f-20"]);
  });

  it("si se desmonta mientras llega la suscripción, la cancela", async () => {
    let resolveSub: (id: number) => void = () => {};
    subscribe.mockImplementationOnce(() => new Promise((r) => (resolveSub = r)));
    query.mockResolvedValueOnce(page([1]));
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    s.dispose();
    resolveSub(42);
    await flush();
    expect(unsubscribe).toHaveBeenCalledWith(42);
    expect(s.ring.length).toBe(0);
  });
});

describe("historial y limpiar", () => {
  it("carga lo anterior con el cursor y avisa del desplazamiento", async () => {
    query.mockResolvedValueOnce(page([5, 6], "f-5"));
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    const shifts: number[] = [];
    s.onShift = (n) => shifts.push(n);
    query.mockResolvedValueOnce(page([3, 4]));
    await s.loadOlder();
    expect(query.mock.calls[1][0].before).toBe("f-5");
    expect(ids(s)).toEqual(["f-3", "f-4", "f-5", "f-6"]);
    expect(shifts).toEqual([2]);
    expect(s.historyDone).toBe(true);
    await s.loadOlder();
    expect(query).toHaveBeenCalledTimes(2);
  });

  it("una primera página vacía con cursor sigue buscando en vez de darse por vacía", async () => {
    query.mockResolvedValueOnce({ entries: [], next_before: "x", scanned_bytes: 32 });
    query.mockResolvedValueOnce({ entries: [], next_before: "y", scanned_bytes: 32 });
    query.mockResolvedValueOnce(page([1], null));
    const s = new LogsStore();
    await s.load({ ...DEFAULT_FILTERS, text: "texto raro" });
    await flush();
    await flush();
    expect(query).toHaveBeenCalledTimes(3);
    expect(query.mock.calls[1][0].before).toBe("x");
    expect(query.mock.calls[2][0].before).toBe("y");
    expect(ids(s)).toEqual(["f-1"]);
    expect(s.hasOlder).toBe(false);
    expect(s.scannedBytes).toBe(32 + 32 + 1000);
  });

  it("tras varias páginas vacías se para y deja seguir a mano", async () => {
    query.mockResolvedValue({ entries: [], next_before: "x", scanned_bytes: 1 });
    const s = new LogsStore();
    await s.load({ ...DEFAULT_FILTERS, text: "nada" });
    await flush();
    await flush();
    expect(query).toHaveBeenCalledTimes(MAX_EMPTY_PAGES);
    expect(s.count).toBe(0);
    expect(s.hasOlder).toBe(true);
    expect(s.olderPaused).toBe(true);
    expect(s.loadingOlder).toBe(false);
    await s.loadOlder();
    expect(query).toHaveBeenCalledTimes(2 * MAX_EMPTY_PAGES);
    query.mockReset();
  });

  it("una carga nueva no se queda bloqueada por el historial de la anterior", async () => {
    query.mockResolvedValueOnce(page([5, 6], "f-5"));
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    query.mockImplementationOnce(() => new Promise(() => {}));
    void s.loadOlder();
    expect(s.loadingOlder).toBe(true);
    query.mockResolvedValueOnce(page([7, 8], "f-7"));
    await s.load({ ...DEFAULT_FILTERS, level: "error" });
    expect(s.loadingOlder).toBe(false);
    query.mockResolvedValueOnce(page([1]));
    await s.loadOlder();
    expect(ids(s)).toEqual(["f-1", "f-7", "f-8"]);
  });

  it("limpiar vacía la lista sin cortar el directo ni volver a pedir historial", async () => {
    query.mockResolvedValueOnce(page([1, 2], "f-1"));
    const s = new LogsStore();
    await s.load(DEFAULT_FILTERS);
    await flush();
    s.clearView();
    expect(s.count).toBe(0);
    expect(s.historyDone).toBe(true);
    expect(unsubscribe).not.toHaveBeenCalled();
    lastSink()(batch([3]));
    expect(ids(s)).toEqual(["f-3"]);
    await s.loadOlder();
    expect(query).toHaveBeenCalledTimes(1);
  });
});

describe("agrupados", () => {
  it("carga los grupos y suma lo que llega en directo", async () => {
    groups.mockResolvedValueOnce([
      { fingerprint: "fp1", count: 2, first_ms: 0, last_ms: 1000, sample: e(1), projects: ["p1"] },
    ]);
    const s = new LogsStore();
    const f: Filters = { ...DEFAULT_FILTERS, mode: "grouped" };
    await s.load(f);
    await flush();
    expect(query).not.toHaveBeenCalled();
    lastSink()(batch([4, 5]));
    expect(s.groups.map((g) => [g.fingerprint, g.count])).toEqual([
      ["fp2", 1],
      ["fp1", 3],
    ]);
    expect(s.ring.length).toBe(0);
  });
});

describe("detalle", () => {
  it("se queda con la última petición", async () => {
    let resolveA: (d: Awaited<ReturnType<typeof ipc.logsEntry>>) => void = () => {};
    const entryMock = vi.mocked(ipc.logsEntry);
    entryMock.mockImplementationOnce(() => new Promise((r) => (resolveA = r)));
    entryMock.mockResolvedValueOnce({ entry: e(2), body: "b", frames: [], context: null });
    const s = new LogsStore();
    const a = s.showDetail("f-1");
    await s.showDetail("f-2");
    resolveA({ entry: e(1), body: "a", frames: [], context: null });
    await a;
    expect(s.detail?.id).toBe("f-2");
    expect(s.detail?.data?.body).toBe("b");
  });

  it("guarda el error para mostrarlo en el inspector", async () => {
    vi.mocked(ipc.logsEntry).mockRejectedValueOnce("La entrada ya no existe: el fichero ha cambiado.");
    const s = new LogsStore();
    await s.showDetail("f-9");
    expect(s.detail).toMatchObject({ id: "f-9", data: null, loading: false, error: "La entrada ya no existe: el fichero ha cambiado." });
  });
});
