// settings · inglés. Las mismas claves que es/settings.ts: si falta o sobra una, no compila.
import type es from "../es/settings";
import type { Dictionary } from "../../types";

export default {
  title: "Settings",
  tabs: {
    label: "Settings sections",
    general: "General",
    scenario: "Setup",
    logs: "Logs",
    agents: "Agents",
  },
  general: {
    saveFailed: "Couldn’t save the setting: {error}",
    menuBar: {
      footer:
        "When you close or minimize the window, Elephant Stack disappears from the Dock but stays in the menu bar. To quit completely, choose Quit from its menu or press ⌘Q.",
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
      subtitle: "Also applies to the menu bar menu and the elephant command",
      system: "System",
    },
    editor: {
      label: "Open files with",
      subtitle: "Log files and project folders",
    },
    phpSource: {
      title: "PHP versions",
      footer:
        "A custom source must use the same layout as php-builds: an index.json in the “catalog” release, or php-X.Y.Z releases with the php-X.Y.Z-macos-arm64.tar.gz package and its .sha256 file. These binaries become your php command, and the SHA-256 checksum comes from the same source, so only use sources you trust.",
      origin: "Download source",
      recommended: "{source} (recommended)",
      modeLabel: "PHP version source",
      custom: "Custom",
      field: "Repository or index",
      placeholder: "owner/repo or https://…/index.json",
      fieldLabel: "GitHub repository or PHP version index URL",
      test: "Test",
      checking: "Checking source…",
      checkingLabel: "Checking source",
      invalid: "Can’t be used as a source",
      inUse: "In use",
      use: "Use this source",
      saved: "PHP versions will now be downloaded from the new source.",
      versions: { one: "{count} version: {list}", other: "{count} versions: {list}" },
      andMore: "{list}, and {count} more",
    },
    assistant: {
      footer: "Review which services and PHP versions you want without losing what you already have.",
      title: "Setup assistant",
      rerun: "Run setup assistant again…",
    },
  },
  scenario: {
    title: "Setup",
    readError: "Couldn’t read the setup",
    none: {
      keep: "You chose “Don’t change anything”: Elephant hasn’t installed or changed anything on your Mac.",
      never: "You haven’t gone through the assistant yet.",
    },
    empty: "No setup",
    choose: "Choose setup…",
    footer: "Applying it again only adds what’s missing: it never uninstalls or turns anything off.",
    applied: "Applied {date}",
    checking: "Checking",
    checkingEllipsis: "Checking…",
    unavailable: "Not available",
    updates: {
      title: "There’s something new for your setup",
      subtitle: "This version of Elephant can do more. Review the changes before applying them.",
    },
    diff: "Differences",
    optional: "Optional",
    checkFailed: "Couldn’t check: {error}",
    change: "Change setup…",
    reviewApply: "Review and apply…",
  },
} satisfies Dictionary<typeof es>;
