import { describe, expect, it } from "vitest";

import { FOLLOWING, appended, jumpedToEnd, scrolled, unseenLabel } from "./follow";

describe("seguir la cola", () => {
  it("siguiendo, lo nuevo no cuenta como sin ver", () => {
    expect(appended(FOLLOWING, 5)).toBe(FOLLOWING);
    expect(unseenLabel(FOLLOWING)).toBeNull();
  });

  it("al subir deja de seguir y cuenta lo que llega", () => {
    let s = scrolled(FOLLOWING, false);
    expect(s).toEqual({ following: false, unseen: 0 });
    expect(unseenLabel(s)).toBeNull();
    s = appended(s, 1);
    expect(unseenLabel(s)).toBe("1 nueva");
    s = appended(s, 2);
    expect(unseenLabel(s)).toBe("3 nuevas");
  });

  it("volver abajo del todo lo reanuda", () => {
    const s = appended(scrolled(FOLLOWING, false), 4);
    expect(scrolled(s, true)).toEqual({ following: true, unseen: 0 });
    expect(jumpedToEnd()).toEqual({ following: true, unseen: 0 });
  });

  it("moverse sin llegar abajo no reinicia el contador", () => {
    const s = appended(scrolled(FOLLOWING, false), 4);
    expect(scrolled(s, false)).toBe(s);
  });

  it("un lote vacío no cambia nada", () => {
    const s = scrolled(FOLLOWING, false);
    expect(appended(s, 0)).toBe(s);
  });

  it("formatea cifras grandes", () => {
    expect(unseenLabel({ following: false, unseen: 12_345 })).toBe("9999+ nuevas");
    expect(unseenLabel({ following: false, unseen: 2_500 })).toBe("2500 nuevas");
  });
});
