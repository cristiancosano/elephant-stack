// Estado del asistente: la máquina pura de lib/wizard/machine.ts más las
// llamadas a Rust. Las vistas leen `wizard.s` y llaman a sus métodos; ninguna
// habla con ipc.ts directamente.

import * as ipc from "../ipc";
import type { Catalog } from "../bindings/Catalog";
import type { Database } from "../bindings/Database";
import type { Detection } from "../bindings/Detection";
import type { McpSnippets } from "../bindings/McpSnippets";
import type { Profile } from "../bindings/Profile";
import type { ScenarioCard } from "../bindings/ScenarioCard";
import type { ScenarioId } from "../bindings/ScenarioId";
import type { ShellPreview } from "../bindings/ShellPreview";
import { go } from "../router.svelte";
import {
  currentInput,
  initialState,
  needsProfile,
  reduce,
  type Draft,
  type Preset,
  type WizardEvent,
  type WizardState,
} from "../wizard/machine";
import { inputFromSetupFile, preselectedDatabase, presetInput } from "../wizard/scenario";
import { toast } from "../../ui/toasts.svelte";

/** Entradas por ruta: `#/asistente/escenario` (Cambiar de escenario) y `#/asistente/revisar`. */
export type WizardEntry = "escenario" | "revisar" | null;

export function entryOf(sub: string | null): WizardEntry {
  return sub === "escenario" || sub === "revisar" ? sub : null;
}

class WizardStore {
  /** Estado de la máquina. Inmutable: cada evento lo sustituye entero. */
  s: WizardState = $state.raw(initialState());

  detection: Detection | null = $state.raw(null);
  detectionError: string | null = $state(null);
  cards: ScenarioCard[] | null = $state.raw(null);
  cardsError: string | null = $state(null);
  /** Para las ramas de PHP de Personalizar; se pide la primera vez que hace falta. */
  catalog: Catalog | null = $state.raw(null);
  planError: string | null = $state(null);
  /** El bloque de PATH para `~/.zshrc`; `null` si no hace falta o no está disponible. */
  shell: ShellPreview | null = $state.raw(null);
  /** «Conectar agentes»: se oculta si `mcp_snippets` falla. */
  snippets: McpSnippets | null = $state.raw(null);
  /** Esperando un perfil para Personalizar, o el «No cambiar nada». */
  busy: "profile" | "skip" | null = $state(null);

  // Cada apertura del asistente es una sesión, y cada plan pedido lleva su número:
  // una respuesta que llega tarde (de otra sesión o de un plan anterior) se descarta.
  #session = 0;
  #planSeq = 0;
  #catalogRequested = false;

  dispatch(e: WizardEvent): void {
    this.s = reduce(this.s, e);
  }

  /** Hay una aplicación en marcha: abrir otra vez el asistente no la pierde. */
  get running(): boolean {
    return !!this.s.run && !this.s.run.finished;
  }

  /** Al montar la vista o cambiar de entrada. */
  async open(entry: WizardEntry): Promise<void> {
    if (this.running) return;
    const seq = ++this.#session;
    this.s = initialState();
    this.planError = null;
    this.shell = null;
    this.snippets = null;
    this.busy = null;
    this.#loadDetection(seq);
    this.#loadCards(seq);
    if (entry) await this.#enter(entry, seq);
  }

  async #enter(entry: Exclude<WizardEntry, null>, seq: number): Promise<void> {
    let file = null;
    try {
      file = (await ipc.setupStatus()).state;
    } catch {
      // Sin estado guardado se entra a la elección sin nada marcado.
    }
    if (seq !== this.#session) return;
    const input = inputFromSetupFile(file);
    const draft: Draft | null =
      file?.scenario === "custom" && file.custom
        ? { profile: file.custom, basedOn: null, database: null, dirty: true }
        : null;
    const screen = entry === "revisar" && input ? "review" : "scenario";
    this.dispatch({ type: "enter", screen, scenario: file?.scenario ?? null, database: file?.database ?? null, draft });
    if (screen === "review") this.#requestPlan();
  }

