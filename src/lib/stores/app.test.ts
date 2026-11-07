import { afterEach, describe, expect, it } from "vitest";
import {
  app,
  applySnapshot,
  brewBusy,
  clientName,
  gaugeMaxKb,
  hero,
  installedText,
  isNewer,
  jobForComponent,
  jobText,
  middleTruncate,
  missing,
  phpBranch,
  phpOptions,
  problemRoute,
  problemsClearedBy,
  resetApp,
  managedCounts,
  runningPids,
  serviceStatusText,
  services,
  sites,
  startableServices,
  targetName,
  tildePath,
  tools,
  versionForProject,
} from "./app.svelte";
import type { Component } from "../bindings/Component";
import type { JobInfo } from "../bindings/JobInfo";
import type { Project } from "../bindings/Project";
import type { Service } from "../bindings/Service";
import type { Snapshot } from "../bindings/Snapshot";

function service(name: string, over: Partial<Service> = {}): Service {
  return { name, status: "none", running: false, registered: false, pid: null, exit_code: null, ...over };
}

function component(id: string, over: Partial<Component> = {}): Component {
  return {
    id,
    kind: "service",
    description: "",
    installed: true,
    version: null,
    origin: "Homebrew",
    path: null,
    service: service(id),
    ...over,
  };
}

function project(name: string, over: Partial<Project> = {}): Project {
  return {
    id: name,
    name,
    root: `/Users/ana/Herd/${name}`,
    hosts: [`${name}.test`],
    kind: "laravel",
    origin: "herd_parked",
    php_version: null,
    log_tz: "UTC",
    ...over,
  };
}

function job(over: Partial<JobInfo> = {}): JobInfo {
  return {
    id: "job-12345678",
    kind: "component_install",
    args: { id: "redis" },
    client: "claude-code 2.1.318",
    state: "running",
    progress: null,
    message: null,
    started_at_ms: 0,
    finished_at_ms: null,
    ...over,
  };
}

function snapshot(over: Partial<Snapshot> = {}): Snapshot {
  return {
    revision: 1,
    refreshed_at_ms: 0,
    components: [],
    managed: [],
    php: { versions: [], active: null },
    projects: [],
    busy: [],
    problems: [],
    jobs: [],
    logs: { available: false, unseen_errors: 0, recent: [] },
    mcp: { linked: false, last_used_ms: null, mode: "confirm" },
    mailpit: { installed: false, running: false, url: "http://127.0.0.1:8025", messages: null },
    setup_needed: false,
    settings: {
      keep_in_menu_bar: true,
      appearance: "system",
      language: "system",
      show_php_in_menu_bar: false,
      watch_services: true,
      editor: "vscode",
      php_source: null,
      mcp: { mode: "confirm" },
    },
    locale: "es",
    ...over,
  };
}

const MB = 1024;
/** Espacio duro que pone format.ts entre la cifra y la unidad. */
const NB = "\u00a0";

describe("revisiones", () => {
  afterEach(resetApp);

  it("solo acepta snapshots más recientes", () => {
    expect(isNewer(null, snapshot({ revision: 0 }))).toBe(true);
    expect(isNewer(snapshot({ revision: 3 }), snapshot({ revision: 4 }))).toBe(true);
    expect(isNewer(snapshot({ revision: 3 }), snapshot({ revision: 3 }))).toBe(false);
    expect(isNewer(snapshot({ revision: 3 }), snapshot({ revision: 2 }))).toBe(false);
  });

  it("ignora un snapshot antiguo que llega tarde", () => {
    expect(applySnapshot(snapshot({ revision: 5 }))).toBe(true);
    expect(applySnapshot(snapshot({ revision: 4, setup_needed: true }))).toBe(false);
    expect(app.snapshot?.revision).toBe(5);
    expect(app.snapshot?.setup_needed).toBe(false);
  });
});

