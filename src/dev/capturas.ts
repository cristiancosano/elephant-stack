// Arnés de las capturas de referencia (docs/capturas, 01 §12). Solo en `pnpm dev`:
// http://localhost:1420/src/dev/capturas.html?tema=claro#/panel (no entra en el
// build). Monta la app de verdad con el IPC de Tauri simulado y los datos de
// capturas-datos.ts; lo usa scripts/capturas.sh con Chrome sin interfaz.
import { mockIPC } from "@tauri-apps/api/mocks";
import * as datos from "./capturas-datos";

const dark = new URLSearchParams(location.search).get("tema") === "oscuro";
const root = document.documentElement;
root.dataset.theme = dark ? "dark" : "light";
// Sin el material nativo, la barra lateral usa su color de respaldo.
root.classList.add("no-vibrancy");
root.style.background = "var(--window)";

const NOT_AVAILABLE = "Todavía no disponible.";

type Args = Record<string, unknown> | undefined;
const handlers: Record<string, (args: Args) => unknown> = {
  get_snapshot: () => datos.snapshot,
  take_pending_route: () => null,
  appearance: () => ({ accent: "#007aff", accent_text: "#ffffff" }),
  process_usage: (a) => datos.usage((a?.pids as number[]) ?? []),
  close_if_minimized: () => null,
  clear_problems: () => null,
  logs_projects: () => datos.summaries,
  logs_roots: () => ["/Users/ana/Code"],
  logs_query: () => ({ entries: datos.entries, next_before: null, scanned_bytes: 182_000 }),
  logs_groups: () => [],
  logs_entry: (a) => datos.detail(String(a?.id ?? "")),
  logs_subscribe: () => 1,
  logs_unsubscribe: () => null,
  logs_mark_seen: () => null,
  setup_status: () => ({
    needed: false,
    state: { version: 1, scenario: "laravel_services", database: "postgresql", custom: null, completed_at: null },
    news: false,
  }),
  setup_scenarios: () => datos.cards,
  setup_detect: () => datos.wizardDetection,
  setup_shell_preview: () => ({ needed: false, path: "~/.zshrc", diff: "", hash: "" }),
  mcp_status: () => datos.mcpStatus,
  mcp_snippets: () => datos.mcpSnippets,
  mcp_activity: () => [],
  php_catalog: () => Promise.reject(NOT_AVAILABLE),
};

mockIPC(
  (cmd, args) => {
    const handler = handlers[cmd];
    return handler ? handler(args as Args) : Promise.reject(NOT_AVAILABLE);
  },
  { shouldMockEvents: true },
);

// La app se importa después de simular el IPC: al montarse ya pide el snapshot.
await import("../main");
