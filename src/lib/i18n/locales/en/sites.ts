// sites · inglés. Las mismas claves que es/sites.ts: si falta o sobra una, no compila.
import type es from "../es/sites";
import type { Dictionary } from "../../types";

export default {
  title: "Sites",
  kind: { laravel: "Laravel", php: "PHP", other: "Other" },
  needs: {
    started: "Started for {name}: {list}.",
    nothing: "{name} already has what it needs running.",
  },
  copied: "Copied: {url}",
  menu: {
    openBrowser: "Open in browser",
    openEditor: "Open in editor",
    openTerminal: "Open in Terminal",
    revealInFinder: "Show in Finder",
    copyUrl: "Copy URL",
    viewLogs: "View logs",
    startNeeds: "Start what it needs",
  },
  php: {
    done: "{name} will use PHP {version} (.php-version).",
    selectLabel: "PHP for {name}",
    notInstalled: "{version} · not installed",
    byDefault: "Default PHP",
  },
  more: "More options for {name}",
  empty: {
    title: "Herd serves your sites for now",
    message: "When Herd serves a project or you add a folder in Settings ▸ Logs, it shows up here.",
    action: "Set up with the assistant",
  },
  group: {
    footer: "Served by Herd. Elephant Stack doesn’t change its configuration.",
    others: "Other projects",
  },
} satisfies Dictionary<typeof es>;
