// Datos de ejemplo de las capturas de referencia (docs/capturas): un Mac con Herd,
// tres proyectos y los servicios de «Laravel con servicios». Solo los usa el arnés
// de src/dev/capturas.ts, que no entra en el build.

import { entry, frame, summary } from "../lib/logs/fixtures";
import { CARDS, detection } from "../lib/wizard/fixtures.test-helpers";
import type { Catalog } from "../lib/bindings/Catalog";
import type { BranchStatus } from "../lib/bindings/BranchStatus";
import type { Component } from "../lib/bindings/Component";
import type { EntryDetail } from "../lib/bindings/EntryDetail";
import type { Locale } from "../lib/bindings/Locale";
import type { LogEntry } from "../lib/bindings/LogEntry";
import type { McpSnippets } from "../lib/bindings/McpSnippets";
import type { ManagedService } from "../lib/bindings/ManagedService";
import type { McpStatus } from "../lib/bindings/McpStatus";
import type { Project } from "../lib/bindings/Project";
import type { ProjectSummary } from "../lib/bindings/ProjectSummary";
import type { Service } from "../lib/bindings/Service";
import type { ServiceRelease } from "../lib/bindings/ServiceRelease";
import type { ServicesCatalog } from "../lib/bindings/ServicesCatalog";
import type { Snapshot } from "../lib/bindings/Snapshot";
import type { ToolStatus } from "../lib/bindings/ToolStatus";
import type { Usage } from "../lib/bindings/Usage";

const NOW = Date.now();

/** Idioma de las capturas: `?idioma=en` (scripts/capturas.sh); por defecto, español. */
export const locale: Locale = new URLSearchParams(location.search).get("idioma") === "en" ? "en" : "es";
const MIN = 60_000;

/** Lo que escribe Rust en el idioma de la app: aquí, en el de la captura. */
const rust = (es: string, en: string) => (locale === "en" ? en : es);

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
    description: rust("Base de datos PostgreSQL", "PostgreSQL database"),
    version: "18.1",
    service: service("postgresql@18", true, 812),
  }),
  component("redis", "service", { description: rust("Caché y colas", "Cache and queues"), version: "8.2.2", service: service("redis", true, 845) }),
  component("mailpit", "service", {
    description: rust("Correo de pruebas", "Test mail"),
    version: "1.27.10",
    service: service("mailpit", false, null, false),
  }),
  component("mysql", "service", {
    description: rust("Base de datos MySQL", "MySQL database"),
    version: "9.5.0",
    service: service("mysql", false, null, false),
  }),
  component("php", "tool", { version: "8.4.27", origin: "Elephant" }),
  component("composer", "tool", { version: "2.8.12", origin: "Elephant" }),
  component("node", "tool", { version: "24.11.0", origin: "nvm" }),
  component("meilisearch", "service", { description: rust("Búsqueda", "Search"), installed: false }),
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

const ES = "/Users/ana/.elephant-stack";

function branch(b: string, version: string, over: Partial<BranchStatus> = {}, id = "mysql"): BranchStatus {
  return {
    branch: b,
    version,
    label: `sh.elephant.${id}.${b}`,
    state: "stopped",
    pid: null,
    autostart: false,
    error: null,
    data_dir: `${ES}/data/${id}/${b}`,
    data_ready: true,
    log_path: `${ES}/logs/${id}-${b}.err`,
    ...over,
  };
}

/**
 * Servicios propios: MySQL 8.4 en marcha (con un parche nuevo), 9.7 instalada y
 * los datos de brew de 9.7; PostgreSQL 18 parado, con los datos de brew en uso;
 * Redis 8.2 en error porque el de brew ocupa el puerto; Mailpit en marcha.
 */
