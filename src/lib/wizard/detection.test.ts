import { describe, expect, it } from "vitest";

import { findings, herdBanner } from "./detection";
import { detection } from "./fixtures.test-helpers";

const texts = (d = detection()) => findings(d).map((f) => f.text);

describe("findings", () => {
  it("un Mac como el del usuario", () => {
    const d = detection({
      formulas: ["postgresql@18", "mailpit", "node"],
      herd: { running: true, tld: "test", sites: 12 },
      php_versions: [
        { key: "8.4.27@Elephant Stack", id: "8.4.27", source: "Elephant Stack" },
        { key: "8.4.10@Herd", id: "8.4.10", source: "Herd" },
        { key: "8.3.14@Herd", id: "8.3.14", source: "Herd" },
      ],
      services: [
        { name: "postgresql@18", status: "started", running: true, registered: true, pid: 10, exit_code: null },
        { name: "mailpit", status: "none", running: false, registered: false, pid: null, exit_code: null },
      ],
      path: "active",
    });
    expect(texts(d)).toEqual([
      "Homebrew · 3 fórmulas",
      "Herd en marcha · 12 sitios .test",
      "PHP 8.4.27 propio · PHP 8.4 y 8.3 de Herd",
      "PostgreSQL 18 en marcha",
      "Selector php en la terminal",
    ]);
    expect(herdBanner(d)).toMatch(/convive/);
  });

  it("un Mac vacío", () => {
    expect(texts()).toEqual(["Sin fórmulas de Homebrew", "Sin ninguna versión de PHP"]);
    expect(herdBanner(detection())).toBeNull();
    expect(herdBanner(null)).toBeNull();
  });

  it("avisa si no es Apple Silicon y de los puertos ocupados por otros", () => {
    const f = findings(
      detection({
        arch_arm64: false,
        ports: [
          { port: 3306, busy: true, owner_hint: "mysqld de cristian-smw" },
          { port: 6379, busy: false, owner_hint: null },
        ],
      }),
    );
    expect(f[0]).toMatchObject({ id: "arch", tone: "warning" });
    expect(f.map((x) => x.text)).toContain("El puerto 3306 lo usa mysqld de cristian-smw");
    expect(f.some((x) => x.id === "port:6379")).toBe(false);
  });

  it("Herd parado y sin número de sitios", () => {
    expect(texts(detection({ herd: { running: false, tld: null, sites: null } }))).toContain("Herd instalado");
  });

  it("varias versiones propias", () => {
    const d = detection({
      composer_phar: true,
      php_versions: [
        { key: "a", id: "8.5.12", source: "Elephant Stack" },
        { key: "b", id: "8.4.27", source: "Elephant Stack" },
      ],
    });
    expect(texts(d)).toContain("PHP 8.5.12 y 8.4.27 propios");
    expect(texts(d)).toContain("Composer propio");
  });
});
