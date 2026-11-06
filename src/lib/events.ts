// Eventos de Rust a la interfaz (00 §5.7). Ningún otro archivo importa
// `@tauri-apps/api/event`.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Appearance } from "./bindings/Appearance";
import type { NavGo } from "./bindings/NavGo";
import type { Snapshot } from "./bindings/Snapshot";
import type { UiCommand } from "./bindings/UiCommand";

export type { UnlistenFn };

/** Snapshot completo tras cada cambio; descarta los de revisión menor. */
export function onStateChanged(cb: (s: Snapshot) => void): Promise<UnlistenFn> {
  return listen<Snapshot>("state:changed", (event) => cb(event.payload));
}

/** Ruta pedida por el menú de la barra o el de la aplicación (`#/logs?…`). */
export function onNavGo(cb: (route: string) => void): Promise<UnlistenFn> {
  return listen<NavGo>("nav:go", (event) => cb(event.payload.route));
}

export function onUiCommand(cb: (c: UiCommand) => void): Promise<UnlistenFn> {
  return listen<UiCommand>("ui:command", (event) => cb(event.payload));
}

export function onAppearanceChanged(cb: (a: Appearance) => void): Promise<UnlistenFn> {
  return listen<Appearance>("appearance:changed", (event) => cb(event.payload));
}
