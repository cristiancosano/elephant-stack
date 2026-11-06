import { describe, expect, it } from "vitest";
import { compareVersions, mergePhpVersions } from "./php-versions";
import type { PhpRelease } from "./bindings/PhpRelease";
import type { VersionInfo } from "./bindings/VersionInfo";

const release = (version: string): PhpRelease => ({
  version,
  url: `https://example.test/php-${version}.tar.gz`,
  size: 1000,
  checksum_url: `https://example.test/php-${version}.tar.gz.sha256`,
});

const installed: VersionInfo[] = [
  { key: "8.4.27", id: "8.4.27", source: "Elephant Stack" },
  { key: "8.4@Herd", id: "8.4", source: "Herd" },
];

describe("mergePhpVersions", () => {
  it("lista cada versión una sola vez, de la más reciente a la más antigua", () => {
    const entries = mergePhpVersions(installed, [release("8.5.12"), release("8.4.27"), release("8.3.36")]);
    expect(entries.map((e) => `${e.kind}:${e.id}`)).toEqual([
      "available:8.5.12",
      "installed:8.4.27",
      "installed:8.4",
      "available:8.3.36",
    ]);
  });

  it("asocia la release a la versión instalada en lugar de repetirla", () => {
    const entries = mergePhpVersions(installed, [release("8.4.27")]);
    const own = entries.find((e) => e.id === "8.4.27");
    expect(own?.kind).toBe("installed");
    expect(own?.release?.version).toBe("8.4.27");
    expect(entries.filter((e) => e.id === "8.4.27")).toHaveLength(1);
  });

  it("muestra las instaladas aunque no haya catálogo", () => {
    expect(mergePhpVersions(installed, []).map((e) => e.kind)).toEqual(["installed", "installed"]);
  });

  it("muestra las disponibles aunque no haya ninguna instalada", () => {
    expect(mergePhpVersions([], [release("8.3.36")])).toEqual([
      expect.objectContaining({ kind: "available", id: "8.3.36", key: "release:8.3.36" }),
    ]);
  });
});

describe("compareVersions", () => {
  it("ordena numéricamente y deja la rama sin parche detrás de sus parches", () => {
    const ids = ["8.4", "8.10.0", "8.4.27", "8.5.12"].sort(compareVersions);
    expect(ids).toEqual(["8.10.0", "8.5.12", "8.4.27", "8.4"]);
  });
});
