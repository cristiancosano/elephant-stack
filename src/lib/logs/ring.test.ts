import { describe, expect, it } from "vitest";

import { RING_CAPACITY, Ring } from "./ring";

const key = (n: number) => String(n);
const range = (from: number, to: number) => Array.from({ length: to - from }, (_, i) => from + i);

describe("Ring", () => {
  it("guarda en orden cronológico", () => {
    const r = new Ring<number>(5, key);
    expect(r.push([1, 2, 3])).toEqual({ added: 3, evicted: 0 });
    expect(r.toArray()).toEqual([1, 2, 3]);
    expect(r.first()).toBe(1);
    expect(r.last()).toBe(3);
    expect(r.at(3)).toBeUndefined();
    expect(r.at(-1)).toBeUndefined();
  });

  it("expulsa las más antiguas al llenarse", () => {
    const r = new Ring<number>(3, key);
    r.push([1, 2]);
    expect(r.push([3, 4, 5])).toEqual({ added: 3, evicted: 2 });
    expect(r.toArray()).toEqual([3, 4, 5]);
    expect(r.has("1")).toBe(false);
    expect(r.full).toBe(true);
  });

  it("no repite entradas que ya tiene", () => {
    const r = new Ring<number>(5, key);
    r.push([1, 2]);
    expect(r.push([2, 3, 3])).toEqual({ added: 1, evicted: 0 });
    expect(r.toArray()).toEqual([1, 2, 3]);
  });

  it("una entrada expulsada puede volver a entrar", () => {
    const r = new Ring<number>(2, key);
    r.push([1, 2, 3]);
    expect(r.unshift([1])).toBe(0); // lleno: el historial no expulsa
    r.clear();
    r.push([1]);
    expect(r.toArray()).toEqual([1]);
  });

  it("el historial entra por delante sin expulsar nada", () => {
    const r = new Ring<number>(5, key);
    r.push([10, 11, 12]);
    expect(r.unshift([7, 8, 9])).toBe(2); // solo caben 2: las más cercanas a lo que hay
    expect(r.toArray()).toEqual([8, 9, 10, 11, 12]);
  });

  it("mezcla historial y directo alrededor del índice de inicio", () => {
    const r = new Ring<number>(4, key);
    r.push([5]);
    r.unshift([3, 4]);
    r.push([6]);
    expect(r.toArray()).toEqual([3, 4, 5, 6]);
    r.push([7]);
    expect(r.toArray()).toEqual([4, 5, 6, 7]);
    expect(r.indexOf("6")).toBe(2);
    expect(r.indexOf("3")).toBe(-1);
  });

  it("aguanta el tope de 10 000 con lotes grandes", () => {
    const r = new Ring<number>(RING_CAPACITY, key);
    r.push(range(0, 12_000));
    expect(r.length).toBe(10_000);
    expect(r.first()).toBe(2_000);
    expect(r.last()).toBe(11_999);
    const { evicted } = r.push(range(12_000, 12_500));
    expect(evicted).toBe(500);
    expect(r.first()).toBe(2_500);
  });

  it("clear lo vacía todo", () => {
    const r = new Ring<number>(3, key);
    r.push([1, 2, 3]);
    r.clear();
    expect(r.length).toBe(0);
    expect(r.has("1")).toBe(false);
    expect(r.toArray()).toEqual([]);
  });

  it("rechaza capacidades no válidas", () => {
    expect(() => new Ring<number>(0, key)).toThrow(RangeError);
  });
});
