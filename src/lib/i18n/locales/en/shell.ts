// shell · inglés. Las mismas claves que es/shell.ts: si falta o sobra una, no compila.
import type es from "../es/shell";
import type { Dictionary } from "../../types";

export default {
  // La ola 2 lo rellena (docs/i18n.md).
} satisfies Dictionary<typeof es>;
