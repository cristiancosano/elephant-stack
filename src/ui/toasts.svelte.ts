// Cola de avisos (HUD) que pinta Toaster.svelte. Info y éxito se cierran solos a
// los 4 s; los errores se quedan hasta que se cierran a mano.

export type ToastTone = "info" | "success" | "error";

export interface ToastAction {
  label: string;
  onclick: () => void;
}

export interface Toast {
  id: number;
  message: string;
  tone: ToastTone;
  action?: ToastAction;
}

/** Tiempo en pantalla de los avisos que se cierran solos. */
export const TOAST_DURATION_MS = 4000;
/** Como mucho se ven tres: el más antiguo deja sitio al nuevo. */
export const MAX_TOASTS = 3;

export const toasts: Toast[] = $state([]);

let nextId = 1;
// Avisos que se cierran solos: id → temporizador (null mientras está en pausa).
const timers = new Map<number, ReturnType<typeof setTimeout> | null>();

export function dismiss(id: number): void {
  const timer = timers.get(id);
  if (timer) clearTimeout(timer);
  timers.delete(id);
  const index = toasts.findIndex((t) => t.id === id);
  if (index !== -1) toasts.splice(index, 1);
}

/** Muestra un aviso y devuelve su id. Un mensaje repetido sustituye al anterior. */
export function toast(message: string, opts: { tone?: ToastTone; action?: ToastAction } = {}): number {
  const tone = opts.tone ?? "info";
  const repeated = toasts.find((t) => t.message === message && t.tone === tone);
  if (repeated) dismiss(repeated.id);

  const id = nextId++;
  toasts.push({ id, message, tone, action: opts.action });
  while (toasts.length > MAX_TOASTS) dismiss(toasts[0].id);

  if (tone !== "error") {
    timers.set(
      id,
      setTimeout(() => dismiss(id), TOAST_DURATION_MS),
    );
  }
  return id;
}

/** Pausa el cierre automático mientras el puntero está encima o el aviso tiene el foco. */
export function hold(id: number): void {
  const timer = timers.get(id);
  if (!timer) return;
  clearTimeout(timer);
  timers.set(id, null);
}

/** Reanuda el cierre automático tras `hold`, con el tiempo completo. */
export function release(id: number): void {
  if (timers.get(id) !== null) return;
  timers.set(
    id,
    setTimeout(() => dismiss(id), TOAST_DURATION_MS),
  );
}

/** Solo para tests: vacía la cola y los temporizadores. */
export function resetToasts(): void {
  for (const timer of timers.values()) if (timer) clearTimeout(timer);
  timers.clear();
  toasts.length = 0;
}
