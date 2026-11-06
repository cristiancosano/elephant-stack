// Formato de cifras y fechas para la interfaz, en español de España.
// Entre la cifra y la unidad va un espacio duro (U+00A0) para que no se separen.

const NBSP = " ";

const integer = new Intl.NumberFormat("es-ES", { maximumFractionDigits: 0 });
const oneDecimal = new Intl.NumberFormat("es-ES", {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});

/** CPU a partir de la cual se muestra el consumo de un servicio (como hasta ahora). */
export const CPU_NOTICE_PERCENT = 5;

/** `1234` → `1234`, `12345` → `12.345` (la RAE no agrupa números de cuatro cifras). */
export function formatNumber(n: number): string {
  return integer.format(n);
}

/**
 * Memoria residente en KiB, como la da `ps -o rss`. Base 1024, como Monitor de Actividad.
 * `420000` → `410 MB`; `1258291` → `1,2 GB`.
 */
export function formatMemory(kb: number): string {
  const mb = Math.max(0, kb) / 1024;
  if (mb >= 1024) return `${oneDecimal.format(mb / 1024)}${NBSP}GB`;
  if (mb > 0 && mb < 1) return `<1${NBSP}MB`;
  return `${integer.format(mb)}${NBSP}MB`;
}

const BYTE_UNITS = ["KB", "MB", "GB", "TB"] as const;

/**
 * Tamaño de archivo o descarga. Base 1000, como el Finder.
 * Una cifra decimal por debajo de 10 (`8,5 MB`) y ninguna a partir de ahí (`412 MB`).
 */
export function formatBytes(bytes: number): string {
  const b = Math.max(0, bytes);
  if (b < 1000) return `${integer.format(b)}${NBSP}B`;
  let value = b / 1000;
  let unit = 0;
  while (value >= 999.5 && unit < BYTE_UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const text = value < 9.95 ? oneDecimal.format(value) : integer.format(value);
  return `${text}${NBSP}${BYTE_UNITS[unit]}`;
}

/** `12.4` → `12 %`. Por debajo del 1 %, `<1 %` en lugar de un cero engañoso. */
export function formatCpu(percent: number): string {
  if (percent > 0 && percent < 1) return `<1${NBSP}%`;
  return `${integer.format(Math.max(0, percent))}${NBSP}%`;
}

export function shouldShowCpu(percent: number): boolean {
  return percent >= CPU_NOTICE_PERCENT;
}

/** Duración corta, por ejemplo de una petición HTTP: `84 ms`, `1,2 s`, `2 min 5 s`. */
export function formatDuration(ms: number): string {
  const v = Math.max(0, ms);
  if (v < 1000) return `${integer.format(v)}${NBSP}ms`;
  if (v < 10_000) return `${oneDecimal.format(v / 1000)}${NBSP}s`;
  if (v < 60_000) return `${integer.format(v / 1000)}${NBSP}s`;
  const totalSeconds = Math.round(v / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return seconds ? `${minutes}${NBSP}min ${seconds}${NBSP}s` : `${minutes}${NBSP}min`;
}

/**
 * `plural(1, "servicio")` → `1 servicio`; `plural(3, "servicio")` → `3 servicios`.
 * Si el plural no es regular (`versión` → `versiones`), se pasa explícito.
 */
export function plural(n: number, singular: string, pluralForm?: string): string {
  const word = n === 1 ? singular : (pluralForm ?? regularPlural(singular));
  return `${formatNumber(n)} ${word}`;
}

function regularPlural(word: string): string {
  if (/[aeiouáéó]$/i.test(word)) return `${word}s`;
  if (/z$/i.test(word)) return `${word.slice(0, -1)}ces`;
  if (/[sx]$/i.test(word)) return word;
  return `${word}es`;
}

function pad(n: number, width = 2): string {
  return String(n).padStart(width, "0");
}

/** Hora local con milisegundos, para las filas de log: `09:05:03.007`. */
export function formatClock(time: number | Date): string {
  const d = typeof time === "number" ? new Date(time) : time;
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

const MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"];

/** Fecha corta: `6 nov`, o `6 nov 2025` si no es de este año. */
export function formatShortDate(time: number | Date, now: number | Date = Date.now()): string {
  const d = typeof time === "number" ? new Date(time) : time;
  const ref = typeof now === "number" ? new Date(now) : now;
  const base = `${d.getDate()} ${MONTHS[d.getMonth()]}`;
  return d.getFullYear() === ref.getFullYear() ? base : `${base} ${d.getFullYear()}`;
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/**
 * Tiempo relativo: `ahora mismo`, `hace 5 min`, `hace 2 h`, `ayer`, `hace 3 días`
 * y, a partir de una semana, la fecha corta. El futuro cercano: `dentro de 5 min`.
 */
export function formatRelative(time: number | Date, now: number | Date = Date.now()): string {
  const t = typeof time === "number" ? time : time.getTime();
  const n = typeof now === "number" ? now : now.getTime();
  const diff = n - t;
  const abs = Math.abs(diff);
  const minute = 60_000;
  const hour = 60 * minute;

  if (abs < 45_000) return "ahora mismo";
  if (abs < hour) {
    const m = Math.max(1, Math.round(abs / minute));
    return diff >= 0 ? `hace ${m}${NBSP}min` : `dentro de ${m}${NBSP}min`;
  }
  if (diff < 0) {
    const h = Math.round(abs / hour);
    return h < 24 ? `dentro de ${h}${NBSP}h` : formatShortDate(t, n);
  }

  const days = Math.round((startOfDay(new Date(n)) - startOfDay(new Date(t))) / (24 * hour));
  if (abs < 24 * hour && days === 0) return `hace ${Math.round(abs / hour)}${NBSP}h`;
  if (days <= 1) return "ayer";
  if (days < 7) return `hace ${days} días`;
  return formatShortDate(t, n);
}
