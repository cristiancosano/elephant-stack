// services · inglés. Las mismas claves que es/services.ts: si falta o sobra una, no compila.
import type es from "../es/services";
import type { Dictionary } from "../../types";

export default {
  title: "Services",
  installed: "{id} is installed.",
  menu: {
    restart: "Restart",
    startAtLogin: "Start at login",
    viewLogs: "View logs",
    revealInFinder: "Show in Finder",
    processes: { one: "{count} process", other: "{count} processes" },
  },
  bulk: {
    label: "Actions for all services",
    restartAll: "Restart all",
    stopAll: "Stop all",
  },
  empty: {
    title: "Nothing found",
    message: "Elephant Stack uses Homebrew for services. Check that it’s installed and click Refresh.",
  },
  jobs: { title: "Agent jobs" },
  group: { services: "Services", tools: "Tools" },
  row: {
    running: "Running",
    stopped: "Stopped",
    exitCode: "Error · exit code {code}",
    exitUnknown: "Error · unknown exit code",
    atLogin: "At login",
    installed: "Installed",
    version: "Version {version}",
    toggleStart: "Start {id}",
    toggleStop: "Stop {id}",
    more: "More options for {id}",
    reveal: "Show {id} in Finder ({path})",
  },
  missing: { title: "Not installed ({count})" },
  install: { busy: "Installing…", action: "Install" },
  job: {
    installing: "Installing {target}…",
    installingAny: "Installing…",
    creating: "Creating {target}…",
    creatingAny: "Creating a project…",
    queued: "Queued…",
    running: "In progress…",
    requestedBy: "{what} · requested by {who}",
    terminal: "the terminal",
    script: "a script",
  },
} satisfies Dictionary<typeof es>;
