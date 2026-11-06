// Estado de la vista Logs: el anillo de entradas, los grupos, la suscripción en directo
// y el detalle del inspector. Se crea uno por cada vez que se monta la vista y se
// suelta al desmontarla (01 §7.5: al salir se cancela la suscripción y se libera el
// búfer). Con la ventana oculta tampoco hay suscripción (C3).
//
// Toda la lógica que se puede probar sin Tauri está en src/lib/logs/*.ts; aquí solo
// se orquestan las llamadas y se descartan las respuestas que llegan tarde.

import type { EntryDetail } from "../bindings/EntryDetail";
import type { LogBatch } from "../bindings/LogBatch";
import type { LogEntry } from "../bindings/LogEntry";
import type { LogGroup } from "../bindings/LogGroup";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import { errorMessage, logsEntry, logsGroups, logsProjects, logsQuery, logsSubscribe, logsUnsubscribe } from "../ipc";
import { DEFAULT_FILTERS, toQuery, type Filters } from "../logs/filters";
import { FOLLOWING, appended, jumpedToEnd, scrolled, type FollowState } from "../logs/follow";
import { mergeIntoGroups, sortGroups } from "../logs/groups";
import { chronological, isNotAvailable } from "../logs/model";
import { RING_CAPACITY, Ring } from "../logs/ring";

export type LoadState = "loading" | "ready" | "error" | "unavailable";

export interface DetailState {
  id: string;
  data: EntryDetail | null;
  error: string | null;
  loading: boolean;
}

const noop = () => {};

export class LogsStore {
  /** No es reactivo: las filas leen `version` para saber cuándo repintar. */
  readonly ring = new Ring<LogEntry>(RING_CAPACITY, (e) => e.id);
  version = $state(0);
  count = $state(0);
  groups: LogGroup[] = $state.raw([]);
  state: LoadState = $state("loading");
  error: string | null = $state(null);
  /** Bytes leídos del disco para lo que se ve (02 §10: presupuesto de lectura). */
  scannedBytes = $state(0);
  /** Entradas que el directo descartó porque llegaban más rápido de lo que se pintaban. */
  dropped = $state(0);
  /** No queda historial que pedir (o el anillo está lleno, o se limpió la vista). */
  historyDone = $state(false);
  loadingOlder = $state(false);
  olderError: string | null = $state(null);
  live = $state(true);
  /** La suscripción está activa. */
  streaming = $state(false);
  streamError: string | null = $state(null);
  follow: FollowState = $state.raw({ ...FOLLOWING });
  detail: DetailState | null = $state.raw(null);

  projects: ProjectSummary[] = $state.raw([]);
  projectsState: LoadState = $state("loading");

  /** La vista compensa el scroll cuando entran (positivo) o salen (negativo) filas por arriba. */
  onShift: ((rows: number) => void) | null = null;
  /** Rust avisa de que un archivo se truncó o rotó. */
  onTruncated: ((fileKeys: string[]) => void) | null = null;

  private filters: Filters = DEFAULT_FILTERS;
  private gen = 0;
  private nextBefore: string | null = null;
  private subId: number | null = null;
  private subToken = 0;
  private subPending = false;
  private detailToken = 0;
  private visible = true;
  private disposed = false;

  private bump(): void {
    this.count = this.ring.length;
    this.version++;
  }

  private stale(gen: number): boolean {
    return gen !== this.gen || this.disposed;
  }

  get mode(): Filters["mode"] {
    return this.filters.mode;
  }

  // ── Carga ────────────────────────────────────────────────────────────────