describe("servicios y herramientas", () => {
  const s = snapshot({
    components: [
      component("mysql"),
      component("redis", { service: service("redis", { running: true, pid: 20 }) }),
      component("nginx", { service: service("nginx", { running: true, pid: 10, registered: true }) }),
      component("postgresql", { installed: false, service: null }),
      component("php", { kind: "tool", service: null }),
      component("composer", { kind: "tool", service: null, version: "2.8.1", origin: "Elephant Stack" }),
    ],
    php: { versions: [{ key: "8.4@Herd", id: "8.4.1", source: "Herd" }], active: null },
  });

  it("pone primero los que están en marcha y conserva el orden del resto", () => {
    expect(services(s).map((c) => c.id)).toEqual(["redis", "nginx", "mysql"]);
    expect(runningPids(s)).toEqual([20, 10]);
    const own = snapshot({
      ...s,
      managed: [
        {
          service: "mysql",
          name: "MySQL",
          port: 3306,
          web_port: null,
          versions: [],
          branches: [
            {
              branch: "8.4",
              version: "8.4.11",
              label: "sh.elephant.mysql.8.4",
              state: "running",
              pid: 30,
              autostart: false,
              error: null,
              data_dir: "/d",
              data_ready: true,
              log_path: "/l",
            },
          ],
          import: null,
        },
      ],
    });
    expect(runningPids(own)).toEqual([20, 10, 30]);
    expect(managedCounts(own)).toEqual({ installed: 1, running: 1 });
    expect(targetName(own, "sh.elephant.mysql.8.4")).toBe("MySQL 8.4");
  });

  it("deja PHP fuera de las herramientas si tiene versiones propias", () => {
    expect(tools(s).map((c) => c.id)).toEqual(["composer"]);
    expect(tools(snapshot({ components: s.components })).map((c) => c.id)).toEqual(["php", "composer"]);
    expect(missing(s).map((c) => c.id)).toEqual(["postgresql"]);
  });

  it("describe el estado como hasta ahora", () => {
    const usage = { pid: 10, memory_kb: 84 * MB, cpu_percent: 12.4, processes: 3 };
    const running = service("nginx", { running: true, pid: 10, registered: true });
    expect(serviceStatusText(running, usage)).toBe(`En marcha · 84${NB}MB · 12${NB}% CPU · Al iniciar sesión`);
    expect(serviceStatusText(running, { ...usage, cpu_percent: 2 })).toBe(`En marcha · 84${NB}MB · Al iniciar sesión`);
    expect(serviceStatusText(service("mysql", { status: "error", exit_code: 1 }))).toBe("Error · código 1");
    expect(serviceStatusText(service("mysql", { status: "error" }))).toBe("Error · código desconocido");
    expect(serviceStatusText(service("redis"))).toBe("Parado");
  });

  it("solo nombra el origen si no es Homebrew", () => {
    expect(installedText(component("a", { version: "1.2" }))).toBe("1.2");
    expect(installedText(component("a", { version: "2.8", origin: "Elephant Stack" }))).toBe("2.8 · Elephant Stack");
    expect(installedText(component("a"))).toBe("Instalado");
  });

  it("bloquea los controles de Homebrew salvo si lo ocupado es PHP", () => {
    expect(brewBusy(snapshot({ busy: [{ target: "php", label: "Cambiando…" }] }))).toBe(false);
    expect(brewBusy(snapshot({ busy: [{ target: "mysql", label: "Arrancando…" }] }))).toBe(true);
    expect(brewBusy(snapshot({ busy: [{ target: "services", label: "Reiniciando todos…" }] }))).toBe(true);
  });
});

