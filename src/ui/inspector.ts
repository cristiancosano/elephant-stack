// Medidas del Inspector (01 §6.4): 320 px por defecto, redimensionable entre 260 y 520.

export const INSPECTOR_DEFAULT = 320;
export const INSPECTOR_MIN = 260;
export const INSPECTOR_MAX = 520;
/** Paso de las flechas del teclado sobre el tirador; con Mayúsculas, el triple. */
export const INSPECTOR_STEP = 10;

export function clampInspectorWidth(width: number): number {
  if (!Number.isFinite(width)) return INSPECTOR_DEFAULT;
  return Math.round(Math.min(INSPECTOR_MAX, Math.max(INSPECTOR_MIN, width)));
}

/**
 * Ancho tras arrastrar el borde izquierdo: el panel está a la derecha, así que
 * mover el puntero hacia la izquierda (dx negativo) lo ensancha.
 */
export function dragInspectorWidth(startWidth: number, startX: number, x: number): number {
  return clampInspectorWidth(startWidth - (x - startX));
}
