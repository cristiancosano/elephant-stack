<script lang="ts">
  // Ajustes ▸ Logs (00 §6, P07): carpetas en las que se buscan proyectos y, por
  // proyecto, la zona horaria de sus logs de Laravel y rutas de logs extra
  // (`~/.elephant-stack/projects.json`, 02 §4.4). Los cambios se guardan al momento.
  // Textos del área diferida «logs» (docs/i18n.md): viajan con esta vista.
  import "../../lib/i18n/locales/logs";
  import { onMount } from "svelte";

  import type { ProjectSummary } from "../../lib/bindings/ProjectSummary";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import { plural } from "../../lib/format";
  import {
    errorMessage,
    logsAddRoot,
    logsProjects,
    logsRemoveRoot,
    logsRoots,
    logsSetOverride,
    pickFolder,
  } from "../../lib/ipc";
  import { isNotAvailable, originLabel, sourceLabel } from "../../lib/logs/model";
  import {
    buildOverride,
    groupRoots,
    guessHome,
    knownTimezones,
    overrideFields,
    parseGlobs,
    tildePath,
    validateTimezone,
    type RootFolder,
  } from "../../lib/logs/settings";
  import { go } from "../../lib/router.svelte";
  import Button from "../../ui/Button.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import Group from "../../ui/Group.svelte";
  import Row from "../../ui/Row.svelte";
  import Sheet from "../../ui/Sheet.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";
  import { toast } from "../../ui/toasts.svelte";

  let { snapshot }: { snapshot: Snapshot | null } = $props();

  let summaries = $state.raw<ProjectSummary[]>([]);
  let rootPaths = $state.raw<string[]>([]);
  let status = $state<"loading" | "ready" | "unavailable" | "error">("loading");
  let loadError = $state<string | null>(null);
  let busyRoot = $state<string | null>(null);
  let adding = $state(false);

  const roots = $derived(groupRoots(rootPaths, summaries.map((s) => s.project)));
  const home = $derived(guessHome([...summaries.map((s) => s.project.root), ...(snapshot?.projects.map((p) => p.root) ?? [])]));

  /** Las carpetas no van en el resumen: se piden aparte tras cada cambio. */
  async function loadRoots() {
    try {
      rootPaths = await logsRoots();
    } catch {
      rootPaths = [];
    }
  }

  async function load() {
    try {
      [summaries] = await Promise.all([logsProjects(), loadRoots()]);
      status = "ready";
    } catch (e) {
      const message = errorMessage(e);
      status = isNotAvailable(message) ? "unavailable" : "error";
      loadError = message;
    }
  }

  onMount(() => void load());

  function showError(e: unknown, unavailable: string) {
    const message = errorMessage(e);
    toast(isNotAvailable(message) ? unavailable : message, { tone: "error" });
  }

  async function addRoot() {
    const path = await pickFolder("Elige una carpeta con proyectos").catch(() => null);
    if (!path) return;
    adding = true;
    try {
      const before = new Set(summaries.map((s) => s.project.id));
      summaries = await logsAddRoot(path);
      await loadRoots();
      status = "ready";
      const found = summaries.filter((s) => !before.has(s.project.id)).length;
      toast(found > 0 ? `Carpeta añadida: ${plural(found, "proyecto nuevo", "proyectos nuevos")}.` : "Carpeta añadida. No se han encontrado proyectos nuevos.", {
        tone: "success",
      });
    } catch (e) {
      showError(e, "Todavía no se pueden añadir carpetas.");
    } finally {
      adding = false;
    }
  }

  async function removeRoot(folder: RootFolder) {
    busyRoot = folder.path;
    try {
      summaries = await logsRemoveRoot(folder.path);
      await loadRoots();
      toast(`Se ha dejado de buscar proyectos en ${tildePath(folder.path, home)}.`, { tone: "success" });
    } catch (e) {
      showError(e, "Todavía no se pueden quitar carpetas.");
    } finally {
      busyRoot = null;
    }
  }

  // ── Ajustes de un proyecto ────────────────────────────────────────────────

  let editing = $state.raw<ProjectSummary | null>(null);
  let timezone = $state("");
  let globsText = $state("");
  let saving = $state(false);
  let zones = $state.raw<string[]>([]);

  const tzError = $derived(editing ? validateTimezone(timezone, zones) : null);
  const globErrors = $derived(editing ? parseGlobs(globsText).errors : []);
  const valid = $derived(tzError === null && globErrors.length === 0);

  function edit(s: ProjectSummary) {
    if (zones.length === 0) zones = knownTimezones();
    ({ timezone, globsText } = overrideFields(s.override));
    editing = s;
  }

  async function save() {
    const target = editing;
    if (!target || !valid) return;
    saving = true;
    try {
      summaries = await logsSetOverride(target.project.id, buildOverride(timezone, globsText));
      toast(`Guardado para ${target.project.name}.`, { tone: "success" });
      editing = null;
    } catch (e) {
      showError(e, "Todavía no se pueden guardar ajustes de proyecto.");
    } finally {
      saving = false;
    }
  }

  function sourceState(s: ProjectSummary["sources"][number]): string {
    if (s.state.kind === "missing") return "no existe";
    if (s.state.kind === "unreadable") return s.state.message;
    return plural(s.files.length, "archivo");
  }
</script>

