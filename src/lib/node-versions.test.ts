import { describe, expect, it } from "vitest";
import { hiddenReleases, ltsName, mergeNodeVersions, specFor } from "./node-versions";
import type { NodeInstalled } from "./bindings/NodeInstalled";
import type { NodeRelease } from "./bindings/NodeRelease";

const installed = (version: string, lts: string | null = "krypton"): NodeInstalled => ({
  version,
  major: Number(version.split(".")[0]),
  lts,
  path: `/h/.elephant-stack/node/${version}`,
  npm: "11.6.2",
  globals: [],
});

const release = (version: string, featured: boolean, lts: string | null = null): NodeRelease => ({
  version,
  major: Number(version.split(".")[0]),
  date: "2026-10-27",
  lts,
  npm: null,
  security: false,
  featured,
});

const releases = [
  release("26.13.0", true, "lithium"),
  release("25.9.0", false),
  release("24.22.2", true, "krypton"),
  release("22.23.4", true, "jod"),
  release("20.20.2", false, "iron"),
];

describe("mergeNodeVersions", () => {
  it("ofrece actualizar dentro de la mayor y no repite las mayores instaladas", () => {
    const entries = mergeNodeVersions([installed("24.11.1")], releases);
    expect(entries.map((e) => e.id)).toEqual(["26.13.0", "24.11.1", "22.23.4"]);
    const node24 = entries[1];
    expect(node24.kind === "installed" && node24.update?.version).toBe("24.22.2");
  });

  it("con «todas» enseña también las mayores sin soporte", () => {
    const entries = mergeNodeVersions([], releases, true);
    expect(entries.map((e) => e.id)).toEqual(["26.13.0", "25.9.0", "24.22.2", "22.23.4", "20.20.2"]);
    expect(hiddenReleases([], releases)).toBe(2);
    expect(hiddenReleases([installed("20.19.5", "iron")], releases)).toBe(1);
  });

  it("una ya al día o una más antigua de su mayor no se actualiza", () => {
    const entries = mergeNodeVersions([installed("24.22.2"), installed("24.11.1")], releases);
    const updates = entries.flatMap((e) => (e.kind === "installed" && e.update ? [e.id] : []));
    expect(updates).toEqual([]);
  });
});

describe("specFor", () => {
  it("escribe la mayor salvo que haya otra más reciente de esa mayor", () => {
    expect(specFor("24.22.2", [installed("24.22.2"), installed("24.11.1")])).toBe("24");
    expect(specFor("24.11.1", [installed("24.22.2"), installed("24.11.1")])).toBe("24.11.1");
    expect(specFor("22.23.4", [installed("22.23.4")])).toBe("22");
  });

  it("pone en mayúscula el nombre de la LTS", () => {
    expect(ltsName("krypton")).toBe("Krypton");
  });
});
