import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dismiss, hold, MAX_TOASTS, release, resetToasts, toast, toasts, TOAST_DURATION_MS } from "./toasts.svelte";

beforeEach(() => {
  vi.useFakeTimers();
  resetToasts();
});

afterEach(() => {
  resetToasts();
  vi.useRealTimers();
});

describe("toast", () => {
  it("info y éxito se cierran solos a los 4 s", () => {
    toast("Guardado", { tone: "success" });
    toast("Copiado");
    expect(toasts.map((t) => t.tone)).toEqual(["success", "info"]);
    vi.advanceTimersByTime(TOAST_DURATION_MS - 1);
    expect(toasts).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(toasts).toHaveLength(0);
  });

  it("los errores se quedan hasta cerrarlos", () => {
    const id = toast("No se pudo detener mysql", { tone: "error" });
    vi.advanceTimersByTime(60_000);
    expect(toasts).toHaveLength(1);
    dismiss(id);
    expect(toasts).toHaveLength(0);
  });

  it("guarda la acción", () => {
    const onclick = vi.fn();
    toast("Servicio detenido", { action: { label: "Deshacer", onclick } });
    toasts[0].action?.onclick();
    expect(onclick).toHaveBeenCalledOnce();
  });

  it("un mensaje repetido sustituye al anterior", () => {
    toast("Copiado");
    vi.advanceTimersByTime(3000);
    toast("Copiado");
    expect(toasts).toHaveLength(1);
    vi.advanceTimersByTime(3000);
    expect(toasts).toHaveLength(1);
  });

  it(`muestra como mucho ${MAX_TOASTS}`, () => {
    for (let i = 0; i < 5; i++) toast(`Aviso ${i}`, { tone: "error" });
    expect(toasts.map((t) => t.message)).toEqual(["Aviso 2", "Aviso 3", "Aviso 4"]);
  });

  it("hold pausa el cierre y release lo reanuda con el tiempo completo", () => {
    const id = toast("Copiado");
    vi.advanceTimersByTime(3000);
    hold(id);
    vi.advanceTimersByTime(10_000);
    expect(toasts).toHaveLength(1);
    release(id);
    vi.advanceTimersByTime(TOAST_DURATION_MS - 1);
    expect(toasts).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(toasts).toHaveLength(0);
  });

  it("hold y release no afectan a los errores", () => {
    const id = toast("Falló", { tone: "error" });
    hold(id);
    release(id);
    vi.advanceTimersByTime(60_000);
    expect(toasts).toHaveLength(1);
  });
});
