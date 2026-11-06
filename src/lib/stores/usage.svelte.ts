// Consumo de RAM y CPU de los servicios en marcha. Se mide cada 5 s y solo con
// la ventana visible (01 §4.3, regla 4): con `visibilitychange` oculta se para el
// sondeo y se pide a Rust que cierre la ventana si se ha minimizado.

import { closeIfMinimized, processUsage, refresh } from "../ipc";
import type { Usage } from "../bindings/Usage";

export const USAGE_INTERVAL_MS = 5000;
/** Un PID que desaparece obliga a refrescar el snapshot, pero no más de una vez en este tiempo. */
export const MISSING_REFRESH_MS = 15_000;

let byPid = $state.raw<ReadonlyMap<number, Usage>>(new Map());

export const usage = {
  get byPid(): ReadonlyMap<number, Usage> {
    return byPid;
  },
};

export function usageOf(pid: number | null | undefined): Usage | undefined {
  return pid ? byPid.get(pid) : undefined;
}

/** RAM total en KiB de estos PIDs con lo último que se ha medido. */
export function totalMemoryKb(pids: readonly number[], map: ReadonlyMap<number, Usage> = byPid): number {
  return pids.reduce((total, pid) => total + (map.get(pid)?.memory_kb ?? 0), 0);
}

export interface PollerDeps {
  measure: (pids: number[]) => Promise<Usage[]>;
  hidden: () => boolean;
  /** Se ha dejado de ver algún PID: el servicio se ha parado o reiniciado. */
  onMissing: () => void;
  onResult: (map: ReadonlyMap<number, Usage>) => void;
  now?: () => number;
}

export interface Poller {
  poll(): Promise<void>;
  start(): void;
  stop(): void;
  readonly running: boolean;
}

/** El sondeo, sin Tauri ni DOM, para poder probarlo. */
export function createPoller(getPids: () => number[], deps: PollerDeps, intervalMs = USAGE_INTERVAL_MS): Poller {
  let timer: ReturnType<typeof setInterval> | null = null;
  let inFlight = false;
  let lastMissingAt = -Infinity;
  const now = deps.now ?? Date.now;

  async function poll(): Promise<void> {
    if (deps.hidden() || inFlight) return;
    const pids = [...new Set(getPids())];
    if (pids.length === 0) {
      deps.onResult(new Map());
      return;
    }
    inFlight = true;
    try {
      const list = await deps.measure(pids);
      deps.onResult(new Map(list.map((u) => [u.pid, u])));
      if (list.length < pids.length && now() - lastMissingAt >= MISSING_REFRESH_MS) {
        lastMissingAt = now();
        deps.onMissing();
      }
    } catch {
      // `ps` puede fallar de forma puntual: se conserva la última medida.
    } finally {
      inFlight = false;
    }
  }

  return {
    poll,
    start() {
      if (timer !== null) return;
      timer = setInterval(() => void poll(), intervalMs);
      void poll();
    },
    stop() {
      if (timer !== null) clearInterval(timer);
      timer = null;
    },
    get running() {
      return timer !== null;
    },
  };
}

/**
 * Arranca el sondeo real. `getPids` se lee en cada medida; quien cambie la lista
 * de PIDs (un servicio que arranca) puede llamar a `poll()` para no esperar 5 s.
 * Devuelve el sondeo y una función que lo para todo.
 */
export function startUsage(getPids: () => number[]): { poller: Poller; stop: () => void } {
  const poller = createPoller(getPids, {
    measure: processUsage,
    hidden: () => document.hidden,
    onMissing: () => void refresh().catch(() => {}),
    onResult: (map) => {
      byPid = map;
    },
  });

  const onVisibility = () => {
    if (document.hidden) {
      poller.stop();
      // Si se ha minimizado, Rust la cierra para que no quede en el Dock.
      void closeIfMinimized().catch(() => {});
    } else {
      poller.start();
    }
  };

  document.addEventListener("visibilitychange", onVisibility);
  if (!document.hidden) poller.start();

  return {
    poller,
    stop() {
      poller.stop();
      document.removeEventListener("visibilitychange", onVisibility);
    },
  };
}