const managed: ManagedService[] = [
  {
    service: "mysql",
    name: "MySQL",
    port: 3306,
    web_port: null,
    versions: [
      { version: "9.7.1", branch: "9.7", path: `${ES}/services/mysql/9.7.1` },
      { version: "8.4.11", branch: "8.4", path: `${ES}/services/mysql/8.4.11` },
    ],
    branches: [
      branch("9.7", "9.7.1", { data_ready: false }),
      branch("8.4", "8.4.11", { state: "running", pid: 901, autostart: true }),
    ],
    import: {
      source: "/opt/homebrew/var/mysql",
      version: "9.7.1",
      branch: "9.7",
      size: 429_000_000,
      blocked: null,
      manual: null,
      replaces: false,
    },
  },
  {
    service: "postgresql",
    name: "PostgreSQL",
    port: 5432,
    web_port: null,
    versions: [{ version: "18.6.1", branch: "18", path: `${ES}/services/postgresql/18.6.1` }],
    branches: [branch("18", "18.6.1", {}, "postgresql")],
    import: {
      source: "/opt/homebrew/var/postgresql@18",
      version: "18",
      branch: "18",
      size: 61_400_000,
      blocked: rust(
        "El PostgreSQL de Homebrew está usando estos datos: detenlo con «brew services stop postgresql@18» antes de copiarlos.",
        "Homebrew’s PostgreSQL is using this data. Stop it with “brew services stop postgresql@18” before copying.",
      ),
      manual: null,
      replaces: true,
    },
  },
  {
    service: "redis",
    name: "Redis",
    port: 6379,
    web_port: null,
    versions: [{ version: "8.2.11", branch: "8.2", path: `${ES}/services/redis/8.2.11` }],
    branches: [
      branch(
        "8.2",
        "8.2.11",
        {
          state: "error",
          error: rust(
            "El puerto 6379 lo usa Redis de Homebrew. Para encender Redis de Elephant, detenlo con «brew services stop redis» (sus datos no se tocan) y vuelve a intentarlo.",
            "Port 6379 is used by Redis from Homebrew. To turn on Elephant’s Redis, stop it with “brew services stop redis” (its data isn’t touched) and try again.",
          ),
        },
        "redis",
      ),
    ],
    import: null,
  },
  {
    service: "mailpit",
    name: "Mailpit",
    port: 1025,
    web_port: 8025,
    versions: [{ version: "1.33.0", branch: "1", path: `${ES}/services/mailpit/1.33.0` }],
    branches: [
      branch("1", "1.33.0", { state: "running", pid: 903, autostart: true, data_dir: `${ES}/data/mailpit` }, "mailpit"),
    ],
    import: null,
  },
];

/** phpMyAdmin instalado y en marcha con el PHP 8.3 de Elephant; pgAdmin sin instalar. */
const managedTools: ToolStatus[] = [
  {
    tool: "phpmyadmin",
    name: "phpMyAdmin",
    version: "5.2.3",
    path: `${ES}/tools/phpmyadmin/5.2.3`,
    external: null,
    target: "sh.elephant.phpmyadmin",
    running: true,
    pid: 950,
    url: "http://127.0.0.1:8081/",
    php: "8.3.36",
    php_range: ">=7.2,<8.4",
    warning: null,
    blocked: false,
  },
  {
    tool: "pgadmin",
    name: "pgAdmin",
    version: null,
    path: null,
    external: null,
    target: "sh.elephant.pgadmin",
    running: false,
    pid: null,
    url: null,
    php: null,
    php_range: null,
    warning: null,
    blocked: false,
  },
];

function serviceRelease(service: ServiceRelease["service"], version: string, branchName: string, url: string, size: number, lts: boolean): ServiceRelease {
  return { service, version, branch: branchName, url, sha256: "b96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009", size, lts };
}

function mysqlRelease(version: string, size: number, lts: boolean): ServiceRelease {
  const branchName = version.split(".").slice(0, 2).join(".");
  return {
    service: "mysql",
    version,
    branch: branchName,
    url: `https://cdn.mysql.com/Downloads/MySQL-${branchName}/mysql-${version}-macos15-arm64.tar.gz`,
    sha256: "b96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009",
    size,
    lts,
  };
}