<div class="tab">
  {#if status === "loading"}
    <Skeleton lines={6} />
  {:else if status === "unavailable"}
    <EmptyState icon="logs" title="Los ajustes de logs todavía no están disponibles" message="Esta versión de Elephant Stack aún no lee los logs de tus proyectos." />
  {:else if status === "error"}
    <EmptyState icon="warning" title="No se han podido leer los proyectos" message={loadError ?? undefined} action={{ label: "Reintentar", onclick: () => void load() }} />
  {:else}
    <Group
      title="Carpetas de proyectos"
      footer="Además de los sitios de Herd, Elephant busca proyectos en estas carpetas, hasta dos niveles por debajo. Solo lee: no cambia nada en ellas."
    >
      {#each roots as folder (folder.path)}
        <Row title={tildePath(folder.path, home)} subtitle={folder.projects.length > 0 ? folder.projects.map((p) => p.name).join(", ") : "Sin proyectos"}>
          {#snippet trailing()}
            <Button variant="plain" busy={busyRoot === folder.path} disabled={busyRoot !== null} onclick={() => void removeRoot(folder)}>Quitar</Button>
          {/snippet}
        </Row>
      {:else}
        <Row title="Ninguna carpeta" subtitle="Añade la carpeta donde guardas tus proyectos para ver sus logs." />
      {/each}
    </Group>
    <div class="add">
      <Button busy={adding} onclick={() => void addRoot()}>Añadir carpeta…</Button>
    </div>

    <Group title="Proyectos" footer="La zona horaria solo afecta a los logs de Laravel, que se escriben sin zona. La hora se muestra siempre en la del Mac.">
      {#each summaries as s (s.project.id)}
        <Row title={s.project.name} subtitle="{originLabel(s.project.origin)} · zona {s.project.log_tz}">
          {#snippet trailing()}
            <Button variant="plain" onclick={() => go(`#/logs?project=${encodeURIComponent(s.project.id)}`)}>Ver logs</Button>
            <Button onclick={() => edit(s)}>Ajustar…</Button>
          {/snippet}
        </Row>
      {:else}
        <Row title="Todavía no hay proyectos" subtitle="Aparecerán los sitios de Herd y los de tus carpetas." />
      {/each}
    </Group>
  {/if}
</div>

<Sheet open={editing !== null} title="Logs de {editing?.project.name ?? ''}" onclose={() => (editing = null)}>
  {#if editing}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <label class="field">
        <span class="label">Zona horaria de los logs</span>
        <input
          type="text"
          list="logs-timezones"
          bind:value={timezone}
          placeholder="Automática (ahora {editing.project.log_tz})"
          autocomplete="off"
          spellcheck="false"
          aria-invalid={tzError !== null}
          aria-describedby="tz-hint"
        />
        <datalist id="logs-timezones">
          {#each zones as zone (zone)}
            <option value={zone}></option>
          {/each}
        </datalist>
        <span class="hint" id="tz-hint">
          {tzError ?? "Vacío: la de config/app.php o la de APP_TIMEZONE en .env; si no hay ninguna, UTC."}
        </span>
      </label>

      <label class="field">
        <span class="label">Rutas de logs extra</span>
        <textarea
          rows="3"
          bind:value={globsText}
          placeholder="/ruta/absoluta/storage/worker/*.log"
          spellcheck="false"
          aria-invalid={globErrors.length > 0}
          aria-describedby="globs-hint"
        ></textarea>
        <span class="hint" id="globs-hint">
          {#if globErrors.length > 0}
            {globErrors[0]}
          {:else}
            Una por línea, además de storage/logs/*.log. El * solo puede ir en el nombre del archivo.
          {/if}
        </span>
      </label>

      {#if editing.sources.length > 0}
        <div class="sources">
          <span class="label">Se leen ahora</span>
          <ul>
            {#each editing.sources as src (src.source.id)}
              <li class:bad={src.state.kind !== "ok"}>
                <strong>{sourceLabel(src.source.kind)}</strong>
                <span class="mono selectable">{tildePath(src.source.dir, home)}/{src.source.pattern}</span>
                · {sourceState(src)}
              </li>
            {/each}
          </ul>
        </div>
      {/if}


      <div class="actions">
        <Button onclick={() => (editing = null)}>Cancelar</Button>
        <Button variant="primary" busy={saving} disabled={!valid} onclick={() => void save()}>Guardar</Button>
      </div>
    </form>
  {/if}
</Sheet>

<style>
  .tab {
    display: flex;
    flex-direction: column;
    gap: var(--group-gap);
  }

  .add {
    display: flex;
    justify-content: flex-end;
    margin-top: calc(var(--s-3) - var(--group-gap));
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }

  .label {
    font: var(--t-headline);
  }

  input,
  textarea {
    padding: 3px var(--s-2);
    border: none;
    border-radius: var(--r-sm);
    background: var(--group);
    box-shadow: 0 0 0 0.5px var(--fill-3), inset 0 0.5px 1px rgb(0 0 0 / 0.06);
    font: var(--t-body);
    color: var(--label);
  }

  textarea {
    resize: vertical;
    font: var(--t-log);
    line-height: 17px;
  }

  input:focus-visible,
  textarea:focus-visible {
    outline: 3px solid var(--focus-ring);
    outline-offset: 0;
  }

  input[aria-invalid="true"],
  textarea[aria-invalid="true"] {
    box-shadow: 0 0 0 1px var(--red);
  }

  .hint {
    font: var(--t-callout);
    color: var(--label-2);
  }

  [aria-invalid="true"] + datalist + .hint,
  [aria-invalid="true"] + .hint {
    color: var(--red);
  }

  .sources ul {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
    margin-top: var(--s-1);
    font: var(--t-callout);
    color: var(--label-2);
  }

  .sources .bad {
    color: var(--orange);
  }

  .mono {
    font: var(--t-log);
    line-height: 15px;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s-2);
  }
</style>
