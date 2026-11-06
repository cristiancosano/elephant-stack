import { describe, expect, it } from "vitest";
import { branchOf, compareVersions, mergePhpVersions } from "./php-versions";
import type { PhpRelease } from "./bindings/PhpRelease";
import type { VersionInfo } from "./bindings/VersionInfo";

const release = (version: string): PhpRelease => ({
  version,
  url: `https://example.test/php-${version}.tar.gz`,
  size: 1000,
  checksum_url: `https://example.test/php-${version}.tar.gz.sha256`,
});

const own = (id: string): VersionInfo => ({ key: id, id, source: "Elephant Stack" });

describe("mergePhpVersions", () => {
  it("lista cada versión una sola vez, de la más reciente a la más antigua", () => {
    const entries = mergePhpVersions([own("8.4.27")], [release("8.5.12"), release("8.4.27"), release("8.3.36")]);
    expect(entries.map((e) => `${e.kind}:${e.id}`)).toEqual([
      "available:8.5.12",
      "installed:8.4.27",
      "available:8.3.36",
    ]);
  });

  it("ofrece actualizar a la versión más reciente de la misma rama, sin repetirla", () => {
    const entries = mergePhpVersions(
      [own("8.4.27")],
      [release("8.4.29"), release("8.4.28"), release("8.4.27"), release("8.5.12")],
    );
    expect(entries.map((e) => `${e.kind}:${e.id}`)).toEqual(["available:8.5.12", "installed:8.4.27"]);
    const installed = entries.find((e) => e.kind === "installed");
    expect(installed?.kind === "installed" && installed.update?.version).toBe("8.4.29");
  });

  it("no ofrece como actualización una rama distinta ni una versión anterior", () => {
    const [entry] = mergePhpVersions([own("8.4.27")], [release("8.4.20")]).filter((e) => e.kind === "installed");
    expect(entry?.kind === "installed" && entry.update).toBeNull();
    const withOtherBranch = mergePhpVersions([own("8.4.27")], [release("8.5.12")]);
    const installed = withOtherBranch.find((e) => e.kind === "installed");
    expect(installed?.kind === "installed" && installed.update).toBeNull();
  });

  it("no ofrece actualizar a una versión que ya está instalada", () => {
    const entries = mergePhpVersions([own("8.4.27"), own("8.4.28")], [release("8.4.28")]);
    const old = entries.find((e) => e.id === "8.4.27");
    expect(old?.kind === "installed" && old.update).toBeNull();
  });

  it("muestra las instaladas aunque no haya catálogo y las disponibles aunque no haya instaladas", () => {
    expect(mergePhpVersions([own("8.4.27")], []).map((e) => e.kind)).toEqual(["installed"]);
    expect(mergePhpVersions([], [release("8.3.36")])).toEqual([
      expect.objectContaining({ kind: "available", id: "8.3.36", key: "release:8.3.36" }),
    ]);
  });
});

describe("compareVersions y branchOf", () => {
  it("ordena numéricamente y deja la rama sin parche detrás de sus parches", () => {
    expect(["8.4", "8.10.0", "8.4.27", "8.5.12"].sort(compareVersions)).toEqual(["8.10.0", "8.5.12", "8.4.27", "8.4"]);
  });

  it("solo da rama para versiones completas", () => {
    expect(branchOf("8.4.27")).toBe("8.4");
    expect(branchOf("8.4")).toBeNull();
  });
});
