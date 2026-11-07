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
  composer: {
    title: "Composer",
    footer:
      "The official composer.phar from getcomposer.org, checked against its sha256. The composer selector runs it with each project’s PHP (.php-version). For PHP older than 7.2.5, use 2.2 LTS.",
    runsWith: "Runs with each project’s PHP · outside them, PHP {php}",
    runsWithNone: "Runs with each project’s PHP · there’s no Elephant PHP",
    lts: "2.2 LTS",
    legacy: "Installed by an earlier version of the setup assistant",
    notInstalled: "Not installed · {version} (PHP {php} or later)",
    unavailable: "Couldn’t check the versions on getcomposer.org",
    foreign: "~/.elephant-stack/bin/composer isn’t Elephant’s, so the terminal uses that one.",
    install: { action: "Install", busy: "Installing Composer", done: "Composer {version} is ready." },
    update: { action: "Update to {next}", busy: "Updating Composer to {next}", done: "Composer updated to {version}." },
    reinstall: "Reinstall",
    menu: {
      more: "More actions for Composer",
      toLts: "Switch to 2.2 LTS ({version})",
      toStable: "Switch to stable ({version})",
      reveal: "Show in Finder",
      uninstall: "Uninstall",
    },
    uninstall: { done: "Composer uninstalled." },
    path: "The terminal finds another composer first ({who}). Put this line at the end of ~/.zshrc: {line}",
    pathMissing: "~/.elephant-stack/bin isn’t in the terminal’s PATH. Put this line at the end of ~/.zshrc: {line}",
  },
  projects: {
    title: "Projects",
    footer: "Choosing a version writes .php-version in the project folder.",
    selectLabel: "PHP for {name}",
    notInstalled: "{version} · not installed",
    byDefault: "Default",
  },
} satisfies Dictionary<typeof es>;
