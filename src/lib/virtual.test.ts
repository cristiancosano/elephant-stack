import { describe, expect, it } from "vitest";
import {
  canLeaveStart,
  isAtBottom,
  isNearStart,
  maxScrollTop,
  scrollTopForIndex,
  totalHeight,
  visibleWindow,
} from "./virtual";

const base = { rowHeight: 20, viewportHeight: 400 };

describe("visibleWindow", () => {
  it("devuelve una ventana vacía sin filas", () => {
    expect(visibleWindow({ ...base, scrollTop: 0, count: 0 })).toEqual({
      first: 0,
      last: 0,
      offset: 0,
      total: 0,
    });
  });

  it("pinta lo visible más 20 filas de margen abajo al principio", () => {
    // 400 / 20 = 20 filas visibles; más 20 de margen.
    expect(visibleWindow({ ...base, scrollTop: 0, count: 10_000 })).toEqual({
      first: 0,
      last: 40,
      offset: 0,
      total: 200_000,
    });
  });

  it("añade margen por arriba y por abajo en mitad de la lista", () => {
    const w = visibleWindow({ ...base, scrollTop: 1000, count: 10_000 });
    expect(w).toEqual({ first: 30, last: 90, offset: 600, total: 200_000 });
  });

  it("cubre filas parcialmente visibles", () => {
    const w = visibleWindow({ ...base, scrollTop: 1010, count: 10_000, overscan: 0 });
    expect(w.first).toBe(50);
    expect(w.last).toBe(71);
  });

  it("no se sale del final", () => {
    const w = visibleWindow({ ...base, scrollTop: 999_999, count: 100 });
    expect(w.last).toBe(100);
    expect(w.first).toBe(60);
  });

  it("acota un scroll negativo (rebote elástico)", () => {
    expect(visibleWindow({ ...base, scrollTop: -50, count: 100 }).first).toBe(0);
  });

  it("menos filas que el alto visible", () => {
    expect(visibleWindow({ ...base, scrollTop: 0, count: 5 })).toEqual({
      first: 0,
      last: 5,
      offset: 0,
      total: 100,
    });
  });

  it("con alto de ventana 0 pinta al menos una fila más el margen", () => {
    const w = visibleWindow({ rowHeight: 20, viewportHeight: 0, scrollTop: 0, count: 100, overscan: 0 });
    expect(w.last).toBe(1);
  });
});

describe("alturas", () => {
  it("calcula el total y el scroll máximo", () => {
    expect(totalHeight(10, 20)).toBe(200);
    expect(maxScrollTop(10, 20, 150)).toBe(50);
    expect(maxScrollTop(5, 20, 150)).toBe(0);
  });
});

describe("isAtBottom e isNearStart", () => {
  it("tolera el redondeo de subpíxeles", () => {
    expect(isAtBottom(600, 400, 1000)).toBe(true);
    expect(isAtBottom(598.5, 400, 1000)).toBe(true);
    expect(isAtBottom(590, 400, 1000)).toBe(false);
  });

  it("detecta la cercanía al principio en filas", () => {
    expect(isNearStart(0, 20)).toBe(true);
    expect(isNearStart(400, 20)).toBe(true);
    expect(isNearStart(401, 20)).toBe(false);
    expect(isNearStart(50, 20, 2)).toBe(false);
  });
});

describe("scrollTopForIndex", () => {
  it("no se mueve si la fila ya se ve", () => {
    expect(scrollTopForIndex(10, 20, 400, 100)).toBe(100);
  });

  it("alinea arriba si la fila queda por encima", () => {
    expect(scrollTopForIndex(2, 20, 400, 100)).toBe(40);
  });

  it("alinea abajo si la fila queda por debajo", () => {
    // Fila 30: de 600 a 620; con 400 de alto, el scroll queda en 220.
    expect(scrollTopForIndex(30, 20, 400, 0)).toBe(220);
  });
});

describe("canLeaveStart", () => {
  it("con menos filas que el alto no hay scroll que pida historial", () => {
    // 5 entradas (100 px) en 400 px.
    expect(canLeaveStart(5, 20, 400)).toBe(false);
    expect(canLeaveStart(0, 20, 400)).toBe(false);
  });

  it("si el scroll no puede pasar de las 20 filas del principio, tampoco", () => {
    // 40 filas: el scroll llega como mucho a 400 px, dentro de la zona.
    expect(canLeaveStart(40, 20, 400)).toBe(false);
    expect(canLeaveStart(41, 20, 400)).toBe(true);
    expect(canLeaveStart(200, 20, 400)).toBe(true);
  });
});
