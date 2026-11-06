// agents · Ajustes ▸ Agentes y la hoja de agentes del asistente (área diferida).
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  mode: {
    title: "Modo",
    rowTitle: "Qué pueden hacer los agentes",
    label: "Modo del canal de agentes",
    footer: "{help} Las lecturas nunca piden confirmación. Ni los agentes ni la orden elephant pueden cambiar el modo.",
    readOnly: "Solo lectura",
    confirm: "Confirmar",
    trusted: "De confianza",
    help: {
      readOnly: "Los agentes pueden leer el estado y los logs, pero no cambiar nada.",
      confirm:
        "Cada cambio que pide un agente (reiniciar un servicio, cambiar el PHP, crear un proyecto) muestra un diálogo de Elephant Stack. Los scripts y git hooks cuentan como agentes. Lo que se escribe en una terminal cuenta como tuyo y no pregunta: un agente que ejecuta órdenes en su propia terminal sin anunciarse (sin CLAUDECODE ni AI_AGENT) también pasaría por ti. Para esos agentes, conéctalos por MCP, que siempre pregunta, o usa Solo lectura.",
      trusted: "Los agentes cambian el entorno sin preguntar. Úsalo solo si confías en lo que ejecutan.",
    },
    changeFailed: "No se pudo cambiar el modo: {error}",
  },
  audit: {
    tool: {
      initialize: "Conexión MCP",
      elephantStatus: "Estado",
      serviceControl: "Servicio",
      phpSetGlobal: "PHP global",
      phpSetProject: "PHP del proyecto",
      componentInstall: "Instalar componente",
      projectStartServices: "Arrancar servicios del proyecto",
      projectCreate: "Crear proyecto",
      logsTail: "Leer logs",
      logsSearch: "Buscar en logs",
      logsErrorsSummary: "Resumen de errores",
      logsEntry: "Entrada de log",
      logsSources: "Fuentes de logs",
      logsWatch: "Vigilar logs",
    },
    title: "Actividad",
    footer: "Se registran los cambios (también los denegados) y las lecturas de logs, sin su contenido.",
    empty: "Sin actividad todavía",
    confirmed: "confirmado",
    denied: "Denegado",
    error: "Error",
    client: { unknown: "Desconocido", terminal: "Terminal", script: "Script" },
  },
  plugin: {
    connected: "Claude Code ya tiene el plugin de Elephant Stack. Funciona en las sesiones nuevas.",
    notVisible: "Se han ejecutado los comandos, pero Claude Code no muestra el plugin todavía.",
  },
  copied: {
    askSnippet: "Fragmento con más avisos copiado",
    allowSnippet: "Fragmento de permisos copiado",
    guide: "Guía copiada",
    claudeDesktop: "Fragmento de Claude Desktop copiado",
    cursor: "Fragmento de Cursor copiado",
    vscode: "Fragmento de VS Code copiado",
    projectJson: "Fragmento .mcp.json copiado",
    command: "Comando copiado",
  },
  link: {
    title: "Orden elephant",
    statusError: "No se ha podido leer el estado",
    footer:
      "Los agentes usan la orden elephant para leer logs, ver el estado y, con tu permiso, cambiar servicios o el PHP. No queda ningún proceso abierto: cada orden se ejecuta y termina.",
    switch: "Activar la orden elephant",
    off: "Desactivada",
  },
  test: {
    title: "Probar conexión",
    result: "{tools} operaciones · modo {mode} · {ms} ms",
    lastUsed: "Usado {when}",
    neverUsed: "Todavía no lo ha usado ningún agente",
    action: "Probar",
  },
  claude: {
    title: "Claude Code",
    footer:
      "El plugin trae una skill que enseña a Claude Code a usar elephant y no arranca ningún servidor. Durante la skill, las órdenes elephant no piden permiso en Claude Code; los cambios siguen pasando por el diálogo de Elephant Stack.",
    plugin: "Plugin de Elephant Stack",
    notFound: "No se encuentra la orden claude",
    connected: "Conectado",
    notConnected: "Sin conectar",
    connect: "Conectar…",
    reconnect: "Volver a conectar…",
  },
  permissions: {
    title: "Permisos fuera de la skill",
    subtitle: "Pégalo en ~/.claude/settings.json para que elephant no pida permiso cada vez.",
    moreAsks: "Con más avisos",
  },
  guide: {
    title: "Otros agentes con terminal",
    footer: "Cursor, Codex o Gemini CLI pueden usar la misma orden elephant. Pega la guía en AGENTS.md o en las reglas del agente.",
    row: "Guía de uso de elephant",
    copy: "Copiar guía",
  },
  mcp: {
    title: "Clientes MCP",
    footer:
      "Para los clientes que solo hablan MCP. Se arranca con elephant mcp cuando el cliente lo pide y aplica el mismo modo y las mismas confirmaciones.",
    project: "Proyecto (.mcp.json)",
    projectSubtitle: "Sin rutas personales: se puede subir al repositorio",
    advanced: "Avanzado: Claude Code por MCP",
    advancedSubtitle: "Duplica el plugin y abre un proceso en cada sesión: no hace falta si usas el plugin.",
    copyCommand: "Copiar comando",
  },
  connectSheet: {
    title: "Conectar Claude Code",
    intro: "Elephant Stack escribirá el plugin en su carpeta y ejecutará estos comandos de Claude Code:",
    note: "No se modifica ningún otro ajuste de Claude Code. El plugin funciona en las sesiones nuevas.",
    confirm: "Conectar",
  },
  disableSheet: {
    title: "Desactivar la orden elephant",
    text: "Se quitará la orden elephant y se desinstalará el plugin de Claude Code (claude plugin uninstall elephant@elephant-stack). Los agentes dejarán de poder usar Elephant Stack.",
    confirm: "Desactivar",
  },
  sheet: {
    title: "Conectar agentes",
    bodyBefore: "Claude Code puede consultar los logs de tus proyectos y encender servicios con el comando",
    bodyAfter:
      "Lo que cambia algo te pide confirmación, salvo que elijas otro modo en Ajustes ▸ Agentes. Para instalar el plugin, ejecuta en la terminal:",
    copyCommands: "Copiar comandos",
    copyGuide: "Copiar guía para otros agentes",
    mcpIntro: "Para otros clientes con MCP (Cursor, VS Code…), este es el fragmento del proyecto:",
    copySnippet: "Copiar fragmento",
    openSettings: "Abrir Ajustes ▸ Agentes",
    done: "Listo",
  },
} as const satisfies Tree;