describe("tareas de agentes", () => {
  it("traduce el cliente", () => {
    expect(clientName("claude-code 2.1.318")).toBe("Claude Code");
    expect(clientName("mcp:cursor 1.2")).toBe("Cursor");
    expect(clientName("tty")).toBe("la terminal");
    expect(clientName("script")).toBe("un script");
    expect(clientName(null)).toBeNull();
  });

  it("asocia una instalación a su componente", () => {
    const s = snapshot({ jobs: [job(), job({ id: "job-2", kind: "project_create", args: { name: "shop" } })] });
    expect(jobForComponent(s, "redis")?.id).toBe("job-12345678");
    expect(jobForComponent(s, "mysql")).toBeUndefined();
  });

  it("describe la tarea y quién la pidió", () => {
    expect(jobText(job())).toBe("Instalando redis… · pedido por Claude Code");
    expect(jobText(job({ message: "Descargando…", client: null }))).toBe("Descargando…");
    expect(jobText(job({ kind: "project_create", args: { name: "shop" }, client: "tty" }))).toBe(
      "Creando shop… · pedido por la terminal",
    );
  });
});

describe("PHP", () => {
  const versions = [
    { key: "8.4.27@Elephant Stack", id: "8.4.27", source: "Elephant Stack" },
    { key: "8.4.27@Herd", id: "8.4.27", source: "Herd" },
    { key: "8.3.14@Herd", id: "8.3.14", source: "Herd" },
  ];

  it("distingue por origen las versiones repetidas", () => {
    expect(phpOptions(versions).map((o) => o.label)).toEqual([
      "8.4.27 · Elephant Stack",
      "8.4.27 · Herd",
      "8.3.14",
    ]);
  });

  it("encuentra la versión de un .php-version por versión exacta o por rama", () => {
    expect(versionForProject(versions, "8.3")?.key).toBe("8.3.14@Herd");
    expect(versionForProject(versions, "8.4.27")?.key).toBe("8.4.27@Elephant Stack");
    expect(versionForProject(versions, "8.2")).toBeUndefined();
    expect(versionForProject(versions, "8")?.key).toBe("8.4.27@Elephant Stack");
    expect(versionForProject(versions, "8.41")).toBeUndefined();
    expect(versionForProject(versions, null)).toBeUndefined();
  });

  it("resume la versión por defecto como rama", () => {
    expect(phpBranch(snapshot({ php: { versions, active: versions[0] } }))).toBe("8.4");
    expect(phpBranch(snapshot())).toBeNull();
  });
});

describe("sitios y rutas", () => {
  it("solo cuenta como sitio lo que tiene dirección web", () => {
    const s = snapshot({ projects: [project("acme"), project("cli", { hosts: [] })] });
    expect(sites(s).map((p) => p.name)).toEqual(["acme"]);
  });

  it("acorta el directorio personal", () => {
    expect(tildePath("/Users/ana/Herd/acme")).toBe("~/Herd/acme");
    expect(tildePath("/Users/ana")).toBe("~");
    expect(tildePath("/opt/sites/acme")).toBe("/opt/sites/acme");
  });

  it("recorta por el centro sin pasarse del máximo", () => {
    expect(middleTruncate("corto", 10)).toBe("corto");
    const cut = middleTruncate("~/Code/clientes/acme/tienda-online", 15);
    expect(cut).toHaveLength(15);
    expect(cut.startsWith("~/Code/")).toBe(true);
    expect(cut.endsWith("online")).toBe(true);
    expect(cut).toContain("…");
  });
});

