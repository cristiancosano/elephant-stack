// php · Vista PHP y lib/php-versions.
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  title: "PHP",
  useIn: {
    action: "Usar en proyecto…",
    pickFolder: "Carpeta del proyecto que usará PHP {version}",
    done: "{name} usará PHP {version} (.php-version).",
  },
  footer: {
    offline: "Sin conexión: las versiones no instaladas son las de la última consulta.",
    default:
      "La marcada es la versión por defecto, la que se usa fuera de cualquier proyecto con .php-version. Las no instaladas se descargan de php-builds.",
  },
  install: {
    done: "PHP {version} está listo.",
    busy: "Instalando PHP {version}",
    action: "Instalar",
  },
  update: {
    done: "PHP {old} actualizada a {next}.",
    doneWithProjects: {
      one: "PHP {old} actualizada a {next}, también en {count} proyecto.",
      other: "PHP {old} actualizada a {next}, también en {count} proyectos.",
    },
    busy: "Actualizando PHP {version} a {next}",
    action: "Actualizar a {next}",
  },
  empty: {
    title: "No hay ninguna versión de PHP",
    message: "No hay versiones instaladas y no se ha podido consultar las disponibles.",
  },
  versions: { title: "Versiones" },
  badge: { default: "Por defecto" },
  installedDefault: "{source} · Por defecto",
  remote: { notInstalled: "No instalada · {size}" },
  catalog: {
    errorTitle: "No se pudo consultar las versiones disponibles",
    errorMessage: "Comprueba la conexión y vuelve a intentarlo en unos minutos.",
  },
  composer: {
    title: "Composer",
    footer:
      "El composer.phar oficial de getcomposer.org, comprobado con su sha256. El selector composer lo ejecuta con el PHP de cada proyecto (.php-version). Para PHP anterior a 7.2.5, usa la 2.2 LTS.",
    runsWith: "Se ejecuta con el PHP de cada proyecto · fuera de ellos, PHP {php}",
    runsWithNone: "Se ejecuta con el PHP de cada proyecto · no hay ningún PHP de Elephant",
    lts: "2.2 LTS",
    legacy: "Instalado por una versión anterior del asistente",
    notInstalled: "No instalado · {version} (PHP {php} o posterior)",
    unavailable: "No se ha podido consultar las versiones de getcomposer.org",
    foreign: "~/.elephant-stack/bin/composer no es de Elephant: la terminal usa ese.",
    install: { action: "Instalar", busy: "Instalando Composer", done: "Composer {version} está listo." },
    update: { action: "Actualizar a {next}", busy: "Actualizando Composer a {next}", done: "Composer actualizado a {version}." },
    reinstall: "Reinstalar",
    menu: {
      more: "Más acciones de Composer",
      toLts: "Cambiar a la 2.2 LTS ({version})",
      toStable: "Cambiar a la estable ({version})",
      reveal: "Mostrar en Finder",
      uninstall: "Desinstalar",
    },
    uninstall: { done: "Composer desinstalado." },
    path: "La terminal encuentra antes otro composer ({who}). Pon esta línea al final de ~/.zshrc: {line}",
    pathMissing: "~/.elephant-stack/bin no está en el PATH de la terminal. Pon esta línea al final de ~/.zshrc: {line}",
  },
  projects: {
    title: "Proyectos",
    footer: "Elegir una versión escribe .php-version en la carpeta del proyecto.",
    selectLabel: "PHP de {name}",
    notInstalled: "{version} · no instalada",
    byDefault: "Por defecto",
  },
} as const satisfies Tree;
