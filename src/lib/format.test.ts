import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatClock,
  formatCpu,
  formatDuration,
  formatMemory,
  formatNumber,
  formatRelative,
  formatShortDate,
  shouldShowCpu,
} from "./format";

// Las salidas llevan espacio duro entre cifra y unidad; en los tests se escribe «~».
const s = (text: string) => text.replaceAll("~", " ");

describe("formatNumber", () => {
  it("agrupa a partir de cinco cifras, como pide la RAE", () => {
    expect(formatNumber(1234)).toBe("1234");
    expect(formatNumber(12345)).toBe("12.345");
    expect(formatNumber(1_000_000)).toBe("1.000.000");
  });
});

describe("formatMemory", () => {
  it("muestra MB enteros y GB con un decimal", () => {
    expect(formatMemory(0)).toBe(s("0~MB"));
    expect(formatMemory(500)).toBe(s("<1~MB"));
    expect(formatMemory(421_888)).toBe(s("412~MB"));
    expect(formatMemory(1023 * 1024)).toBe(s("1023~MB"));
    expect(formatMemory(1024 * 1024)).toBe(s("1,0~GB"));
    expect(formatMemory(1.25 * 1024 * 1024)).toBe(s("1,3~GB"));
  });

  it("no muestra valores negativos", () => {
    expect(formatMemory(-10)).toBe(s("0~MB"));
  });
});

describe("formatBytes", () => {
  it("usa base 1000 y un decimal solo por debajo de 10", () => {
    expect(formatBytes(0)).toBe(s("0~B"));
    expect(formatBytes(999)).toBe(s("999~B"));
    expect(formatBytes(1000)).toBe(s("1,0~KB"));
    expect(formatBytes(8_500_000)).toBe(s("8,5~MB"));
    expect(formatBytes(412_000_000)).toBe(s("412~MB"));
    expect(formatBytes(1_200_000_000)).toBe(s("1,2~GB"));
    expect(formatBytes(3_000_000_000_000)).toBe(s("3,0~TB"));
  });

  it("sube de unidad en vez de mostrar 1000", () => {
    expect(formatBytes(999_600)).toBe(s("1,0~MB"));
    expect(formatBytes(9_960)).toBe(s("10~KB"));
  });
});

describe("formatCpu", () => {
  it("redondea y evita el cero engañoso", () => {
    expect(formatCpu(0)).toBe(s("0~%"));
    expect(formatCpu(0.3)).toBe(s("<1~%"));
    expect(formatCpu(12.4)).toBe(s("12~%"));
    expect(formatCpu(250)).toBe(s("250~%"));
  });

  it("solo se muestra a partir del 5 %", () => {
    expect(shouldShowCpu(4.9)).toBe(false);
    expect(shouldShowCpu(5)).toBe(true);
  });
});

describe("formatDuration", () => {
  it("elige la unidad según la magnitud", () => {
    expect(formatDuration(84)).toBe(s("84~ms"));
    expect(formatDuration(1234)).toBe(s("1,2~s"));
    expect(formatDuration(42_000)).toBe(s("42~s"));
    expect(formatDuration(125_000)).toBe(s("2~min 5~s"));
    expect(formatDuration(180_000)).toBe(s("3~min"));
  });
});

describe("formatClock", () => {
  it("da HH:mm:ss.SSS en hora local", () => {
    expect(formatClock(new Date(2026, 10, 6, 9, 5, 3, 7))).toBe("09:05:03.007");
    expect(formatClock(new Date(2026, 10, 6, 23, 59, 59, 999).getTime())).toBe("23:59:59.999");
  });
});

describe("formatShortDate", () => {
  it("omite el año si es el actual", () => {
    const now = new Date(2026, 10, 6);
    expect(formatShortDate(new Date(2026, 8, 3), now)).toBe("3 sept");
    expect(formatShortDate(new Date(2025, 0, 15), now)).toBe("15 ene 2025");
  });
});

describe("formatRelative", () => {
  const now = new Date(2026, 10, 6, 12, 0, 0).getTime();
  const min = 60_000;
  const hour = 60 * min;

  it("describe el pasado reciente", () => {
    expect(formatRelative(now - 10_000, now)).toBe("ahora mismo");
    expect(formatRelative(now - 50_000, now)).toBe(s("hace 1~min"));
    expect(formatRelative(now - 5 * min, now)).toBe(s("hace 5~min"));
    expect(formatRelative(now - 2 * hour, now)).toBe(s("hace 2~h"));
  });

  it("usa días naturales para ayer y la última semana", () => {
    expect(formatRelative(new Date(2026, 10, 5, 23, 0).getTime(), now)).toBe("ayer");
    expect(formatRelative(new Date(2026, 10, 3, 18, 0).getTime(), now)).toBe("hace 3 días");
    expect(formatRelative(new Date(2026, 9, 20).getTime(), now)).toBe("20 oct");
  });

  it("describe el futuro cercano", () => {
    expect(formatRelative(now + 5 * min, now)).toBe(s("dentro de 5~min"));
    expect(formatRelative(now + 3 * hour, now)).toBe(s("dentro de 3~h"));
  });

  it("acepta Date", () => {
    expect(formatRelative(new Date(now - 5 * min), new Date(now))).toBe(s("hace 5~min"));
  });
});
