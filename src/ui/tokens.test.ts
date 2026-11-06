import { describe, expect, it } from "vitest";

// @ts-expect-error: el proyecto no tiene @types/node (como vite.config.ts).
import { readFileSync } from "node:fs";

// El bloque claro de :root, leído del propio tokens.css (vitest vacía los CSS importados).
const css: string = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

function lightTokens(): Map<string, string> {
  const block = css.slice(css.indexOf(":root {"), css.indexOf("@media (prefers-color-scheme: dark)"));
  const tokens = new Map<string, string>();
  for (const [, name, value] of block.matchAll(/(--[\w-]+):\s*([^;]+);/g)) tokens.set(name, value.trim());
  return tokens;
}

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function luminance([r, g, b]: [number, number, number]): number {
  const f = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

function contrast(a: [number, number, number], b: [number, number, number]): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** El color mezclado al `amount` sobre blanco: el tinte de LevelPill. */
function tint(color: [number, number, number], amount: number): [number, number, number] {
  return color.map((c) => Math.round(c * amount + 255 * (1 - amount))) as [number, number, number];
}

describe("contraste de los tokens en modo claro", () => {
  const tokens = lightTokens();
  const white: [number, number, number] = [255, 255, 255];
  const text = ["--lvl-info", "--lvl-notice", "--lvl-warning", "--lvl-error", "--lvl-critical", "--http-2xx", "--http-4xx", "--http-5xx"];

  it.each(text)("%s pasa de 4,5:1 sobre blanco y sobre su tinte del 14 %%", (name) => {
    const value = tokens.get(name);
    expect(value, name).toMatch(/^#[0-9a-f]{6}$/i);
    const color = rgb(value!);
    expect(contrast(color, white)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color, tint(color, 0.14))).toBeGreaterThanOrEqual(4.5);
  });
});
