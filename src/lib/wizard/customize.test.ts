import { describe, expect, it } from "vitest";

import { catalog, detection, profile } from "./fixtures.test-helpers";
import {
  branchOf,
  checkedBranches,
  defaultPhpOptions,
  phpBranches,
  serviceLabel,
  serviceRows,
  setBranch,
  setDefaultPhp,
  setService,
  setToggle,
} from "./customize";

const det = detection({
  php_versions: [
    { key: "8.4.27@Elephant Stack", id: "8.4.27", source: "Elephant Stack" },
    { key: "8.3.14@Herd", id: "8.3.14", source: "Herd" },
  ],
});

describe("servicios", () => {
  it("nombres legibles", () => {
    expect(serviceLabel("postgresql@18")).toEqual({ name: "PostgreSQL 18", purpose: "Base de datos" });
    expect(serviceLabel("mailpit").name).toBe("Mailpit");
    expect(serviceLabel("rabbitmq")).toEqual({ name: "rabbitmq", purpose: null });
  });

  it("filas: las conocidas en orden fijo, con «No tocar» si el perfil no las nombra", () => {
    const rows = serviceRows(profile({ services: { mailpit: "autostart", nginx: "installed" } }));
    expect(rows.map((r) => r.id)).toEqual(["mysql", "postgresql@18", "redis", "mailpit", "nginx"]);
    expect(rows.find((r) => r.id === "mailpit")?.want).toBe("autostart");
    expect(rows.find((r) => r.id === "redis")?.want).toBe("keep");
  });

  it("setService no muta el perfil original", () => {
    const before = profile();
    const after = setService(before, "redis", "installed");
    expect(after.services.redis).toBe("installed");
    expect(before.services.redis).toBeUndefined();
  });
});

describe("ramas de PHP", () => {
  it("une el catálogo y las propias, de la más nueva a la más vieja", () => {
    const branches = phpBranches(catalog(), det);
    expect(branches.map((b) => b.branch)).toEqual(["8.5", "8.4", "8.3"]);
    expect(branches[1]).toEqual({ branch: "8.4", installed: "8.4.27", latest: "8.4.27", size: 65_000_000 });
    expect(branches[0].installed).toBeNull();
    expect(branchOf("8.10.1")).toBe("8.10");
  });

  it("ordena 8.10 después de 8.9", () => {
    expect(phpBranches(catalog(["8.9.1", "8.10.0"]), null).map((b) => b.branch)).toEqual(["8.10", "8.9"]);
  });

  it("sin red solo quedan las propias (el PHP de Herd no cuenta)", () => {
    expect(phpBranches(null, det)).toEqual([{ branch: "8.4", installed: "8.4.27", latest: null, size: 0 }]);
  });

  it("any_own se pinta con las propias o, si no hay, con la última", () => {
    const own = phpBranches(catalog(), det);
    expect(checkedBranches(profile(), own)).toEqual(["8.4"]);
    const none = phpBranches(catalog(), detection());
    expect(checkedBranches(profile(), none)).toEqual(["8.5"]);
    expect(checkedBranches(profile({ php: { kind: "none" } }), own)).toEqual([]);
    expect(checkedBranches(profile({ php: { kind: "branches", value: ["8.5", "8.4"] } }), own)).toEqual(["8.5", "8.4"]);
  });

  it("marcar y desmarcar pasa a ramas concretas, y sin ninguna a «none»", () => {
    const branches = phpBranches(catalog(), det);
    let p = setBranch(profile(), branches, "8.5", true);
    expect(p.php).toEqual({ kind: "branches", value: ["8.5", "8.4"] });
    p = setBranch(p, branches, "8.5", false);
    p = setBranch(p, branches, "8.4", false);
    expect(p.php).toEqual({ kind: "none" });
  });

  it("desmarcar la rama por defecto vuelve a la versión automática", () => {
    const branches = phpBranches(catalog(), det);
    let p = setBranch(profile(), branches, "8.5", true);
    p = setDefaultPhp(p, "8.5.12");
    expect(p.default_php).toBe("8.5.12");
    p = setBranch(p, branches, "8.4", false);
    expect(p.default_php).toBe("8.5.12");
    p = setBranch(p, branches, "8.5", false);
    expect(p.default_php).toBeNull();
  });

  it("opciones de la versión por defecto: automática y una por rama marcada", () => {
    const branches = phpBranches(catalog(), det);
    const p = setBranch(profile(), branches, "8.5", true);
    expect(defaultPhpOptions(p, branches)).toEqual([
      { value: null, label: "Automática (la más reciente)" },
      { value: "8.5.12", label: "PHP 8.5.12 (se descargará)" },
      { value: "8.4.27", label: "PHP 8.4.27" },
    ]);
  });
});

describe("interruptores", () => {
  it("setToggle cambia solo ese campo", () => {
    const p = setToggle(profile(), "composer", false);
    expect(p.composer).toBe(false);
    expect(p.shim).toBe(true);
  });
});
