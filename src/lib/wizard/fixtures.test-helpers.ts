// Datos sintéticos para los tests del asistente. No entra en el build: solo lo
// importan los *.test.ts.

import type { Catalog } from "../bindings/Catalog";
import type { Detection } from "../bindings/Detection";
import type { Plan } from "../bindings/Plan";
import type { Profile } from "../bindings/Profile";
import type { ScenarioCard } from "../bindings/ScenarioCard";
import type { Step } from "../bindings/Step";

export function step(key: string, over: Partial<Step> = {}): Step {
  return {
    key,
    action: { kind: "install_shim" },
    group: "configure",
    title: `Paso ${key}`,
    command: null,
    optional: false,
    after: [],
    restarts_service: false,
    since: 1,
    ...over,
  };
}

export function plan(steps: Step[] = [], over: Partial<Plan> = {}): Plan {
  return {
    id: "plan-1",
    scenario: "laravel",
    steps,
    satisfied: [],
    notices: [],
    conflicts: [],
    totals: { download_bytes: 0, brew_formulas: 0, idle_ram_mb: 0 },
    ...over,
  };
}

export function profile(over: Partial<Profile> = {}): Profile {
  return {
    scenario: "laravel",
    php: { kind: "any_own" },
    composer: true,
    shim: true,
    services: { mailpit: "installed" },
    launch_at_login: true,
    default_php: null,
    ...over,
  };
}

export function detection(over: Partial<Detection> = {}): Detection {
  return {
    arch_arm64: true,
    formulas: [],
    services: [],
    php_versions: [],
    global_php: null,
    shim: "missing",
    composer_phar: false,
    path: "missing",
    ports: [],
    herd: null,
    launch_at_login: false,
    data_dirs: [],
    ...over,
  };
}

export function catalog(versions: string[] = ["8.5.12", "8.4.27", "8.4.20", "8.3.36"]): Catalog {
  return {
    php: versions.map((version) => ({
      version,
      url: `https://example.invalid/php-${version}.tar.gz`,
      size: 65_000_000,
      checksum_url: `https://example.invalid/php-${version}.tar.gz.sha256`,
    })),
    composer: null,
    offline: false,
  };
}

export const CARDS: ScenarioCard[] = [
  {
    id: "laravel",
    name: "Laravel",
    phrase: "Proyectos con SQLite. Nada en marcha.",
    recommended: true,
    components: ["PHP", "Composer", "Mailpit"],
    idle_ram_mb: 0,
    download_bytes: 0,
    steps: 2,
    databases: [],
  },
  {
    id: "laravel_services",
    name: "Laravel con servicios",
    phrase: "MySQL o PostgreSQL, Redis y correo de pruebas.",
    recommended: false,
    components: ["PHP", "Composer", "Redis", "Mailpit"],
    idle_ram_mb: 25,
    download_bytes: 0,
    steps: 4,
    databases: [
      { database: "mysql", label: "MySQL", idle_ram_mb: 400, preselected: false },
      { database: "postgres", label: "PostgreSQL", idle_ram_mb: 20, preselected: true },
    ],
  },
];
