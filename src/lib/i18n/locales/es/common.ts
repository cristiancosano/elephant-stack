// common · Textos que comparten varias áreas y los de los formatos (fechas relativas).
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  time: {
    justNow: "ahora mismo",
    minutesAgo: "hace {n}\u00a0min",
    inMinutes: "dentro de {n}\u00a0min",
    hoursAgo: "hace {n}\u00a0h",
    inHours: "dentro de {n}\u00a0h",
    yesterday: "ayer",
    daysAgo: { one: "hace {count} día", other: "hace {count} días" },
  },
} as const satisfies Tree;
