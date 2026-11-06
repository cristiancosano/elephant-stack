// Registro de las áreas de texto (docs/i18n.md). Cada área es un archivo por
// idioma, es/<área>.ts y en/<área>.ts, con un único dueño en cada ola.
//
// - El español de las áreas que usa la ventana al abrirse va en el chunk inicial
//   (`eagerEs`).
// - El de las áreas diferidas viaja con su vista: la vista importa
//   `locales/<área>.ts`, que lo registra. Así no engorda el chunk inicial (01 §11).
// - El inglés va entero en un chunk aparte (en/index.ts) y solo se descarga si
//   hace falta (`loaders`).
//
// Los tipos salen de todas las áreas a la vez (`import type` no carga nada).

import type agents from "./es/agents";
import type logs from "./es/logs";
import type sites from "./es/sites";
import type wizard from "./es/wizard";
import common from "./es/common";
import panel from "./es/panel";
import php from "./es/php";
import services from "./es/services";
import settings from "./es/settings";
import shell from "./es/shell";
import ui from "./es/ui";
import type { Locale } from "../locale.svelte";
import type { Dictionary } from "../types";

export const AREAS = [
  "common",
  "shell",
  "ui",
  "panel",
  "services",
  "php",
  "sites",
  "logs",
  "wizard",
  "settings",
  "agents",
] as const;

export type Area = (typeof AREAS)[number];

/** Áreas cuyo español no va en el chunk inicial: las registra su vista. */
export const DEFERRED_AREAS = ["sites", "logs", "wizard", "agents"] as const satisfies readonly Area[];

export type DeferredArea = (typeof DEFERRED_AREAS)[number];

/** Todos los textos en español: la referencia de tipos de `t()`. */
export interface Messages {
  common: typeof common;
  shell: typeof shell;
  ui: typeof ui;
  panel: typeof panel;
  services: typeof services;
  php: typeof php;
  sites: typeof sites;
  logs: typeof logs;
  wizard: typeof wizard;
  settings: typeof settings;
  agents: typeof agents;
}

export const eagerEs: Omit<Messages, DeferredArea> = { common, shell, ui, panel, services, php, settings };

/** Los idiomas que no son la referencia, cada uno en su propio chunk. */
export const loaders: Record<Exclude<Locale, "es">, () => Promise<{ default: Dictionary<Messages> }>> = {
  en: () => import("./en/index"),
};
