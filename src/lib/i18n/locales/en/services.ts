// services · inglés. Las mismas claves que es/services.ts: si falta o sobra una, no compila.
import type es from "../es/services";
import type { Dictionary } from "../../types";

export default {
  // La ola 2 lo rellena (docs/i18n.md).
} satisfies Dictionary<typeof es>;