export const servicesCatalog: ServicesCatalog = {
  releases: [
    mysqlRelease("26.10.0", 192_417_435, false),
    mysqlRelease("9.7.1", 188_000_000, true),
    mysqlRelease("8.4.12", 168_100_000, true),
    mysqlRelease("8.4.11", 167_977_240, true),
    serviceRelease("postgresql", "18.6.1", "18", "https://github.com/theseus-rs/postgresql-binaries/releases/download/18.6.1/postgresql-18.6.1-aarch64-apple-darwin.tar.gz", 88_814_625, true),
    serviceRelease("postgresql", "17.11.1", "17", "https://github.com/theseus-rs/postgresql-binaries/releases/download/17.11.1/postgresql-17.11.1-aarch64-apple-darwin.tar.gz", 90_030_572, true),
    serviceRelease("redis", "8.10.3", "8.10", "https://github.com/cristiancosano/php-builds/releases/download/redis-8.10.3/redis-8.10.3-macos-arm64.tar.gz", 1_493_851, false),
    serviceRelease("redis", "8.2.11", "8.2", "https://github.com/cristiancosano/php-builds/releases/download/redis-8.2.11/redis-8.2.11-macos-arm64.tar.gz", 1_299_441, false),
    serviceRelease("mailpit", "1.33.0", "1", "https://github.com/axllent/mailpit/releases/download/v1.33.0/mailpit-darwin-arm64.tar.gz", 10_406_800, false),
  ],
  tools: [
    {
      tool: "phpmyadmin",
      version: "5.2.3",
      url: "https://files.phpmyadmin.net/phpMyAdmin/5.2.3/phpMyAdmin-5.2.3-all-languages.zip",
      sha256: "2d2e13c735366d318425c78e4ee2cc8fc648d77faba3ddea2cd516e43885733f",
      size: 16_431_330,
      php: ">=7.2,<8.4",
    },
    {
      tool: "pgadmin",
      version: "9.19",
      url: "https://ftp.postgresql.org/pub/pgadmin/pgadmin4/v9.19/macos/pgadmin4-9.19-arm64.dmg",
      sha256: "8827d9444752497602f15cb01bd4014378510a2fdd118902686da3f36713b8ed",
      size: 234_409_999,
      php: null,
    },
  ],
  offline: false,
};

export const snapshot: Snapshot = {
  revision: 1,
  refreshed_at_ms: NOW,
  components,
  managed,
  managed_tools: managedTools,
  php: {
    versions: [
      { key: "own:8.4.27", id: "8.4.27", source: "Elephant" },
      { key: "own:8.3.36", id: "8.3.36", source: "Elephant" },
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
    language: locale === "en" ? "en" : "system",
    show_php_in_menu_bar: false,
    watch_services: true,
    editor: "vscode",
    php_source: null,
    mcp: { mode: "confirm" },
  },
  locale,
};

export function usage(pids: number[]): Usage[] {
  const table: Record<number, [number, number]> = {
    812: [96_000, 0.4],
    845: [14_200, 0.2],
    873: [31_500, 6.1],
    901: [412_000, 0.3],
  };
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

const CARD_TEXT_EN: Record<string, { name: string; phrase: string }> = {
  laravel: { name: "Laravel", phrase: "Projects with SQLite. Nothing running." },
  laravel_services: { name: "Laravel with services", phrase: "MySQL or PostgreSQL, Redis and test mail." },
  terminal: { name: "Terminal only", phrase: "PHP and Composer in the terminal. Herd keeps serving your sites." },
  custom: { name: "Custom", phrase: "Pick each piece. Starts from what you already have." },
};

export const cards = locale === "en" ? CARDS.map((c) => ({ ...c, ...CARD_TEXT_EN[c.id] })) : CARDS;
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

// Catálogo de php-builds: la 8.5.12 aparece como no instalada, la 8.4.28 como
// actualización de la 8.4.27 y la 8.3.36 se funde con la instalada.
export const catalog: Catalog = {
  php: [release("8.5.12", 67_413_472), release("8.4.28", 66_600_000), release("8.4.27", 66_592_142), release("8.3.36", 64_000_000)],
  composer: null,
  offline: false,
};
