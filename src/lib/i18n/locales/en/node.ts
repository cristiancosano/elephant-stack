// node · inglés. Las mismas claves que es/node.ts: si falta o sobra una, no compila.
import type es from "../es/node";
import type { Dictionary } from "../../types";

export default {
  title: "Node",
  versions: { title: "Versions" },
  badge: { default: "Default" },
  lts: "LTS {name}",
  current: "Current",
  npm: "npm {version}",
  globals: "Globals: {list}",
  installedDefault: "Default",
  remote: { notInstalled: "Not installed" },
  footer: {
    default:
      "The checked version is the default, used outside any project with a .nvmrc or .node-version file. Versions that aren’t installed are downloaded from nodejs.org and checked against its SHASUMS256.",
    offline: "Offline: the versions that aren’t installed are from the last check.",
  },
  showAll: {
    action: { one: "Show {count} more version", other: "Show {count} more versions" },
    hint: "Major versions without support or that are no longer Current.",
    less: "Show only the recommended ones",
  },
  install: {
    action: "Install",
    busy: "Installing Node {version}",
    done: "Node {version} is ready. node, npm and npx now use it.",
  },
  update: {
    action: "Update to {next}",
    busy: "Updating Node {version} to {next}",
    done: "Node {old} updated to {next}.",
    doneWithProjects: {
      one: "Node {old} updated to {next}, also in {count} project.",
      other: "Node {old} updated to {next}, also in {count} projects.",
    },
  },
  useIn: {
    action: "Use in project…",
    pickFolder: "Project folder that will use Node {version}",
    done: "{name} will use Node {spec} ({file}).",
  },
  menu: {
    more: "More actions for Node {version}",
    makeDefault: "Use as default",
    reveal: "Show in Finder",
    uninstall: "Uninstall",
  },
  uninstall: { done: "Node {version} uninstalled." },
  defaultSet: "Node {spec} is now the default.",
  empty: {
    title: "No Node versions",
    message: "No versions are installed and the available ones couldn’t be checked.",
  },
  catalog: {
    errorTitle: "Couldn’t check the available versions",
    errorMessage: "Check the connection and try again in a few minutes.",
  },
  terminal: {
    title: "Terminal",
    shadowed: {
      title: "The terminal uses {who}’s Node first",
      message: "In ~/.zshrc, {who} loads after Elephant’s line and ends up first in PATH. Move this line to the end of ~/.zshrc and open a new terminal:",
    },
    missing: {
      title: "The terminal doesn’t use Elephant’s selectors",
      message: "~/.elephant-stack/bin isn’t in PATH. Add this line to the end of ~/.zshrc and open a new terminal:",
    },
    copy: "Copy the line",
    copied: "Line copied.",
    nvm: {
      title: "nvm is installed",
      subtitle: "Elephant doesn’t use it or touch {path}.",
      message: "In the terminal, PATH order decides: with Elephant’s line at the end of ~/.zshrc, node, npm and npx are Elephant’s.",
    },
    footer: "Elephant doesn’t edit ~/.zshrc. You make the change.",
  },
  projects: {
    title: "Projects",
    footer: "Choosing a version writes .nvmrc (or .node-version, if the project already uses that) in its folder, like nvm.",
    selectLabel: "Node for {name}",
    byDefault: "Default ({version})",
    byDefaultNone: "Default",
    notInstalled: "{version} · not installed",
    fromFile: "{file}: {requested}",
  },
} satisfies Dictionary<typeof es>;
