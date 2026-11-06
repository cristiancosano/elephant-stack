// panel · Vista Panel (Dashboard) y su héroe.
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  title: "Panel",
  hero: {
    label: "Estado",
    noneRunning: "Ningún servicio en marcha",
    noServices: "Sin servicios",
    running: { one: "{count} servicio", other: "{count} servicios" },
    actionFailed: "No se pudo completar la acción con {name}",
    moreProblems: { one: "{count} problema más", other: "{count} problemas más" },
    viewProblem: "Ver problema",
    allRunning: "Todo en marcha",
    nothingRunning: "Nada en marcha",
    startServices: "Arrancar servicios",
    viewServices: "Ver servicios",
  },
  php: {
    title: "PHP por defecto",
    empty: "No hay ninguna versión instalada.",
    goToPhp: "Ir a PHP",
    selectLabel: "Versión de PHP por defecto",
  },
  services: {
    title: "Servicios",
    empty: "Ningún servicio está en marcha ni arranca al iniciar sesión.",
    viewAll: "Ver servicios",
    toggleStart: "Arrancar {id}",
    toggleStop: "Detener {id}",
  },
  errors: {
    title: "Errores recientes",
    unavailable: "Los logs todavía no están disponibles.",
    none: "Sin errores en la última hora",
  },
  mailpit: {
    description: "Recoge el correo que envían tus proyectos, sin que salga de tu Mac.",
    installFromServices: "Instalar desde Servicios",
    stopped: "Está detenido.",
    running: "En marcha",
    messages: { one: "{count} correo", other: "{count} correos" },
    start: "Arrancar Mailpit",
    open: "Abrir Mailpit",
  },
  sites: {
    title: "Sitios",
    viewAll: "Ver todos los sitios",
  },
} as const satisfies Tree;
