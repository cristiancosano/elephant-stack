// shell · Ventana: barra lateral, armazón de las vistas y avisos generales (Shell, Sidebar, App).
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  nav: {
    label: "Secciones",
    overview: "Panel",
    sites: "Sitios",
    services: "Servicios",
    php: "PHP",
    node: "Node",
    logs: "Logs",
    settings: "Ajustes",
    nothingRunning: "Nada en marcha",
    runningCount: { one: "{count} en marcha", other: "{count} en marcha" },
    sitesCount: { one: "{count} sitio", other: "{count} sitios" },
    servicesRunning: {
      one: "{running} de {count} servicio en marcha",
      other: "{running} de {count} servicios en marcha",
    },
    defaultPhp: "PHP {php} por defecto",
    defaultNode: "Node {node} por defecto",
    unseenErrors: { one: "{count} error sin ver", other: "{count} errores sin ver" },
    agents: "Agentes · {mode}",
    agentMode: {
      readOnly: "solo lectura",
      confirm: "activo",
      trusted: "sin confirmación",
    },
  },
  sidebar: {
    hide: "Ocultar la barra lateral",
    show: "Mostrar la barra lateral",
  },
  snapshotFailed: "No se pudo leer el estado: {error}",
} as const satisfies Tree;
