// Hoja «Añadirlo por mí»: el diff de `~/.zshrc` que devuelve `setup_shell_preview`
// (04 §4.7), clasificado línea a línea para pintarlo.

export type DiffKind = "add" | "remove" | "context" | "header" | "hunk";

export interface DiffLine {
  kind: DiffKind;
  text: string;
}

export function diffLines(diff: string): DiffLine[] {
  const lines = diff.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n");
  if (lines.length === 1 && lines[0] === "") return [];
  return lines.map((text): DiffLine => {
    if (text.startsWith("+++") || text.startsWith("---")) return { kind: "header", text };
    if (text.startsWith("@@")) return { kind: "hunk", text };
    if (text.startsWith("+")) return { kind: "add", text };
    if (text.startsWith("-")) return { kind: "remove", text };
    return { kind: "context", text };
  });
}

/** Lo que se copia con «Copiar»: las líneas añadidas, sin el «+», listas para pegar. */
export function addedBlock(diff: string): string {
  return diffLines(diff)
    .filter((l) => l.kind === "add")
    .map((l) => l.text.slice(1))
    .join("\n");
}
