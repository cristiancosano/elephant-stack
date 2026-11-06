// Ruta actual de la ventana, ligada al hash de la URL.
// `nav:go` (events.ts) y los enlaces `<a href="#/…">` acaban aquí.

import { DEFAULT_ROUTE, parse, sameRoute, serialize, toRoute, withParams, type Route } from "./route";

const hasWindow = typeof window !== "undefined";

export const router: { route: Route } = $state({
  route: hasWindow ? parse(window.location.hash) : DEFAULT_ROUTE,
});

if (hasWindow) {
  window.addEventListener("hashchange", () => {
    const next = parse(window.location.hash);
    if (!sameRoute(next, router.route)) router.route = next;
  });
}

function commit(next: Route, replace: boolean) {
  if (sameRoute(next, router.route)) return;
  router.route = next;
  if (!hasWindow) return;
  const hash = serialize(next);
  if (replace) {
    // replaceState no emite hashchange: la ruta ya está actualizada arriba.
    window.history.replaceState(window.history.state, "", hash);
  } else {
    window.location.hash = hash;
  }
}

/** Navega a un hash (`#/logs?level=error`), un nombre de vista o una ruta. */
export function go(target: string | Route): void {
  commit(toRoute(target), false);
}

/**
 * Cambia parámetros de la ruta actual. `null` quita la clave. Con `replace`, no deja
 * entrada en el historial (útil para filtros que cambian al escribir).
 */
export function setParams(p: Record<string, string | null>, opts?: { replace?: boolean }): void {
  commit(withParams(router.route, p), opts?.replace ?? false);
}