  /** Vacía todo y vuelve a cargar con estos filtros. */
  async load(filters: Filters): Promise<void> {
    if (this.disposed) return;
    this.filters = filters;
    const gen = ++this.gen;
    this.unsubscribe();
    this.ring.clear();
    this.bump();
    this.groups = [];
    this.follow = jumpedToEnd();
    this.dropped = 0;
    this.nextBefore = null;
    this.historyDone = false;
    this.olderError = null;
    this.streamError = null;
    this.scannedBytes = 0;
    this.error = null;
    this.state = "loading";

    try {
      if (filters.mode === "grouped") {
        const groups = await logsGroups(toQuery(filters, Date.now()));
        if (this.stale(gen)) return;
        this.groups = sortGroups(groups.slice());
      } else {
        const page = await logsQuery(toQuery(filters, Date.now()));
        if (this.stale(gen)) return;
        this.ring.push(chronological(page.entries));
        this.nextBefore = page.next_before;
        this.historyDone = page.next_before === null;
        this.scannedBytes = page.scanned_bytes;
        this.bump();
      }
      this.state = "ready";
    } catch (e) {
      if (this.stale(gen)) return;
      const message = errorMessage(e);
      this.state = isNotAvailable(message) ? "unavailable" : "error";
      this.error = message;
      return;
    }
    void this.subscribe();
  }

  /** Historial anterior a lo más antiguo que hay, al acercarse arriba. */
  async loadOlder(): Promise<void> {
    if (
      this.filters.mode !== "chrono" ||
      this.state !== "ready" ||
      this.loadingOlder ||
      this.historyDone ||
      this.nextBefore === null
    ) {
      return;
    }
    if (this.ring.full) {
      this.historyDone = true;
      return;
    }
    const gen = this.gen;
    this.loadingOlder = true;
    this.olderError = null;
    try {
      const page = await logsQuery(toQuery(this.filters, Date.now(), this.nextBefore));
      if (this.stale(gen)) return;
      const added = this.ring.unshift(chronological(page.entries));
      this.nextBefore = page.next_before;
      this.historyDone = page.next_before === null || this.ring.full;
      this.scannedBytes += page.scanned_bytes;
      if (added > 0) {
        this.bump();
        this.onShift?.(added);
      }
    } catch (e) {
      if (!this.stale(gen)) this.olderError = errorMessage(e);
    } finally {
      if (!this.stale(gen)) this.loadingOlder = false;
    }
  }

  /**
   * Tras un rato sin suscripción (directo en pausa o ventana oculta): trae lo más
   * reciente y lo añade al final. El anillo descarta lo que ya tenía.
   */
  private async catchUp(): Promise<void> {
    if (this.state !== "ready") return;
    const gen = this.gen;
    try {
      if (this.filters.mode === "grouped") {
        const groups = await logsGroups(toQuery(this.filters, Date.now()));
        if (!this.stale(gen)) this.groups = sortGroups(groups.slice());
        return;
      }
      const page = await logsQuery(toQuery(this.filters, Date.now()));
      if (this.stale(gen)) return;
      const last = this.ring.last();
      const fresh = chronological(page.entries).filter((e) => !last || e.ts_ms >= last.ts_ms);
      this.append(fresh);
    } catch {
      // Sin red que perder: si falla, el directo sigue y la próxima carga lo arregla.
    }
  }

  private append(entries: LogEntry[]): void {
    if (entries.length === 0) return;
    const { added, evicted } = this.ring.push(entries);
    if (added === 0) return;
    this.bump();
    if (evicted > 0 && !this.follow.following) this.onShift?.(-evicted);
    this.follow = appended(this.follow, added);
  }

  // ── Directo ──────────────────────────────────────────────────────────────

  private wantsStream(): boolean {
    return this.live && this.visible && !this.disposed && this.state === "ready";
  }

  private async subscribe(): Promise<void> {
    if (!this.wantsStream() || this.subId !== null || this.subPending) return;
    const token = ++this.subToken;
    this.subPending = true;
    try {
      const id = await logsSubscribe(toQuery(this.filters, Date.now()), (batch) => {
        if (token === this.subToken) this.onBatch(batch);
      });
      if (token !== this.subToken) {
        // Se canceló mientras llegaba la respuesta.
        void logsUnsubscribe(id).catch(noop);
        return;
      }
      this.subPending = false;
      this.subId = id;
      this.streaming = true;
      this.streamError = null;
    } catch (e) {
      if (token !== this.subToken) return;
      this.subPending = false;
      this.streaming = false;
      const message = errorMessage(e);
      // Sin directo en el motor: la lista funciona igual, solo sin «En vivo».
      this.streamError = isNotAvailable(message) ? null : message;
    }
  }

