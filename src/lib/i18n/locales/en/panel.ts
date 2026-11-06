// panel · inglés. Las mismas claves que es/panel.ts: si falta o sobra una, no compila.
import type es from "../es/panel";
import type { Dictionary } from "../../types";

export default {
  title: "Overview",
  hero: {
    label: "Status",
    noneRunning: "No services running",
    noServices: "No services",
    running: { one: "{count} service", other: "{count} services" },
    actionFailed: "Couldn’t complete the action with {name}",
    moreProblems: { one: "{count} more problem", other: "{count} more problems" },
    viewProblem: "View problem",
    allRunning: "Everything running",
    nothingRunning: "Nothing running",
    startServices: "Start services",
    viewServices: "View services",
  },
  php: {
    title: "Default PHP",
    empty: "No versions installed.",
    goToPhp: "Go to PHP",
    selectLabel: "Default PHP version",
  },
  services: {
    title: "Services",
    empty: "No services are running or start at login.",
    viewAll: "View services",
    toggleStart: "Start {id}",
    toggleStop: "Stop {id}",
  },
  errors: {
    title: "Recent errors",
    unavailable: "Logs aren’t available yet.",
    none: "No errors in the last hour",
  },
  mailpit: {
    description: "Catches the mail your projects send, without it leaving your Mac.",
    installFromServices: "Install from Services",
    stopped: "It’s stopped.",
    running: "Running",
    messages: { one: "{count} message", other: "{count} messages" },
    start: "Start Mailpit",
    open: "Open Mailpit",
  },
  sites: {
    title: "Sites",
    viewAll: "View all sites",
  },
} satisfies Dictionary<typeof es>;
