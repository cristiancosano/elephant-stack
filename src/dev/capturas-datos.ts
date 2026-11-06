// Datos de ejemplo de las capturas de referencia (docs/capturas): un Mac con Herd,
// tres proyectos y los servicios de «Laravel con servicios». Solo los usa el arnés
// de src/dev/capturas.ts, que no entra en el build.

import { entry, frame, summary } from "../lib/logs/fixtures";
import { CARDS, detection } from "../lib/wizard/fixtures.test-helpers";
import type { Catalog } from "../lib/bindings/Catalog";
import type { Component } from "../lib/bindings/Component";
import type { EntryDetail } from "../lib/bindings/EntryDetail";
import type { LogEntry } from "../lib/bindings/LogEntry";
import type { McpSnippets } from "../lib/bindings/McpSnippets";
import type { McpStatus } from "../lib/bindings/McpStatus";
import type { Project } from "../lib/bindings/Project";
import type { ProjectSummary } from "../lib/bindings/ProjectSummary";
import type { Service } from "../lib/bindings/Service";
import type { Snapshot } from "../lib/bindings/Snapshot";
import type { Usage } from "../lib/bindings/Usage";

const NOW = Date.now();
const MIN = 60_000;

function service(name: string, running: boolean, pid: number | null, registered = true): Service {
  return { name, status: running ? "started" : "none", running, registered, pid, exit_code: null };
}

function component(id: string, kind: "service" | "tool", over: Partial<Component> = {}): Component {
  return {
    id,
    kind,
    description: "",
    installed: true,
    version: null,
    origin: "Homebrew",
    path: null,
    service: null,
    ...over,
  };
}

const components: Component[] = [
  component("postgresql@18", "service", {
    description: "Base de datos PostgreSQL",
    version: "18.1",
    service: service("postgresql@18", true, 812),
  }),
  component("redis", "service", { description: "Caché y colas", version: "8.2.2", service: service("redis", true, 845) }),
  component("mailpit", "service", {
    description: "Correo de pruebas",
    version: "1.27.10",
    service: service("mailpit", true, 873),
  }),
  component("mysql", "service", {
    description: "Base de datos MySQL",
    version: "9.5.0",
    service: service("mysql", false, null, false),
  }),
  component("php", "tool", { version: "8.4.27", origin: "Elephant" }),
  component("composer", "tool", { version: "2.8.12", origin: "Elephant" }),
  component("node", "tool", { version: "24.11.0", origin: "nvm" }),
  component("meilisearch", "service", { description: "Búsqueda", installed: false }),
];

const projects: Project[] = [
  {
    id: "a1b2c3d4e5f60718",
    name: "shop",
    root: "/Users/ana/Herd/shop",
    hosts: ["shop.test"],
    kind: "laravel",
    origin: "herd_parked",
    php_version: "8.4",
    log_tz: "Europe/Madrid",
  },
  {
    id: "0f1e2d3c4b5a6978",
    name: "blog",
    root: "/Users/ana/Herd/blog",
    hosts: ["blog.test"],
    kind: "laravel",
    origin: "herd_parked",
    php_version: null,
    log_tz: "UTC",
  },
  {
    id: "9a8b7c6d5e4f3021",
    name: "api-pagos",
    root: "/Users/ana/Code/api-pagos",
    hosts: [],
    kind: "php",
    origin: "user",
    php_version: "8.3",
    log_tz: "Europe/Madrid",
  },
];

export const snapshot: Snapshot = {
  revision: 1,
  refreshed_at_ms: NOW,
  components,
  php: {
    versions: [
      { key: "own:8.4.27", id: "8.4.27", source: "Elephant" },
      { key: "own:8.3.36", id: "8.3.36", source: "Elephant" },
      { key: "herd:8.4", id: "8.4.20", source: "Herd" },
    ],
    active: { key: "own:8.4.27", id: "8.4.27", source: "Elephant" },
  },
  projects,
  busy: [],
  problems: [],
  jobs: [],
  logs: {
    available: true,
    unseen_errors: 3,
    recent: [{ id: projects[0].id, name: "shop", errors: 3, last_ms: NOW - 4 * MIN }],
  },
  mcp: { linked: true, last_used_ms: NOW - 12 * MIN, mode: "confirm" },
  mailpit: { installed: true, running: true, url: "http://127.0.0.1:8025", messages: 4 },
  setup_needed: false,
  settings: {
    keep_in_menu_bar: true,
    appearance: "system",
    show_php_in_menu_bar: false,
    watch_services: true,
    editor: "vscode",
    mcp: { mode: "confirm" },
  },
};

export function usage(pids: number[]): Usage[] {
  const table: Record<number, [number, number]> = { 812: [96_000, 0.4], 845: [14_200, 0.2], 873: [31_500, 6.1] };
  return pids.map((pid) => {
    const [memory_kb, cpu_percent] = table[pid] ?? [10_000, 0];
    return { pid, memory_kb, cpu_percent, processes: pid === 812 ? 7 : 1 };
  });
}

export const summaries: ProjectSummary[] = projects.map((p, i) =>
  summary(p, {
    errors_24h: [3, 0, 1][i],
    last_ms: NOW - [4, 90, 300][i] * MIN,
    sources: [
      {
        source: {
          id: `laravel_app:${p.id}`,
          kind: "laravel_app",
          project: p.id,
          dir: `${p.root}/storage/logs`,
          pattern: "laravel*.log",
        },
        files: ["laravel.log"],
        state: { kind: "ok" },
      },
    ],
  }),
);

