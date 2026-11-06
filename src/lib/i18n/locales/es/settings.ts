// settings · Ajustes: la vista, General y Escenario.
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  title: "Ajustes",
  tabs: {
    label: "Sección de los ajustes",
    general: "General",
    scenario: "Escenario",
    logs: "Logs",
    agents: "Agentes",
  },
  general: {
    saveFailed: "No se pudo guardar el ajuste: {error}",
    menuBar: {
      footer:
        "Al cerrar o minimizar la ventana, Elephant Stack desaparece del Dock y sigue en la barra de menús. Para salir del todo, usa «Salir» en su menú o ⌘Q.",
      keep: "Seguir en la barra de menús al cerrar",
      showPhp: "Mostrar la versión de PHP junto al icono",
      showPhpLabel: "Mostrar la versión de PHP junto al icono de la barra de menús",
      watch: "Vigilar servicios en segundo plano",
      watchSubtitle: "Avisa en la barra de menús si un servicio se detiene",
    },
    appearance: {
      label: "Apariencia",
      system: "Sistema",
      light: "Claro",
      dark: "Oscuro",
    },
    language: {
      label: "Idioma",
      subtitle: "También el del menú de la barra y la orden elephant",
      system: "Sistema",
    },
    editor: {
      label: "Editor para abrir archivos",
      subtitle: "Los archivos de los logs y las carpetas de los proyectos",
    },
    phpSource: {
      title: "Versiones de PHP",
      footer:
        "Una fuente propia debe publicar igual que php-builds: un index.json en la release «catalog», o releases php-X.Y.Z con el paquete php-X.Y.Z-macos-arm64.tar.gz y su .sha256. Esos binarios se ejecutan como tu php, y el SHA-256 viene de la misma fuente: usa solo fuentes de confianza.",
      origin: "Fuente de las descargas",
      recommended: "{source} (recomendado)",
      modeLabel: "Fuente de las versiones de PHP",
      custom: "Personalizada",
      field: "Repositorio o índice",
      placeholder: "propietario/repo o https://…/index.json",
      fieldLabel: "Repositorio de GitHub o URL del índice de versiones de PHP",
      test: "Probar",
      checking: "Comprobando la fuente…",
      checkingLabel: "Comprobando la fuente",
      invalid: "No sirve como fuente",
      inUse: "En uso",
      use: "Usar esta fuente",
      saved: "Las versiones de PHP se descargarán de la nueva fuente.",
      versions: { one: "{count} versión: {list}", other: "{count} versiones: {list}" },
      andMore: "{list} y {count} más",
    },
    assistant: {
      footer: "Revisa qué servicios y versiones de PHP quieres, sin perder lo que ya tienes.",
      title: "Asistente de configuración",
      rerun: "Volver a ejecutar el asistente…",
    },
  },
  scenario: {
    title: "Escenario",
    readError: "No se ha podido leer el escenario",
    none: {
      keep: "Elegiste «No cambiar nada»: Elephant no ha instalado ni cambiado nada en tu Mac.",
      never: "Todavía no has pasado por el asistente.",
    },
    empty: "Sin escenario",
    choose: "Elegir escenario…",
    footer: "Volver a aplicarlo solo suma lo que falte: nunca desinstala ni apaga nada.",
    applied: "Aplicado {date}",
    checking: "Comprobando",
    checkingEllipsis: "Comprobando…",
    unavailable: "No disponible",
    updates: {
      title: "Hay novedades para tu escenario",
      subtitle: "Esta versión de Elephant sabe hacer más cosas. Revísalas antes de aplicarlas.",
    },
    diff: "Diferencias",
    optional: "Opcional",
    checkFailed: "No se ha podido comprobar: {error}",
    change: "Cambiar de escenario…",
    reviewApply: "Revisar y aplicar…",
  },
} as const satisfies Tree;
