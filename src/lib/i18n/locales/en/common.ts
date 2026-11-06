// common · inglés. Las mismas claves que es/common.ts: si falta o sobra una, no compila.
import type es from "../es/common";
import type { Dictionary } from "../../types";

export default {
  time: {
    justNow: "just now",
    minutesAgo: "{n}\u00a0min ago",
    inMinutes: "in {n}\u00a0min",
    hoursAgo: "{n}\u00a0hr ago",
    inHours: "in {n}\u00a0hr",
    yesterday: "yesterday",
    daysAgo: { one: "{count} day ago", other: "{count} days ago" },
  },
} satisfies Dictionary<typeof es>;
