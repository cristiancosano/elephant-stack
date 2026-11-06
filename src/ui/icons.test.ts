import { describe, expect, it } from "vitest";
import { ICON_NAMES, iconFiles, iconSvg } from "./icons";

describe("iconos", () => {
  it("cada nombre tiene su archivo y no sobra ninguno", () => {
    expect(iconFiles()).toEqual([...ICON_NAMES].sort());
  });

  it("todos usan currentColor y la rejilla de 16 px", () => {
    for (const name of ICON_NAMES) {
      const svg = iconSvg(name);
      expect(svg.startsWith("<svg"), name).toBe(true);
      expect(svg, name).toContain('viewBox="0 0 16 16"');
      expect(svg, name).toContain('stroke="currentColor"');
      expect(svg, name).toContain('stroke-width="1.5"');
      expect(svg, name).not.toMatch(/#[0-9a-f]{3,6}/i);
    }
  });
});