const shop = projects[0].id;

export const entries: LogEntry[] = [
  entry({
    id: "00000000000000ab-9f20",
    project: shop,
    ts_ms: NOW - 4 * MIN,
    level: "error",
    message: "SQLSTATE[42P01]: Undefined table: relation \"cupones\" does not exist",
    exception: {
      class: "Illuminate\\Database\\QueryException",
      message: "SQLSTATE[42P01]: Undefined table: relation \"cupones\" does not exist",
      file: "/Users/ana/Herd/shop/app/Http/Controllers/CarritoController.php",
      line: 42,
    },
    fingerprint: "c0ffee0000000001",
  }),
  entry({
    id: "00000000000000cd-1a2b",
    kind: "nginx_access",
    project: shop,
    host: "shop.test",
    ts_ms: NOW - 4 * MIN - 800,
    level: "error",
    channel: null,
    message: "GET /carrito 500",
    http: {
      method: "GET",
      path: "/carrito",
      status: 500,
      bytes: 6_120,
      duration_ms: 182,
      upstream_ms: 180,
      ip: "127.0.0.1",
      ua: null,
    },
    fingerprint: "c0ffee0000000002",
  }),
  entry({
    id: "00000000000000ab-9e10",
    project: shop,
    ts_ms: NOW - 9 * MIN,
    level: "warning",
    message: "Pasarela de pago lenta: 2.300 ms",
    fingerprint: "c0ffee0000000003",
  }),
  entry({
    id: "00000000000000cd-1900",
    kind: "nginx_access",
    project: shop,
    host: "shop.test",
    ts_ms: NOW - 11 * MIN,
    level: "info",
    channel: null,
    message: "POST /login 302",
    http: {
      method: "POST",
      path: "/login",
      status: 302,
      bytes: 0,
      duration_ms: 64,
      upstream_ms: 61,
      ip: "127.0.0.1",
      ua: null,
    },
    fingerprint: "c0ffee0000000004",
  }),
  entry({
    id: "00000000000000ab-9c00",
    project: shop,
    ts_ms: NOW - 15 * MIN,
    level: "info",
    message: "Pedido 1042 confirmado",
    fingerprint: "c0ffee0000000005",
  }),
  entry({
    id: "00000000000000ef-0400",
    kind: "php_fpm",
    project: null,
    ts_ms: NOW - 32 * MIN,
    level: "notice",
    channel: null,
    message: "[pool www] child 4121 exited with code 0 after 3600.0 seconds from start",
    fingerprint: "c0ffee0000000006",
  }),
];

export function detail(id: string): EntryDetail {
  const e = entries.find((x) => x.id === id) ?? entries[0];
  return {
    entry: e,
    body: e.exception ? `${e.message}\n#0 app/Http/Controllers/CarritoController.php(42)` : e.message,
    frames: e.exception
      ? [
          frame(0, "/Users/ana/Herd/shop/app/Http/Controllers/CarritoController.php"),
          frame(1, "/Users/ana/Herd/shop/vendor/laravel/framework/src/Illuminate/Routing/Controller.php", true, true),
        ]
      : [],
    context: e.exception ? { userId: 7, cupon: "OTOÑO" } : null,
  };
}

export const mcpStatus: McpStatus = {
  linked: true,
  link_path: "/Users/ana/.elephant-stack/bin/elephant",
  target: "/Applications/Elephant Stack.app/Contents/MacOS/cli",
  mode: "confirm",
  claude_path: "/Users/ana/.local/bin/claude",
  claude_registered: true,
  last_used_ms: NOW - 12 * MIN,
};

export const mcpSnippets: McpSnippets = {
  project_json: '{ "mcpServers": { "elephant": { "command": "/Users/ana/.elephant-stack/bin/elephant", "args": ["mcp"] } } }',
  cursor: "",
  claude_desktop: "",
  vscode: "",
  claude_command:
    "claude plugin marketplace add ~/.elephant-stack/share/claude-plugin\nclaude plugin install elephant@elephant-stack -s user",
  shell_guide: "elephant status · elephant logs errors --since 1h · elephant php use 8.3",
};

export const cards = CARDS;
export const wizardDetection = detection({
  formulas: ["postgresql@18", "redis", "mailpit", "mysql"],
  global_php: "8.4.27",
  shim: "same",
  path: "active",
  herd: { running: true, tld: "test", sites: 2 },
  launch_at_login: true,
});

const release = (version: string, size: number) => ({
  version,
  url: `https://github.com/cristiancosano/php-builds/releases/download/php-${version}/php-${version}-macos-arm64.tar.gz`,
  size,
  checksum_url: `https://github.com/cristiancosano/php-builds/releases/download/php-${version}/php-${version}-macos-arm64.tar.gz.sha256`,
});

// Catálogo de php-builds: la 8.5.12 aparece como no instalada y la 8.4.27 y la
// 8.3.36 se funden con las instaladas.
export const catalog: Catalog = {
  php: [release("8.5.12", 67_413_472), release("8.4.27", 66_592_142), release("8.3.36", 64_000_000)],
  composer: null,
  offline: false,
};
