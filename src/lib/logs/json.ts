// Resaltado mínimo de JSON para el contexto del inspector (01 §7.5): claves, cadenas,
// números y literales, sin librería. El tokenizador es tolerante: un texto que no es
// JSON válido sale entero, troceado en tokens `text` y `punct`, sin perder caracteres.

export type TokenKind = "key" | "string" | "number" | "literal" | "punct" | "space" | "text";

export interface Token {
  kind: TokenKind;
  text: string;
}

/** Tope del texto formateado: más allá, el inspector se volvería lento. */
export const MAX_PRETTY_CHARS = 200_000;

const NUMBER = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/;
const LITERAL = /^(?:true|false|null)\b/;
const PUNCT = "{}[],:";

/** Fin de la cadena que empieza en `start` (comilla incluida), o el final del texto. */
function stringEnd(text: string, start: number): number {
  let i = start + 1;
  while (i < text.length) {
    const c = text[i];
    if (c === "\\") i += 2;
    else if (c === '"') return i + 1;
    else if (c === "\n") return i; // cadena sin cerrar: se corta en el salto de línea
    else i++;
  }
  return text.length;
}

/** Una cadena es clave si lo siguiente que no es espacio es `:`. */
function followedByColon(text: string, from: number): boolean {
  for (let i = from; i < text.length; i++) {
    const c = text[i];
    if (c === ":") return true;
    if (c !== " " && c !== "\t" && c !== "\n" && c !== "\r") return false;
  }
  return false;
}

export function tokenize(text: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  const push = (kind: TokenKind, value: string) => {
    const last = tokens[tokens.length - 1];
    // Los `text` seguidos se juntan para no generar un nodo por carácter.
    if (kind === "text" && last?.kind === "text") last.text += value;
    else tokens.push({ kind, text: value });
  };

  while (i < text.length) {
    const c = text[i];
    if (c === " " || c === "\n" || c === "\t" || c === "\r") {
      let j = i + 1;
      while (j < text.length && " \n\t\r".includes(text[j])) j++;
      push("space", text.slice(i, j));
      i = j;
    } else if (c === '"') {
      const end = stringEnd(text, i);
      push(followedByColon(text, end) ? "key" : "string", text.slice(i, end));
      i = end;
    } else if (PUNCT.includes(c)) {
      push("punct", c);
      i++;
    } else {
      const rest = text.slice(i, i + 400);
      const literal = LITERAL.exec(rest);
      const number = literal ? null : NUMBER.exec(rest);
      const match = literal ?? number;
      if (match) {
        push(literal ? "literal" : "number", match[0]);
        i += match[0].length;
      } else {
        push("text", c);
        i++;
      }
    }
  }
  return tokens;
}

/** JSON con sangría de 2 espacios, recortado a `MAX_PRETTY_CHARS`. */
export function prettyJson(value: unknown): { text: string; truncated: boolean } {
  let text: string;
  try {
    text = JSON.stringify(value, null, 2) ?? String(value);
  } catch {
    text = String(value);
  }
  if (text.length <= MAX_PRETTY_CHARS) return { text, truncated: false };
  return { text: text.slice(0, MAX_PRETTY_CHARS), truncated: true };
}

/** ¿Merece la pena mostrar el contexto? `{}` y `[]` no aportan nada. */
export function hasContent(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === "object") return Object.keys(value).length > 0;
  return true;
}
