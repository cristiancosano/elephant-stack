// agents · inglés. Las mismas claves que es/agents.ts: si falta o sobra una, no compila.
import type es from "../es/agents";
import type { Dictionary } from "../../types";

export default {
  // La ola 2 lo rellena (docs/i18n.md).
} satisfies Dictionary<typeof es>;
