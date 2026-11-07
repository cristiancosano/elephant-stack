// Rutas de la ventana: un hash como `#/logs?project=acme&level=error`.
// Puro y sin dependencias para poder probarlo; el estado vive en router.svelte.ts.

export type View = "panel" | "sitios" | "servicios" | "php" | "node" | "logs" | "ajustes" | "asistente";

export interface Route {
  view: View;
  /** Segmento tras la vista: `general` en `#/ajustes/general`. */
  sub: string | null;
  params: Record<string, string>;
}

export const VIEWS: readonly View[] = [
  "panel",
  "sitios",
  "servicios",
  "php",
  "node",
  "logs",
  "ajustes",
  "asistente",
];

export const DEFAULT_ROUTE: Route = { view: "panel", sub: null, params: {} };

function isView(value: string): value is View {
  return (VIEWS as readonly string[]).includes(value);
}

function decode(part: string): string {
  try {
    return decodeURIComponent(part);
  } catch {
    return part;
  }
}

/**
 * Acepta `#/logs`, `#logs`, `/logs` o `logs`. Una vista desconocida devuelve el panel
 * sin parámetros, para no arrastrar filtros de otra vista.
 */
export function parse(hash: string): Route {
  const raw = hash.replace(/^#/, "").replace(/^\/+/, "");
  const q = raw.indexOf("?");
  const path = q === -1 ? raw : raw.slice(0, q);
  const query = q === -1 ? "" : raw.slice(q + 1);

  const segments = path.split("/").filter(Boolean).map(decode);
  const view = segments[0]?.toLowerCase() ?? "";
  if (!isView(view)) return { ...DEFAULT_ROUTE, params: {} };

  const params: Record<string, string> = {};
  for (const [key, value] of new URLSearchParams(query)) {
    if (key) params[key] = value;
  }
  const sub = segments.length > 1 ? segments.slice(1).join("/") : null;
  return { view, sub, params };
}

/** Inversa de `parse`. Los parámetros vacíos no se escriben. */
export function serialize(r: Route): string {
  let out = `#/${r.view}`;
  if (r.sub) out += `/${r.sub.split("/").map(encodeURIComponent).join("/")}`;
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(r.params)) {
    if (value !== "") query.append(key, value);
  }
  const qs = query.toString();
  return qs ? `${out}?${qs}` : out;
}

/** Aplica cambios de parámetros: `null` o `""` quitan la clave. */
export function withParams(r: Route, changes: Record<string, string | null>): Route {
  const params = { ...r.params };
  for (const [key, value] of Object.entries(changes)) {
    if (value === null || value === "") delete params[key];
    else params[key] = value;
  }
  return { ...r, params };
}

/** Normaliza un destino de `go`: un hash, una ruta o un nombre de vista. */
export function toRoute(target: string | Route): Route {
  return typeof target === "string" ? parse(target) : target;
}

export function sameRoute(a: Route, b: Route): boolean {
  return serialize(a) === serialize(b);
}
