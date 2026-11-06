// php · inglés. Las mismas claves que es/php.ts: si falta o sobra una, no compila.
import type es from "../es/php";
import type { Dictionary } from "../../types";

export default {
  title: "PHP",
  useIn: {
    action: "Use in project…",
    pickFolder: "Project folder that will use PHP {version}",
    done: "{name} will use PHP {version} (.php-version).",
  },
  footer: {
    offline: "Offline: the versions that aren’t installed are from the last check.",
    default:
      "The checked version is the default, used outside any project with a .php-version file. Versions that aren’t installed are downloaded from php-builds.",
  },
  install: {
    done: "PHP {version} is ready.",
    busy: "Installing PHP {version}",
    action: "Install",
  },
  update: {
    done: "PHP {old} updated to {next}.",
    doneWithProjects: {
      one: "PHP {old} updated to {next}, also in {count} project.",
      other: "PHP {old} updated to {next}, also in {count} projects.",
    },
    busy: "Updating PHP {version} to {next}",
    action: "Update to {next}",
  },
  empty: {
    title: "No PHP versions",
    message: "No versions are installed and the available ones couldn’t be checked.",
  },
  versions: { title: "Versions" },
  badge: { default: "Default" },
  installedDefault: "{source} · Default",
  remote: { notInstalled: "Not installed · {size}" },
  catalog: {
    errorTitle: "Couldn’t check the available versions",
    errorMessage: "Check the connection and try again in a few minutes.",
  },
  projects: {
    title: "Projects",
    footer: "Choosing a version writes .php-version in the project folder.",
    selectLabel: "PHP for {name}",
    notInstalled: "{version} · not installed",
    byDefault: "Default",
  },
} satisfies Dictionary<typeof es>;
