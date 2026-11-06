// common · inglés. Las mismas claves que es/common.ts: si falta o sobra una, no compila.
import type es from "../es/common";
import type { Dictionary } from "../../types";

export default {
  time: {
    justNow: "just now",
    minutesAgo: "{n} min ago",
    inMinutes: "in {n} min",
    hoursAgo: "{n} hr ago",
    inHours: "in {n} hr",
    yesterday: "yesterday",
    daysAgo: { one: "{count} day ago", other: "{count} days ago" },
  },
  refresh: "Refresh",
  retry: "Try again",
  cancel: "Cancel",
  save: "Save",
  copy: "Copy",
  moreActions: "More actions",
  openInBrowser: "Open {host} in browser",
  openInEditor: "Open in editor",
  revealInFinder: "Show in Finder",
  viewLogs: "View logs",
  copied: "Copied: {what}",
  system: "System",
  project: "Project",
  start: "Start",
  stop: "Stop",
  restart: "Restart",
} satisfies Dictionary<typeof es>;
