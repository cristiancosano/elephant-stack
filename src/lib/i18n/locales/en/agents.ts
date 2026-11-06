// agents · inglés. Las mismas claves que es/agents.ts: si falta o sobra una, no compila.
import type es from "../es/agents";
import type { Dictionary } from "../../types";

export default {
  mode: {
    title: "Mode",
    rowTitle: "What agents can do",
    label: "Agent channel mode",
    footer: "{help} Reads never ask for confirmation. Neither agents nor the elephant command can change the mode.",
    readOnly: "Read-only",
    confirm: "Confirm",
    trusted: "Trusted",
    help: {
      readOnly: "Agents can read the status and the logs, but can’t change anything.",
      confirm:
        "Every change an agent asks for (restarting a service, switching PHP, creating a project) shows an Elephant Stack dialog. Scripts and git hooks count as agents. What’s typed in a terminal counts as yours and doesn’t ask: an agent that runs commands in its own terminal without identifying itself (without CLAUDECODE or AI_AGENT) would also pass as you. Connect those agents over MCP, which always asks, or use Read-only.",
      trusted: "Agents change the environment without asking. Only use it if you trust what they run.",
    },
    changeFailed: "Couldn’t change the mode: {error}",
  },
  audit: {
    tool: {
      initialize: "MCP connection",
      elephantStatus: "Status",
      serviceControl: "Service",
      phpSetGlobal: "Global PHP",
      phpSetProject: "Project PHP",
      componentInstall: "Install component",
      projectStartServices: "Start project services",
      projectCreate: "Create project",
      logsTail: "Read logs",
      logsSearch: "Search logs",
      logsErrorsSummary: "Error summary",
      logsEntry: "Log entry",
      logsSources: "Log sources",
      logsWatch: "Watch logs",
    },
    title: "Activity",
    footer: "Changes (including denied ones) and log reads are recorded, without their contents.",
    empty: "No activity yet",
    confirmed: "confirmed",
    denied: "Denied",
    error: "Error",
    client: { unknown: "Unknown", terminal: "Terminal", script: "Script" },
  },
  plugin: {
    connected: "Claude Code now has the Elephant Stack plugin. It works in new sessions.",
    notVisible: "The commands ran, but Claude Code doesn’t show the plugin yet.",
  },
  copied: {
    askSnippet: "Snippet with more prompts copied",
    allowSnippet: "Permissions snippet copied",
    guide: "Guide copied",
    claudeDesktop: "Claude Desktop snippet copied",
    cursor: "Cursor snippet copied",
    vscode: "VS Code snippet copied",
    projectJson: ".mcp.json snippet copied",
    command: "Command copied",
  },
  link: {
    title: "elephant command",
    statusError: "Couldn’t read the status",
    footer:
      "Agents use the elephant command to read logs, check the status and, with your permission, change services or PHP. No process stays open: each command runs and exits.",
    switch: "Turn on the elephant command",
    off: "Off",
  },
  test: {
    title: "Test connection",
    result: "{tools} operations · {mode} mode · {ms} ms",
    lastUsed: "Used {when}",
    neverUsed: "No agent has used it yet",
    action: "Test",
  },
  claude: {
    title: "Claude Code",
    footer:
      "The plugin includes a skill that teaches Claude Code to use elephant, and it doesn’t start any server. During the skill, elephant commands don’t ask for permission in Claude Code. Changes still go through the Elephant Stack dialog.",
    plugin: "Elephant Stack plugin",
    notFound: "Couldn’t find the claude command",
    connected: "Connected",
    notConnected: "Not connected",
    connect: "Connect…",
    reconnect: "Reconnect…",
  },
  permissions: {
    title: "Permissions outside the skill",
    subtitle: "Paste it in ~/.claude/settings.json so elephant doesn’t ask for permission every time.",
    moreAsks: "With more prompts",
  },
  guide: {
    title: "Other agents with a terminal",
    footer: "Cursor, Codex or Gemini CLI can use the same elephant command. Paste the guide in AGENTS.md or in the agent’s rules.",
    row: "elephant usage guide",
    copy: "Copy guide",
  },
  mcp: {
    title: "MCP clients",
    footer:
      "For clients that only speak MCP. It starts with elephant mcp when the client asks for it, with the same mode and confirmations.",
    project: "Project (.mcp.json)",
    projectSubtitle: "No personal paths: you can commit it to the repository",
    advanced: "Advanced: Claude Code over MCP",
    advancedSubtitle: "Duplicates the plugin and opens a process in every session. You don’t need it if you use the plugin.",
    copyCommand: "Copy command",
  },
  connectSheet: {
    title: "Connect Claude Code",
    intro: "Elephant Stack will write the plugin to its folder and run these Claude Code commands:",
    note: "No other Claude Code setting is changed. The plugin works in new sessions.",
    confirm: "Connect",
  },
  disableSheet: {
    title: "Turn off the elephant command",
    text: "The elephant command will be removed and the Claude Code plugin uninstalled (claude plugin uninstall elephant@elephant-stack). Agents won’t be able to use Elephant Stack anymore.",
    confirm: "Turn off",
  },
  sheet: {
    title: "Connect agents",
    bodyBefore: "Claude Code can check your projects’ logs and turn services on with",
    bodyAfter:
      "Anything that changes something asks for confirmation, unless you choose another mode in Settings ▸ Agents. To install the plugin, run this in the terminal:",
    copyCommands: "Copy commands",
    copyGuide: "Copy guide for other agents",
    mcpIntro: "For other MCP clients (Cursor, VS Code…), this is the project snippet:",
    copySnippet: "Copy snippet",
    openSettings: "Open Settings ▸ Agents",
    done: "Done",
  },
} satisfies Dictionary<typeof es>;
