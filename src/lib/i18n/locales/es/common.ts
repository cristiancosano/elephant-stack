// common · Textos que comparten varias áreas y los de los formatos (fechas relativas).
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  time: {
    justNow: "ahora mismo",
    minutesAgo: "hace {n} min",
    inMinutes: "dentro de {n} min",
    hoursAgo: "hace {n} h",
    inHours: "dentro de {n} h",
    yesterday: "ayer",
    daysAgo: { one: "hace {count} día", other: "hace {count} días" },
  },
  refresh: "Actualizar",
  retry: "Reintentar",
  cancel: "Cancelar",
  save: "Guardar",
  copy: "Copiar",
  moreActions: "Más acciones",
  openInBrowser: "Abrir {host} en el navegador",
  openInEditor: "Abrir en el editor",
  revealInFinder: "Mostrar en Finder",
  viewLogs: "Ver logs",
  copied: "Copiado: {what}",
  system: "Sistema",
  project: "Proyecto",
  start: "Arrancar",
  stop: "Detener",
  restart: "Reiniciar",
} as const satisfies Tree;
