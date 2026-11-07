// shell · inglés. Las mismas claves que es/shell.ts: si falta o sobra una, no compila.
import type es from "../es/shell";
import type { Dictionary } from "../../types";

export default {
  nav: {
    label: "Sections",
    overview: "Overview",
    sites: "Sites",
    services: "Services",
    php: "PHP",
    node: "Node",
    logs: "Logs",
    settings: "Settings",
    nothingRunning: "Nothing running",
    runningCount: { one: "{count} running", other: "{count} running" },
    sitesCount: { one: "{count} site", other: "{count} sites" },
    servicesRunning: {
      one: "{running} of {count} service running",
      other: "{running} of {count} services running",
    },
    defaultPhp: "Default PHP {php}",
    defaultNode: "Default Node {node}",
    unseenErrors: { one: "{count} unseen error", other: "{count} unseen errors" },
    agents: "Agents · {mode}",
    agentMode: {
      readOnly: "read-only",
      confirm: "on",
      trusted: "no confirmation",
    },
  },
  sidebar: {
    hide: "Hide sidebar",
    show: "Show sidebar",
  },
  snapshotFailed: "Couldn’t read the status: {error}",
} satisfies Dictionary<typeof es>;
