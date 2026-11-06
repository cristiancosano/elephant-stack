// ui · inglés. Las mismas claves que es/ui.ts: si falta o sobra una, no compila.
import type es from "../es/ui";
import type { Dictionary } from "../../types";

export default {
  toast: { close: "Close notification" },
  search: { clear: "Clear search" },
  progress: { label: "In progress" },
  inspector: { label: "Inspector", resize: "Inspector width" },
} satisfies Dictionary<typeof es>;
