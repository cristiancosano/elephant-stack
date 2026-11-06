import { describe, expect, it } from "vitest";
import {
  clampInspectorWidth,
  dragInspectorWidth,
  INSPECTOR_DEFAULT,
  INSPECTOR_MAX,
  INSPECTOR_MIN,
} from "./inspector";

describe("clampInspectorWidth", () => {
  it("se queda entre 260 y 520", () => {
    expect(clampInspectorWidth(100)).toBe(INSPECTOR_MIN);
    expect(clampInspectorWidth(900)).toBe(INSPECTOR_MAX);
    expect(clampInspectorWidth(333.6)).toBe(334);
  });

  it("vuelve al ancho por defecto con valores no numéricos", () => {
    expect(clampInspectorWidth(Number.NaN)).toBe(INSPECTOR_DEFAULT);
    expect(clampInspectorWidth(Number.POSITIVE_INFINITY)).toBe(INSPECTOR_DEFAULT);
  });
});

describe("dragInspectorWidth", () => {
  it("ensancha al arrastrar hacia la izquierda y estrecha hacia la derecha", () => {
    expect(dragInspectorWidth(320, 600, 560)).toBe(360);
    expect(dragInspectorWidth(320, 600, 640)).toBe(280);
    expect(dragInspectorWidth(320, 600, 0)).toBe(INSPECTOR_MAX);
  });
});
