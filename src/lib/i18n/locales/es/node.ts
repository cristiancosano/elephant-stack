// node · Vista Node y lib/node-versions (06, fase 3).
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  title: "Node",
  versions: { title: "Versiones" },
  badge: { default: "Por defecto" },
  lts: "LTS {name}",
  current: "Current",
  npm: "npm {version}",
  globals: "Globales: {list}",
  installedDefault: "Por defecto",
  remote: { notInstalled: "No instalada" },
  footer: {
    default:
      "La marcada es la versión por defecto, la que se usa fuera de cualquier proyecto con .nvmrc o .node-version. Las no instaladas se descargan de nodejs.org y se comprueban con su SHASUMS256.",
    offline: "Sin conexión: las versiones no instaladas son las de la última consulta.",
  },
  showAll: {
    action: { one: "Mostrar {count} versión más", other: "Mostrar {count} versiones más" },
    hint: "Versiones mayores sin soporte o que ya no son la Current.",
    less: "Mostrar solo las recomendadas",
  },
  install: {
    action: "Instalar",
    busy: "Instalando Node {version}",
    done: "Node {version} está listo. node, npm y npx ya lo usan.",
  },
  update: {
    action: "Actualizar a {next}",
    busy: "Actualizando Node {version} a {next}",
    done: "Node {old} actualizado a {next}.",
    doneWithProjects: {
      one: "Node {old} actualizado a {next}, también en {count} proyecto.",
      other: "Node {old} actualizado a {next}, también en {count} proyectos.",
    },
  },
  useIn: {
    action: "Usar en proyecto…",
    pickFolder: "Carpeta del proyecto que usará Node {version}",
    done: "{name} usará Node {spec} ({file}).",
  },
  menu: {
    more: "Más acciones de Node {version}",
    makeDefault: "Usar por defecto",
    reveal: "Mostrar en Finder",
    uninstall: "Desinstalar",
  },
  uninstall: { done: "Node {version} desinstalado." },
  defaultSet: "Node {spec} es ahora el de por defecto.",
  empty: {
    title: "No hay ninguna versión de Node",
    message: "No hay versiones instaladas y no se ha podido consultar las disponibles.",
  },
  catalog: {
    errorTitle: "No se pudo consultar las versiones disponibles",
    errorMessage: "Comprueba la conexión y vuelve a intentarlo en unos minutos.",
  },
  terminal: {
    title: "Terminal",
    shadowed: {
      title: "La terminal usa antes el Node de {who}",
      message: "En ~/.zshrc, {who} se carga después de la línea de Elephant y queda delante en el PATH. Mueve esta línea al final de ~/.zshrc y abre otra terminal:",
    },
    missing: {
      title: "La terminal no usa los selectores de Elephant",
      message: "~/.elephant-stack/bin no está en el PATH. Añade esta línea al final de ~/.zshrc y abre otra terminal:",
    },
    copy: "Copiar la línea",
    copied: "Línea copiada.",
    nvm: {
      title: "nvm está instalado",
      subtitle: "Elephant no lo usa ni toca {path}.",
      message: "En la terminal manda el orden del PATH: con la línea de Elephant al final de ~/.zshrc, node, npm y npx son los de Elephant.",
    },
    footer: "Elephant no edita ~/.zshrc: el cambio lo haces tú.",
  },
  projects: {
    title: "Proyectos",
    footer: "Elegir una versión escribe .nvmrc (o .node-version, si el proyecto ya usa ese) en su carpeta, como nvm.",
    selectLabel: "Node de {name}",
    byDefault: "Por defecto ({version})",
    byDefaultNone: "Por defecto",
    notInstalled: "{version} · no instalada",
    fromFile: "{file}: {requested}",
  },
} as const satisfies Tree;
