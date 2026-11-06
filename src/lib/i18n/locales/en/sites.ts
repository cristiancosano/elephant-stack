// sites · inglés. Las mismas claves que es/sites.ts: si falta o sobra una, no compila.
import type es from "../es/sites";
import type { Dictionary } from "../../types";

export default {
  // La ola 2 lo rellena (docs/i18n.md).
} satisfies Dictionary<typeof es>;
