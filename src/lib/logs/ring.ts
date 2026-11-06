// Anillo de entradas de la vista Logs (01 §7.5): como mucho 10 000, en orden
// cronológico. Lo que llega en directo entra por el final y expulsa las más antiguas;
// el historial que se pide al subir entra por el principio solo mientras quede sitio.
// No es reactivo a propósito: la vista lleva un contador de versión aparte, para que
// Svelte no envuelva 10 000 objetos en proxies.

export const RING_CAPACITY = 10_000;

export class Ring<T> {
  readonly capacity: number;
  private readonly keyOf: (item: T) => string;
  private buf: (T | undefined)[];
  private head = 0;
  private size = 0;
  private keys = new Set<string>();

  constructor(capacity: number, keyOf: (item: T) => string) {
    if (!Number.isInteger(capacity) || capacity <= 0) throw new RangeError("La capacidad debe ser un entero positivo.");
    this.capacity = capacity;
    this.keyOf = keyOf;
    this.buf = new Array<T | undefined>(capacity);
  }

  get length(): number {
    return this.size;
  }

  get full(): boolean {
    return this.size === this.capacity;
  }

  /** Elemento `i` en orden cronológico (0 = el más antiguo). */
  at(i: number): T | undefined {
    if (i < 0 || i >= this.size) return undefined;
    return this.buf[(this.head + i) % this.capacity];
  }

  first(): T | undefined {
    return this.at(0);
  }

  last(): T | undefined {
    return this.at(this.size - 1);
  }

  has(key: string): boolean {
    return this.keys.has(key);
  }

  /** Posición de la entrada con esa clave, o -1. Lineal: solo para acciones del usuario. */
  indexOf(key: string): number {
    if (!this.keys.has(key)) return -1;
    for (let i = 0; i < this.size; i++) {
      const item = this.at(i);
      if (item !== undefined && this.keyOf(item) === key) return i;
    }
    return -1;
  }

  /**
   * Añade al final, en el orden dado, saltando las que ya están. Si no cabe, expulsa
   * las más antiguas. Devuelve cuántas se añadieron y cuántas se expulsaron.
   */
  push(items: readonly T[]): { added: number; evicted: number } {
    let added = 0;
    let evicted = 0;
    for (const item of items) {
      const key = this.keyOf(item);
      if (this.keys.has(key)) continue;
      if (this.size === this.capacity) {
        const old = this.buf[this.head];
        if (old !== undefined) this.keys.delete(this.keyOf(old));
        this.buf[this.head] = item;
        this.head = (this.head + 1) % this.capacity;
        evicted++;
      } else {
        this.buf[(this.head + this.size) % this.capacity] = item;
        this.size++;
      }
      this.keys.add(key);
      added++;
    }
    return { added, evicted };
  }

  /**
   * Añade al principio un bloque más antiguo, en orden cronológico. Nunca expulsa: si
   * no cabe entero, entran las más recientes del bloque (las que van pegadas a lo que
   * ya hay). Devuelve cuántas entraron.
   */
  unshift(items: readonly T[]): number {
    let added = 0;
    for (let k = items.length - 1; k >= 0 && this.size < this.capacity; k--) {
      const item = items[k];
      const key = this.keyOf(item);
      if (this.keys.has(key)) continue;
      this.head = (this.head - 1 + this.capacity) % this.capacity;
      this.buf[this.head] = item;
      this.size++;
      this.keys.add(key);
      added++;
    }
    return added;
  }

  clear(): void {
    this.buf = new Array<T | undefined>(this.capacity);
    this.head = 0;
    this.size = 0;
    this.keys = new Set();
  }

  toArray(): T[] {
    return Array.from({ length: this.size }, (_, i) => this.at(i) as T);
  }
}
