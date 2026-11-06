// Regla de seguir la cola (01 §7.5): como en Terminal, solo se sigue si estás abajo
// del todo. Al subir se deja de seguir y se cuentan las entradas nuevas para la
// píldora «N nuevas ↓». Volver abajo (con el scroll o con la píldora) lo reanuda.
import "../i18n/locales/logs";
import { t } from "../i18n/index.svelte";

export interface FollowState {
  following: boolean;
  /** Entradas llegadas mientras no se seguía. */
  unseen: number;
}

export const FOLLOWING: Readonly<FollowState> = Object.freeze({ following: true, unseen: 0 });

/** Han llegado `n` entradas por el final. */
export function appended(s: FollowState, n: number): FollowState {
  if (n <= 0 || s.following) return s;
  return { following: false, unseen: s.unseen + n };
}

/** El usuario ha movido el scroll: seguir solo si ha quedado abajo del todo. */
export function scrolled(s: FollowState, atBottom: boolean): FollowState {
  if (atBottom) return s.following && s.unseen === 0 ? s : { following: true, unseen: 0 };
  return s.following ? { following: false, unseen: 0 } : s;
}

/** Píldora «N nuevas ↓», «Ir al final» o limpiar la vista. */
export function jumpedToEnd(): FollowState {
  return { ...FOLLOWING };
}

/** Texto de la píldora, o `null` si no hay que mostrarla. */
export function unseenLabel(s: FollowState): string | null {
  if (s.following || s.unseen === 0) return null;
  if (s.unseen > 9999) return t("logs.pillMax");
  return t("logs.pill", { count: s.unseen });
}
