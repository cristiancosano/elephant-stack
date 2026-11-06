// setupFiles de vitest (vite.config.ts): todos los tests empiezan en español.
import { beforeEach } from "vitest";
import { setTestLocale } from "./testing";

setTestLocale("es");
beforeEach(() => setTestLocale("es"));