  #loadDetection(seq: number): void {
    this.detection = null;
    this.detectionError = null;
    ipc
      .setupDetect()
      .then((d) => {
        if (seq === this.#session) this.detection = d;
      })
      .catch((e: unknown) => {
        if (seq === this.#session) this.detectionError = ipc.errorMessage(e);
      });
  }

  #loadCards(seq: number): void {
    this.cards = null;
    this.cardsError = null;
    ipc
      .setupScenarios()
      .then((cards) => {
        if (seq === this.#session) this.cards = cards;
      })
      .catch((e: unknown) => {
        if (seq === this.#session) this.cardsError = ipc.errorMessage(e);
      });
  }

  retryCards(): void {
    this.#loadCards(this.#session);
  }

  ensureCatalog(): void {
    if (this.#catalogRequested) return;
    this.#catalogRequested = true;
    ipc
      .phpCatalog()
      .then((c) => (this.catalog = c))
      .catch(() => {
        // Sin catálogo, Personalizar enseña solo las versiones propias.
        this.#catalogRequested = false;
      });
  }

  /** La base de datos que muestra la tarjeta: la elegida o la preseleccionada. */
  get database(): Database {
    return this.s.database ?? preselectedDatabase(this.cards ?? []);
  }

  start(): void {
    this.dispatch({ type: "start" });
  }

  back(): void {
    this.planError = null;
    this.dispatch({ type: "back" });
  }

  select(id: ScenarioId): void {
    // «Laravel con servicios» se elige con la base de datos que la tarjeta marca.
    if (id === "laravel_services") this.dispatch({ type: "database", database: this.database });
    else this.dispatch({ type: "select", scenario: id });
  }

  setDatabase(database: Database): void {
    this.dispatch({ type: "database", database });
  }

  /** «Ajustar» en una tarjeta (`basedOn`) o «Personalizado» (`null`, desde el sistema). */
  async customize(basedOn: Preset | null): Promise<void> {
    if (this.busy) return;
    this.ensureCatalog();
    const database = basedOn === "laravel_services" ? this.database : null;
    if (!needsProfile(this.s, basedOn, database)) {
      this.dispatch({ type: "reopen_customize" });
      return;
    }
    const seq = this.#session;
    this.busy = "profile";
    try {
      const profile = await ipc.setupProfile(basedOn ? presetInput(basedOn, database) : null);
      if (seq === this.#session) this.dispatch({ type: "customize", profile, basedOn, database });
    } catch (e) {
      if (seq === this.#session) toast(`No se ha podido preparar la personalización: ${ipc.errorMessage(e)}`, { tone: "error" });
    } finally {
      if (seq === this.#session) this.busy = null;
    }
  }

  edit(profile: Profile): void {
    this.dispatch({ type: "edit", profile });
  }

  /** Continuar hacia la revisión: calcula el plan en Rust. */
  review(): void {
    this.dispatch({ type: "review" });
    if (this.s.screen === "review") this.#requestPlan();
  }

  retryPlan(): void {
    if (this.s.screen === "review") this.#requestPlan();
  }

  #requestPlan(): void {
    const input = currentInput(this.s);
    if (!input) return;
    const session = this.#session;
    const seq = ++this.#planSeq;
    const current = () => session === this.#session && seq === this.#planSeq;
    this.planError = null;
    this.#loadShell(session);
    ipc
      .setupPreview(input)
      .then((plan) => {
        if (current()) this.dispatch({ type: "plan_loaded", plan, input });
      })
      .catch((e: unknown) => {
        if (current()) this.planError = ipc.errorMessage(e);
      });
  }

  #loadShell(seq: number): void {
    ipc
      .setupShellPreview()
      .then((p) => {
        if (seq === this.#session) this.shell = p.needed ? p : null;
      })
      .catch(() => {
        if (seq === this.#session) this.shell = null;
      });
  }

  /** Tras «Añadirlo por mí»: se vuelve a comprobar si sigue haciendo falta. */
  refreshShell(): void {
    this.#loadShell(this.#session);
  }

  toggle(key: string, on: boolean): void {
    this.dispatch({ type: "toggle", key, on });
  }

  /** «Aplicar (N pasos)» o «Hecho». Con el plan vacío también se llama: así queda guardado el escenario. */
  async apply(): Promise<void> {
    const plan = this.s.plan;
    if (this.s.screen !== "review" || !plan) return;
    const accepted = this.s.accepted;
    this.dispatch({ type: "apply_started" });
    const seq = this.#session;
    try {
      const outcome = await ipc.setupApply(plan.id, accepted, (event) => {
        if (seq === this.#session) this.dispatch({ type: "progress", event });
      });
      if (seq !== this.#session) return;
      if (outcome.kind === "stale") {
        this.dispatch({ type: "apply_stale", plan: outcome.plan });
        this.#loadShell(seq);
        return;
      }
      this.dispatch({ type: "apply_done", report: outcome.report });
    } catch (e) {
      if (seq === this.#session) this.dispatch({ type: "apply_failed", error: ipc.errorMessage(e) });
    }
    if (seq !== this.#session) return;
    this.#loadShell(seq);
    this.#loadSnippets(seq);
  }

  async cancel(): Promise<void> {
    if (!this.running) return;
    this.dispatch({ type: "cancel_requested" });
    try {
      await ipc.setupCancel();
    } catch (e) {
      toast(`No se ha podido cancelar: ${ipc.errorMessage(e)}`, { tone: "error" });
    }
  }

  /** «Reintentar lo que falta»: un plan nuevo con la misma elección. */
  retry(): void {
    this.dispatch({ type: "retry" });
    if (this.s.screen === "review") this.#requestPlan();
  }

  #loadSnippets(seq: number): void {
    ipc
      .mcpSnippets()
      .then((sn) => {
        if (seq === this.#session) this.snippets = sn.claude_command.trim() ? sn : null;
      })
      .catch(() => {
        if (seq === this.#session) this.snippets = null;
      });
  }

  /** «Ya tengo mi entorno: no cambiar nada». No ejecuta ningún comando. */
  async skip(): Promise<void> {
    if (this.busy || this.running) return;
    this.busy = "skip";
    try {
      await ipc.setupSkip();
      this.close();
    } catch (e) {
      toast(`No se ha podido guardar: ${ipc.errorMessage(e)}`, { tone: "error" });
    } finally {
      this.busy = null;
    }
  }

  /** Sale del asistente al panel. */
  close(): void {
    if (this.running) return;
    go("#/panel");
  }
}

export const wizard = new WizardStore();

/** Copia al portapapeles y lo confirma con un aviso. */
export async function copyText(text: string, what: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast(`${what} copiado`, { tone: "success" });
  } catch {
    toast(`No se ha podido copiar ${what.toLowerCase()}`, { tone: "error" });
  }
}
