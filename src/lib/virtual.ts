// Cálculo puro de la ventana visible de una lista de filas de altura fija.
// Lo usa src/ui/VirtualList.svelte; aquí no hay DOM para poder probarlo.

/** Filas que se pintan de más por encima y por debajo de lo visible. */
export const OVERSCAN = 20;

export interface WindowInput {
  scrollTop: number;
  viewportHeight: number;
  rowHeight: number;
  count: number;
  overscan?: number;
}

export interface VirtualWindow {
  /** Primera fila que se pinta (incluida). */
  first: number;
  /** Fila siguiente a la última que se pinta (excluida): se pintan `first..last`. */
  last: number;
  /** Desplazamiento en px de la primera fila pintada. */
  offset: number;
  /** Alto total del contenido en px. */
  total: number;
}

export function totalHeight(count: number, rowHeight: number): number {
  return Math.max(0, count) * Math.max(0, rowHeight);
}

export function maxScrollTop(count: number, rowHeight: number, viewportHeight: number): number {
  return Math.max(0, totalHeight(count, rowHeight) - Math.max(0, viewportHeight));
}

export function visibleWindow(input: WindowInput): VirtualWindow {
  const { rowHeight, viewportHeight } = input;
  const count = Math.max(0, Math.floor(input.count));
  const overscan = Math.max(0, input.overscan ?? OVERSCAN);
  const total = totalHeight(count, rowHeight);
  if (count === 0 || rowHeight <= 0) return { first: 0, last: 0, offset: 0, total };

  const top = Math.min(Math.max(0, input.scrollTop), maxScrollTop(count, rowHeight, viewportHeight));
  const firstVisible = Math.floor(top / rowHeight);
  const lastVisible = Math.ceil((top + Math.max(0, viewportHeight)) / rowHeight);

  const first = Math.max(0, firstVisible - overscan);
  const last = Math.min(count, Math.max(lastVisible, firstVisible + 1) + overscan);
  return { first, last, offset: first * rowHeight, total };
}

/** ¿Está el scroll abajo del todo? Con tolerancia por el redondeo de subpíxeles. */
export function isAtBottom(
  scrollTop: number,
  viewportHeight: number,
  scrollHeight: number,
  tolerance = 2,
): boolean {
  return scrollHeight - (scrollTop + viewportHeight) <= tolerance;
}

/** ¿Está el scroll cerca del principio? Sirve para pedir más historial. */
export function isNearStart(scrollTop: number, rowHeight: number, rows = OVERSCAN): boolean {
  return scrollTop <= rowHeight * rows;
}

/**
 * ¿Se puede salir de la zona «cerca del principio» con el scroll? Si no (pocas
 * filas, o ninguna barra de desplazamiento), nunca llega la transición que pide
 * más historial y hay que pedirlo sin esperar al scroll.
 */
export function canLeaveStart(count: number, rowHeight: number, viewportHeight: number, rows = OVERSCAN): boolean {
  return !isNearStart(maxScrollTop(count, rowHeight, viewportHeight), rowHeight, rows);
}

/**
 * `scrollTop` mínimo que deja visible la fila `index` sin moverse si ya lo está.
 * Si queda por encima, la alinea arriba; si queda por debajo, abajo.
 */
export function scrollTopForIndex(
  index: number,
  rowHeight: number,
  viewportHeight: number,
  currentScrollTop: number,
): number {
  const top = Math.max(0, index) * rowHeight;
  const bottom = top + rowHeight;
  if (top < currentScrollTop) return top;
  if (bottom > currentScrollTop + viewportHeight) return Math.max(0, bottom - viewportHeight);
  return currentScrollTop;
}
