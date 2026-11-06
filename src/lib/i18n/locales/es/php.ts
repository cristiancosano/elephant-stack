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
  projects: {
    title: "Proyectos",
    footer: "Elegir una versión escribe .php-version en la carpeta del proyecto.",
    selectLabel: "PHP de {name}",
    notInstalled: "{version} · no instalada",
    byDefault: "Por defecto",
  },
} as const satisfies Tree;
