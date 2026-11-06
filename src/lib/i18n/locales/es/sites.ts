// sites · Vista Sitios (área diferida).
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  title: "Sitios",
  kind: { laravel: "Laravel", php: "PHP", other: "Otro" },
  needs: {
    started: "Arrancado para {name}: {list}.",
    nothing: "{name} ya tiene en marcha lo que necesita.",
  },
  copied: "Copiado: {url}",
  menu: {
    openBrowser: "Abrir en el navegador",
    openEditor: "Abrir en el editor",
    openTerminal: "Abrir terminal",
    revealInFinder: "Mostrar en Finder",
    copyUrl: "Copiar URL",
    viewLogs: "Ver logs",
    startNeeds: "Arrancar lo que necesita",
  },
  php: {
    done: "{name} usará PHP {version} (.php-version).",
    selectLabel: "PHP de {name}",
    notInstalled: "{version} · no instalada",
    byDefault: "PHP por defecto",
  },
  more: "Más opciones de {name}",
  empty: {
    title: "Tus sitios los sirve Herd por ahora",
    message: "Cuando Herd sirva un proyecto o añadas una carpeta en Ajustes ▸ Logs, aparecerá aquí.",
    action: "Configurar con el asistente",
  },
  group: {
    footer: "Los sirve Herd. Elephant Stack no cambia su configuración.",
    others: "Otros proyectos",
  },
} as const satisfies Tree;
