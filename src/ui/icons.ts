// Juego de iconos propio (01 §6.5): un .svg por icono en ./icons, rejilla de 16 px,
// trazo de 1,5 con extremos redondeados y `currentColor`. Se incrustan como texto.

export const ICON_NAMES = [
  // Barra lateral
  "panel",
  "sitios",
  "servicios",
  "php",
  "logs",
  "ajustes",
  // Acciones
  "play",
  "stop",
  "restart",
  "more",
  "search",
  "filter",
  "open",
  "folder",
  "terminal",
  "editor",
  "copy",
  "check",
  "x",
  // Estado
  "warning",
  "error",
  "info",
  "lock",
  // Componentes
  "globe",
  "mail",
  "database",
  "cache",
  // Navegación
  "chevron-right",
  "chevron-down",
  "sidebar",
  // Marca, asistente y canal de agentes
  "elephant",
  "sparkles",
  "agent",
] as const;

export type IconName = (typeof ICON_NAMES)[number];

const files = import.meta.glob<string>("./icons/*.svg", {
  query: "?raw",
  import: "default",
  eager: true,
});

const SVGS: Record<string, string> = {};
for (const [path, svg] of Object.entries(files)) {
  const name = path.slice("./icons/".length, -".svg".length);
  SVGS[name] = svg.trim();
}

/** Marcado SVG del icono, o una cadena vacía si no existe (no debería ocurrir: lo vigila un test). */
export function iconSvg(name: IconName): string {
  return SVGS[name] ?? "";
}

/** Nombres de los archivos que hay en ./icons, para el test de coherencia. */
export function iconFiles(): string[] {
  return Object.keys(SVGS).sort();
}