  private unsubscribe(): void {
    this.subToken++;
    this.subPending = false;
    this.streaming = false;
    const id = this.subId;
    this.subId = null;
    if (id !== null) void logsUnsubscribe(id).catch(noop);
  }

  private onBatch(batch: LogBatch): void {
    if (this.disposed) return;
    if (batch.dropped > 0) this.dropped += batch.dropped;
    if (batch.truncated.length > 0) this.onTruncated?.(batch.truncated);
    if (batch.entries.length === 0) return;
    const entries = chronological(batch.entries);
    if (this.filters.mode === "grouped") this.groups = mergeIntoGroups(this.groups, entries);
    else this.append(entries);
  }

  setLive(on: boolean): void {
    if (this.live === on) return;
    this.live = on;
    if (!on) {
      this.unsubscribe();
      return;
    }
    this.follow = jumpedToEnd();
    void this.catchUp().then(() => this.subscribe());
  }

  /** `visibilitychange`: con la ventana oculta no queda suscripción ni sondeo (C3). */
  setVisible(visible: boolean): void {
    if (this.visible === visible) return;
    this.visible = visible;
    if (!visible) {
      this.unsubscribe();
      return;
    }
    if (this.live) void this.catchUp().then(() => this.subscribe());
  }

  // ── Vista ────────────────────────────────────────────────────────────────

  /** ⌘K «Limpiar vista» (C20): vacía la lista, no el archivo. El directo sigue. */
  clearView(): void {
    this.ring.clear();
    this.bump();
    this.groups = [];
    this.follow = jumpedToEnd();
    this.dropped = 0;
    // Como ⌘K en Terminal: lo limpiado no vuelve al subir.
    this.nextBefore = null;
    this.historyDone = true;
  }

  scrolled(atBottom: boolean): void {
    this.follow = scrolled(this.follow, atBottom);
  }

  jumpToEnd(): void {
    this.follow = jumpedToEnd();
  }

  /** Entrada `i` del anillo; lee `version` para que la fila se repinte cuando cambia. */
  entryAt(i: number): LogEntry | undefined {
    void this.version;
    return this.ring.at(i);
  }

  // ── Detalle ──────────────────────────────────────────────────────────────

  async showDetail(id: string | null): Promise<void> {
    if (id === null) {
      this.detailToken++;
      this.detail = null;
      return;
    }
    const current = this.detail;
    if (current?.id === id && (current.loading || current.data)) return;
    const token = ++this.detailToken;
    this.detail = { id, data: null, error: null, loading: true };
    try {
      const data = await logsEntry(id);
      if (token === this.detailToken) this.detail = { id, data, error: null, loading: false };
    } catch (e) {
      if (token === this.detailToken) this.detail = { id, data: null, error: errorMessage(e), loading: false };
    }
  }

  /** El detalle completo de una entrada: el del inspector si es esa, o uno nuevo. */
  async fetchDetail(id: string): Promise<EntryDetail> {
    const current = this.detail;
    if (current?.id === id && current.data) return current.data;
    return logsEntry(id);
  }

  // ── Proyectos ────────────────────────────────────────────────────────────

  async loadProjects(): Promise<void> {
    try {
      const list = await logsProjects();
      if (this.disposed) return;
      this.projects = list;
      this.projectsState = "ready";
    } catch (e) {
      if (this.disposed) return;
      this.projectsState = isNotAvailable(errorMessage(e)) ? "unavailable" : "error";
    }
  }

  setProjects(list: ProjectSummary[]): void {
    this.projects = list;
    this.projectsState = "ready";
  }

  /** Al desmontar la vista: sin suscripción y sin búfer. */
  dispose(): void {
    this.disposed = true;
    this.gen++;
    this.detailToken++;
    this.unsubscribe();
    this.ring.clear();
    this.groups = [];
    this.detail = null;
    this.onShift = null;
    this.onTruncated = null;
  }
}

export function createLogsStore(): LogsStore {
  return new LogsStore();
}
