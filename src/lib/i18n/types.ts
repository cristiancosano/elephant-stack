// Tipos de los diccionarios (docs/i18n.md). El español es la referencia: sus
// diccionarios se declaran `as const`, y de ellos salen las claves válidas de
// `t()` y los parámetros que pide cada texto. El inglés se declara con
// `satisfies Dictionary<typeof es>`: si le falta o le sobra una clave, no compila.

/**
 * Texto con plural, elegido con `Intl.PluralRules` a partir de `count`. `one` y
 * `other` son obligatorias; el resto, solo si el idioma las usa (si falta la que
 * toca, se usa `other`). Estas claves están reservadas: no se usan como grupos.
 */
export interface Plural {
  readonly one: string;
  readonly other: string;
  readonly zero?: string;
  readonly two?: string;
  readonly few?: string;
  readonly many?: string;
}

/** Forma de cualquier diccionario: textos, plurales y grupos anidados. */
export interface Tree {
  readonly [key: string]: string | Plural | Tree;
}

/** Misma forma que `T`, con cualquier texto en cada hoja: la de las traducciones. */
export type Dictionary<T> = {
  readonly [K in keyof T]: T[K] extends string
    ? string
    : T[K] extends Plural
      ? Plural
      : Dictionary<T[K]>;
};

/** Rutas con punto de todas las hojas: `"title" | "language.label" | …`. */
export type Paths<T, Prefix extends string = ""> = {
  [K in keyof T & string]: T[K] extends string | Plural ? `${Prefix}${K}` : Paths<T[K], `${Prefix}${K}.`>;
}[keyof T & string];

/** El valor que hay en una ruta. */
export type At<T, P extends string> = P extends `${infer Head}.${infer Rest}`
  ? Head extends keyof T
    ? At<T[Head], Rest>
    : never
  : P extends keyof T
    ? T[P]
    : never;

/** Nombres entre llaves de un texto: `"{name} se ha detenido"` → `"name"`. */
export type Placeholders<S> = S extends `${string}{${infer Name}}${infer Rest}` ? Name | Placeholders<Rest> : never;

/** Lo que se puede interpolar. Los números se formatean con el idioma actual. */
export type ParamValue = string | number;

/** Argumentos de `t()` después de la clave, según el texto. */
export type ArgsFor<V> = V extends Plural
  ? [params: { count: number } & { [P in Exclude<Placeholders<V[keyof V]>, "count">]: ParamValue }]
  : [Placeholders<V>] extends [never]
    ? []
    : [params: { [P in Placeholders<V>]: ParamValue }];
