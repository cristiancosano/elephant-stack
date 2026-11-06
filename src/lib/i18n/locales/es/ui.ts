// ui · Componentes de src/ui (etiquetas accesibles por defecto, toasts…).
// Español: la referencia. Las claves en inglés y camelCase; ver docs/i18n.md.
import type { Tree } from "../../types";

export default {
  toast: { close: "Cerrar aviso" },
  search: { clear: "Borrar búsqueda" },
  progress: { label: "En curso" },
  inspector: { label: "Inspector", resize: "Ancho del inspector" },
} as const satisfies Tree;
