// Texto que sale de la vista Logs: la línea que copia ⌘C, «Copiar» del inspector y
// «Copiar como Markdown» (02 §9), pensado para pegarlo en un issue o en un chat con
// un asistente. La traza se recorta como en C14: los 8 primeros frames propios más el
// primero de vendor; el resto se resume.

import type { EntryDetail } from "../bindings/EntryDetail";
import type { Frame } from "../bindings/Frame";
import type { LogEntry } from "../bindings/LogEntry";
import { formatDuration } from "../format";
import { hasContent, prettyJson } from "./json";
import { levelLabel, sourceLabel } from "./model";

/** Frames propios que se copian en Markdown (C14). */
export const MARKDOWN_OWN_FRAMES = 8;
/** Tope del cuerpo cuando no hay frames que mostrar. */
export const MARKDOWN_BODY_CHARS = 4_000;

export type FrameSegment = { kind: "own"; frame: Frame } | { kind: "vendor"; frames: Frame[] };

/** Agrupa los frames de `vendor/` seguidos en un bloque plegable (02 §9). */
export function foldFrames(frames: readonly Frame[]): FrameSegment[] {
  const out: FrameSegment[] = [];
  for (const frame of frames) {
    const last = out[out.length - 1];
    if (!frame.vendor) out.push({ kind: "own", frame });
    else if (last?.kind === "vendor") last.frames.push(frame);
    else out.push({ kind: "vendor", frames: [frame] });
  }
  return out;
}

export interface TrimmedFrames {
  shown: Frame[];
  omittedVendor: number;
  omittedOwn: number;
}

/** Los `ownMax` primeros frames propios y el primero de vendor, en su orden. */
export function trimFrames(frames: readonly Frame[], ownMax = MARKDOWN_OWN_FRAMES): TrimmedFrames {
  const shown: Frame[] = [];
  let own = 0;
  let vendorShown = false;
  let omittedVendor = 0;
  let omittedOwn = 0;
  for (const frame of frames) {
    if (frame.vendor) {
      if (vendorShown) omittedVendor++;
      else {
        vendorShown = true;
        shown.push(frame);
      }
    } else if (own < ownMax) {
      own++;
      shown.push(frame);
    } else {
      omittedOwn++;
    }
  }
  return { shown, omittedVendor, omittedOwn };
}

function pad(n: number, width = 2): string {
  return String(n).padStart(width, "0");
}

/** `2026-11-06 09:05:03.007`, en la hora local del Mac. */
export function formatTimestamp(ms: number): string {
  const d = new Date(ms);
  const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
  return `${date} ${time}`;
}

/** `GET /pedidos → 500 · 1,2 s` */
export function httpSummary(entry: LogEntry): string | null {
  const h = entry.http;
  if (!h) return null;
  const duration = h.duration_ms !== null ? ` · ${formatDuration(h.duration_ms)}` : "";
  return `${h.method} ${h.path} → ${h.status}${duration}`;
}

/** Una línea por entrada, como la ve la lista. Es lo que copia ⌘C sin el detalle. */
export function entryLine(entry: LogEntry, project: string): string {
  const message = httpSummary(entry) ?? entry.message;
  return `${formatTimestamp(entry.ts_ms)} ${levelLabel(entry.level).toUpperCase()} [${project}] ${message}`;
}

/** Valla de código que no choca con las comillas invertidas del contenido. */
export function fence(content: string, lang = ""): string {
  const longest = Math.max(2, ...Array.from(content.matchAll(/`+/g), (m) => m[0].length));
  const ticks = "`".repeat(longest + 1);
  return `${ticks}${lang}\n${content.replace(/\n+$/, "")}\n${ticks}`;
}

function frameLine(f: Frame): string {
  const where = f.line > 0 ? `${f.file}(${f.line})` : f.file;
  return `#${f.index} ${where}${f.call ? `: ${f.call}` : ""}`;
}

/** «Copiar como Markdown»: cabecera, mensaje, excepción, petición, traza recortada y contexto. */
export function entryMarkdown(detail: EntryDetail, project: string): string {
  const e = detail.entry;
  const parts: string[] = [];
  parts.push(`### ${levelLabel(e.level)} · ${project} · ${formatTimestamp(e.ts_ms)}`);

  const meta: string[] = [`**Fuente:** ${sourceLabel(e.kind)}${e.channel ? ` (canal \`${e.channel}\`)` : ""}`];
  if (e.host) meta.push(`**Host:** ${e.host}`);
  const http = httpSummary(e);
  if (http) meta.push(`**Petición:** \`${http}\``);
  parts.push(meta.join("  \n"));

  parts.push(fence(e.message));

  if (e.exception) {
    const x = e.exception;
    const where = x.file ? ` en \`${x.file}${x.line !== null ? `:${x.line}` : ""}\`` : "";
    parts.push(`**Excepción:** \`${x.class}\`${where}  \n${x.message}`);
  }

  if (detail.frames.length > 0) {
    const t = trimFrames(detail.frames);
    const lines = t.shown.map(frameLine);
    if (t.omittedOwn > 0) lines.push(`+${t.omittedOwn} frames propios más`);
    if (t.omittedVendor > 0) lines.push(`+${t.omittedVendor} frames de vendor`);
    parts.push(`**Traza**\n\n${fence(lines.join("\n"))}`);
  } else if (detail.body.trim() !== "" && detail.body.trim() !== e.message.trim()) {
    const body =
      detail.body.length > MARKDOWN_BODY_CHARS
        ? `${detail.body.slice(0, MARKDOWN_BODY_CHARS)}\n… (recortado)`
        : detail.body;
    parts.push(`**Entrada completa**\n\n${fence(body)}`);
  }

  if (hasContent(detail.context)) {
    parts.push(`**Contexto**\n\n${fence(prettyJson(detail.context).text, "json")}`);
  }

  return `${parts.join("\n\n")}\n`;
}