describe("héroe del Panel", () => {
  const running = component("nginx", { service: service("nginx", { running: true, pid: 1 }) });
  const stopped = component("mysql", { service: service("mysql", { registered: true }) });

  it("con todo en marcha no ofrece ninguna acción", () => {
    const h = hero(snapshot({ components: [running] }), 412 * MB);
    expect(h).toMatchObject({ tone: "ok", title: "Todo en marcha", action: null });
    expect(h.detail).toBe(`1 servicio · 412${NB}MB`);
  });

  it("un problema manda sobre lo demás", () => {
    const h = hero(
      snapshot({
        components: [running],
        busy: [{ target: "redis", label: "Arrancando…" }],
        problems: [
          { id: "service_error:mysql", target: "mysql", message: "mysql se ha detenido (código 1)", kind: "service_error" },
        ],
      }),
      0,
    );
    expect(h).toMatchObject({ tone: "bad", title: "mysql se ha detenido (código 1)" });
    expect(h.action?.kind).toBe("problem");
  });

  it("resume un fallo de acción en vez de enseñar la salida cruda de la orden", () => {
    const h = hero(
      snapshot({
        components: [running],
        problems: [
          {
            id: "action_failed:mysql",
            target: "mysql",
            message: "brew services start mysql falló: Error: Failure while executing; `/bin/launchctl bootstrap gui/501 …` exited with 5.",
            kind: "action_failed",
          },
        ],
      }),
      0,
    );
    expect(h).toMatchObject({ tone: "bad", title: "No se pudo completar la acción con mysql" });
    expect(h.action?.label).toBe("Ver problema");
  });

  it("avisa de lo que está ocupado", () => {
    expect(hero(snapshot({ busy: [{ target: "redis", label: "Arrancando…" }] }), 0)).toMatchObject({
      tone: "warn",
      title: "redis: arrancando…",
    });
    expect(hero(snapshot({ jobs: [job()] }), 0).title).toBe("Instalando redis… · pedido por Claude Code");
  });

  it("nombra el proyecto ocupado, no su id", () => {
    const s = snapshot({
      projects: [project("tienda", { id: "0123456789abcdef" })],
      busy: [{ target: "0123456789abcdef", label: "Arrancando lo que necesita…" }],
    });
    expect(targetName(s, "0123456789abcdef")).toBe("tienda");
    expect(targetName(s, "redis")).toBe("redis");
    expect(hero(s, 0).title).toBe("tienda: arrancando lo que necesita…");
  });

  it("sin nada en marcha propone arrancar lo que arranca al iniciar sesión", () => {
    const s = snapshot({ components: [stopped], php: { versions: [], active: { key: "k", id: "8.4.1", source: "Herd" } } });
    expect(startableServices(s)).toEqual(["mysql"]);
    expect(hero(s, 0)).toMatchObject({
      title: "Nada en marcha",
      detail: "Ningún servicio en marcha · PHP 8.4",
      action: { kind: "start" },
    });
    const plain = snapshot({ components: [component("redis")] });
    expect(hero(plain, 0).action?.kind).toBe("services");
    expect(hero(snapshot(), 0)).toMatchObject({ detail: "Sin servicios", action: null });
  });

  it("escala el anillo de RAM por escalones", () => {
    expect(gaugeMaxKb(0)).toBe(512 * MB);
    expect(gaugeMaxKb(412 * MB)).toBe(512 * MB);
    expect(gaugeMaxKb(700 * MB)).toBe(1024 * MB);
    expect(gaugeMaxKb(3000 * MB)).toBe(4096 * MB);
    expect(gaugeMaxKb(40000 * MB)).toBe(49152 * MB);
  });
});

describe("problemas", () => {
  const s = snapshot({
    projects: [project("tienda", { id: "0123456789abcdef" })],
    problems: [
      { id: "action_failed:php", target: "php", message: "x", kind: "action_failed" },
      { id: "action_failed:0123456789abcdef", target: "0123456789abcdef", message: "x", kind: "action_failed" },
      { id: "action_failed:redis", target: "redis", message: "x", kind: "action_failed" },
      { id: "service_error:mysql", target: "mysql", message: "x", kind: "service_error" },
    ],
  });

  it("cada problema lleva a la vista de su objetivo, como en el menú de la barra", () => {
    expect(problemRoute(s, "php")).toBe("#/php");
    expect(problemRoute(s, "0123456789abcdef")).toBe("#/sitios");
    expect(problemRoute(s, "redis")).toBe("#/servicios");
  });

  it("al abrir una vista se quitan sus fallos de acciones, no los errores de servicio", () => {
    expect(problemsClearedBy(s, "servicios")).toEqual(["redis"]);
    expect(problemsClearedBy(s, "sitios")).toEqual(["0123456789abcdef"]);
    expect(problemsClearedBy(s, "php")).toEqual(["php"]);
    expect(problemsClearedBy(s, "panel")).toEqual([]);
  });
});
