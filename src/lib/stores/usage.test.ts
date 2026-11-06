import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPoller, MISSING_REFRESH_MS, totalMemoryKb, USAGE_INTERVAL_MS, type PollerDeps } from "./usage.svelte";
import type { Usage } from "../bindings/Usage";

function u(pid: number, memory_kb = 1000): Usage {
  return { pid, memory_kb, cpu_percent: 0, processes: 1 };
}

function deps(over: Partial<PollerDeps> = {}) {
  const results: ReadonlyMap<number, Usage>[] = [];
  const d = {
    measure: vi.fn(async (pids: number[]) => pids.map((pid) => u(pid))),
    hidden: vi.fn(() => false),
    onMissing: vi.fn(),
    onResult: vi.fn((map: ReadonlyMap<number, Usage>) => void results.push(map)),
    now: () => Date.now(),
    ...over,
  };
  return { d, results };
}

describe("sondeo de uso", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("mide al arrancar y luego cada 5 s", async () => {
    const { d } = deps();
    const poller = createPoller(() => [1, 2, 2], d);
    poller.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(d.measure).toHaveBeenCalledTimes(1);
    // Los PIDs repetidos se miden una vez.
    expect(d.measure).toHaveBeenLastCalledWith([1, 2]);
    await vi.advanceTimersByTimeAsync(USAGE_INTERVAL_MS);
    expect(d.measure).toHaveBeenCalledTimes(2);
    poller.stop();
    await vi.advanceTimersByTimeAsync(USAGE_INTERVAL_MS * 3);
    expect(d.measure).toHaveBeenCalledTimes(2);
    expect(poller.running).toBe(false);
  });

  it("no mide con la ventana oculta", async () => {
    const { d } = deps({ hidden: () => true });
    await createPoller(() => [1], d).poll();
    expect(d.measure).not.toHaveBeenCalled();
  });

  it("sin servicios en marcha vacía la medida sin llamar a ps", async () => {
    const { d, results } = deps();
    await createPoller(() => [], d).poll();
    expect(d.measure).not.toHaveBeenCalled();
    expect(results.at(-1)?.size).toBe(0);
  });

  it("un PID que desaparece pide refrescar, como mucho una vez cada 15 s", async () => {
    const { d } = deps({ measure: vi.fn(async () => [u(1)]) });
    const poller = createPoller(() => [1, 2], d);
    await poller.poll();
    await poller.poll();
    expect(d.onMissing).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(MISSING_REFRESH_MS);
    await poller.poll();
    expect(d.onMissing).toHaveBeenCalledTimes(2);
  });

  it("un fallo de ps conserva la última medida", async () => {
    const measure = vi.fn(async (pids: number[]) => pids.map((pid) => u(pid)));
    const { d, results } = deps({ measure });
    const poller = createPoller(() => [1], d);
    await poller.poll();
    measure.mockRejectedValueOnce("ps ha fallado.");
    await poller.poll();
    expect(results).toHaveLength(1);
  });

  it("no solapa dos medidas", async () => {
    let finish: (list: Usage[]) => void = () => {};
    const measure = vi.fn(() => new Promise<Usage[]>((resolve) => (finish = resolve)));
    const { d } = deps({ measure });
    const poller = createPoller(() => [1], d);
    const first = poller.poll();
    await poller.poll();
    expect(measure).toHaveBeenCalledTimes(1);
    finish([u(1)]);
    await first;
  });

  it("suma la RAM de los PIDs conocidos", () => {
    const map = new Map([
      [1, u(1, 100)],
      [2, u(2, 250)],
    ]);
    expect(totalMemoryKb([1, 2, 3], map)).toBe(350);
    expect(totalMemoryKb([], map)).toBe(0);
  });
});
