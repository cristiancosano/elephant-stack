// settings · inglés. Las mismas claves que es/settings.ts: si falta o sobra una, no compila.
import type es from "../es/settings";
import type { Dictionary } from "../../types";

export default {
  title: "Settings",
  tabs: {
    label: "Settings section",
    general: "General",
    scenario: "Setup",
    logs: "Logs",
    agents: "Agents",
  },
  general: {
    saveFailed: "Couldn’t save the setting: {error}",
    menuBar: {
      footer:
        "When you close or minimize the window, Elephant Stack leaves the Dock and stays in the menu bar. To quit completely, choose Quit from its menu or press ⌘Q.",
      keep: "Keep in menu bar when closed",
      showPhp: "Show PHP version next to the icon",
      showPhpLabel: "Show PHP version next to the menu bar icon",
      watch: "Monitor services in the background",
      watchSubtitle: "Alerts you in the menu bar when a service stops",
    },
    appearance: {
      label: "Appearance",
      system: "System",
      light: "Light",
      dark: "Dark",
    },
    language: {
      label: "Language",
      subtitle: "Also used in the menu bar and the elephant command",
      system: "System",
    },
    editor: {
      label: "Editor for opening files",
      subtitle: "Log files and project folders",
    },
    phpSource: {
      title: "PHP versions",
      footer:
        "A custom source must publish the same way as php-builds: an index.json in the “catalog” release, or php-X.Y.Z releases with the php-X.Y.Z-macos-arm64.tar.gz package and its .sha256. These binaries run as your php, and the SHA-256 comes from the same source, so only use sources you trust.",
      origin: "Download source",
      recommended: "{source} (recommended)",
      modeLabel: "Source of PHP versions",
      custom: "Custom",
      field: "Repository or index",
      placeholder: "user/repo or https://…/index.json",
      fieldLabel: "GitHub repository or URL of the PHP version index",
      test: "Test",
      checking: "Checking source…",
      checkingLabel: "Checking source",
      invalid: "Can’t be used as a source",
      inUse: "In use",
      use: "Use this source",
      saved: "PHP versions will now download from the new source.",
      versions: { one: "{count} version: {list}", other: "{count} versions: {list}" },
      andMore: "{list} and {count} more",
    },
    assistant: {
      footer: "Review which services and PHP versions you want without losing what you already have.",
      title: "Setup assistant",
      rerun: "Run setup assistant again…",
    },
  },
} satisfies Dictionary<typeof es>;
