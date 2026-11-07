import { describe, expect, it } from "vitest";
import { mergeManagedVersions, runningBranch } from "./managed-services";
import type { BranchStatus } from "./bindings/BranchStatus";
import type { ManagedService } from "./bindings/ManagedService";
import type { ServiceRelease } from "./bindings/ServiceRelease";

const release = (version: string, lts = true): ServiceRelease => ({
  service: "mysql",
  version,
  branch: version.split(".").slice(0, 2).join("."),
  url: `https://cdn.mysql.com/mysql-${version}-macos15-arm64.tar.gz`,
  sha256: "a".repeat(64),
  size: 190_000_000,
  lts,
});

const branch = (b: string, version: string, over: Partial<BranchStatus> = {}): BranchStatus => ({
  branch: b,
  version,
  label: `sh.elephant.mysql.${b}`,
  state: "stopped",
  pid: null,
  autostart: false,
  error: null,
  data_dir: `/Users/ana/.elephant-stack/data/mysql/${b}`,
  data_ready: true,
  log_path: `/Users/ana/.elephant-stack/logs/mysql-${b}.err`,
  ...over,
});

function mysql(versions: string[], branches: BranchStatus[]): ManagedService {
  return {
    service: "mysql",
    name: "MySQL",
    port: 3306,
    web_port: null,
    versions: versions.map((version) => ({
      version,
      branch: version.split(".").slice(0, 2).join("."),
      path: `/Users/ana/.elephant-stack/services/mysql/${version}`,
    })),
    branches,
    import: null,
  };
}

const CATALOG = [release("26.10.0", false), release("9.7.2"), release("9.7.1"), release("8.4.11"), release("8.4.7")];

describe("mergeManagedVersions", () => {
  it("sin nada instalado ofrece la última de cada rama", () => {
    const entries = mergeManagedVersions(mysql([], []), CATALOG);
    expect(entries.map((e) => (e.kind === "available" ? e.release.version : ""))).toEqual(["26.10.0", "9.7.2", "8.4.11"]);
  });

  it("una rama instalada ofrece actualizar a su último parche y no se repite", () => {
    const entries = mergeManagedVersions(mysql(["8.4.7"], [branch("8.4", "8.4.7")]), CATALOG);
    expect(entries.map((e) => `${e.kind}:${e.kind === "installed" ? e.version.version : e.release.version}`)).toEqual([
      "available:26.10.0",
      "available:9.7.2",
      "installed:8.4.7",
    ]);
    const installed = entries[2];
    expect(installed.kind === "installed" && installed.update?.version).toBe("8.4.11");
    expect(installed.kind === "installed" && installed.branch?.branch).toBe("8.4");
  });

  it("dentro de una rama, solo la más reciente lleva el estado; las demás dicen cuál se usa", () => {
    const entries = mergeManagedVersions(mysql(["8.4.11", "8.4.7"], [branch("8.4", "8.4.11")]), CATALOG);
    const [newer, older] = entries.filter((e) => e.kind === "installed");
    expect(newer.kind === "installed" && [newer.branch?.version, newer.update, newer.usedInstead]).toEqual([
      "8.4.11",
      null,
      null,
    ]);
    expect(older.kind === "installed" && [older.branch, older.update, older.usedInstead]).toEqual([null, null, "8.4.11"]);
  });

  it("sin catálogo quedan las instaladas", () => {
    const entries = mergeManagedVersions(mysql(["9.7.1"], [branch("9.7", "9.7.1")]), []);
    expect(entries).toHaveLength(1);
  });

  it("encuentra la rama en marcha", () => {
    const service = mysql(["8.4.11", "9.7.1"], [branch("8.4", "8.4.11"), branch("9.7", "9.7.1", { state: "running", pid: 7 })]);
    expect(runningBranch(service)?.branch).toBe("9.7");
    expect(runningBranch(mysql([], []))).toBeNull();
  });
});
