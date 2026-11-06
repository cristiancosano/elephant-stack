import { describe, expect, it } from "vitest";

import type { LogGroup } from "../bindings/LogGroup";
import { entry } from "./fixtures";
import { mergeIntoGroups } from "./groups";

function group(fingerprint: string, count: number, last: number, projects: string[] = ["p1"]): LogGroup {
  return { fingerprint, count, first_ms: last - 1000, last_ms: last, sample: entry({ fingerprint, ts_ms: last }), projects };
}

describe("mergeIntoGroups", () => {
  it("suma al grupo existente y lo sube arriba", () => {
    const groups = [group("a", 3, 2000), group("b", 1, 1000)];
    const out = mergeIntoGroups(groups, [entry({ id: "x-1", fingerprint: "b", ts_ms: 5000, project: "p2" })]);
    expect(out.map((g) => g.fingerprint)).toEqual(["b", "a"]);
    expect(out[0]).toMatchObject({ count: 2, last_ms: 5000, projects: ["p1", "p2"] });
    expect(out[0].sample.id).toBe("x-1");
  });

  it("crea grupos nuevos", () => {
    const out = mergeIntoGroups([], [entry({ fingerprint: "n", ts_ms: 10, project: null })]);
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ count: 1, first_ms: 10, last_ms: 10, projects: [] });
  });

  it("no modifica la lista de entrada", () => {
    const groups = [group("a", 1, 100)];
    mergeIntoGroups(groups, [entry({ fingerprint: "a", ts_ms: 200 })]);
    expect(groups[0].count).toBe(1);
    expect(groups[0].projects).toEqual(["p1"]);
  });

  it("una entrada más antigua no cambia la muestra", () => {
    const out = mergeIntoGroups([group("a", 1, 5000)], [entry({ id: "old-1", fingerprint: "a", ts_ms: 100 })]);
    expect(out[0].sample.id).not.toBe("old-1");
    expect(out[0].first_ms).toBe(100);
    expect(out[0].last_ms).toBe(5000);
  });
});
