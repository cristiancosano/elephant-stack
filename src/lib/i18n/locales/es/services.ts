// services · Vista Servicios y los textos de servicios de stores/app.
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  title: "Servicios",
  installed: "{id} se ha instalado.",
  menu: {
    restart: "Reiniciar",
    startAtLogin: "Arrancar al iniciar sesión",
    viewLogs: "Ver logs",
    revealInFinder: "Mostrar en Finder",
    processes: { one: "{count} proceso", other: "{count} procesos" },
  },
  bulk: {
    label: "Acciones de todos los servicios",
    restartAll: "Reiniciar todos",
    stopAll: "Detener todos",
  },
  empty: {
    title: "No se ha encontrado nada",
    message: "Elephant Stack usa Homebrew para los servicios. Comprueba que está instalado y pulsa Actualizar.",
  },
  jobs: { title: "Tareas de agentes" },
  group: { services: "Servicios", tools: "Herramientas" },
  row: {
    running: "En marcha",
    stopped: "Parado",
    exitCode: "Error · código {code}",
    exitUnknown: "Error · código desconocido",
    atLogin: "Al iniciar sesión",
    installed: "Instalado",
    version: "Versión {version}",
    toggleStart: "Arrancar {id}",
    toggleStop: "Detener {id}",
    more: "Más opciones de {id}",
    reveal: "Mostrar {id} en Finder ({path})",
  },
  missing: { title: "Sin instalar ({count})" },
  install: { busy: "Instalando…", action: "Instalar" },
  job: {
    installing: "Instalando {target}…",
    installingAny: "Instalando…",
    creating: "Creando {target}…",
    creatingAny: "Creando un proyecto…",
    queued: "En cola…",
    running: "En curso…",
    requestedBy: "{what} · pedido por {who}",
    terminal: "la terminal",
    script: "un script",
  },
} as const satisfies Tree;
