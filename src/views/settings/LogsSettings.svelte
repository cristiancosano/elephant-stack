<script lang="ts">
  // Ajustes ▸ Logs (00 §6, P07): carpetas en las que se buscan proyectos y, por
  // proyecto, la zona horaria de sus logs de Laravel y rutas de logs extra
  // (`~/.elephant-stack/projects.json`, 02 §4.4). Los cambios se guardan al momento.
  // Textos del área diferida «logs» (docs/i18n.md): viajan con esta vista.
  import "../../lib/i18n/locales/logs";
  import { onMount } from "svelte";

  import type { ProjectSummary } from "../../lib/bindings/ProjectSummary";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import { t } from "../../lib/i18n/index.svelte";
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
    const path = await pickFolder(t("logs.settings.roots.pick")).catch(() => null);
    if (!path) return;
    adding = true;
    try {
      const before = new Set(summaries.map((s) => s.project.id));
      summaries = await logsAddRoot(path);
      await loadRoots();
      status = "ready";
      const found = summaries.filter((s) => !before.has(s.project.id)).length;
      toast(found > 0 ? t("logs.settings.roots.added", { count: found }) : t("logs.settings.roots.addedNone"), {
        tone: "success",
      });
    } catch (e) {
      showError(e, t("logs.settings.roots.addUnavailable"));
    } finally {
      adding = false;
    }
  }

  async function removeRoot(folder: RootFolder) {
    busyRoot = folder.path;
    try {
      summaries = await logsRemoveRoot(folder.path);
      await loadRoots();
      toast(t("logs.settings.roots.removed", { path: tildePath(folder.path, home) }), { tone: "success" });
    } catch (e) {
      showError(e, t("logs.settings.roots.removeUnavailable"));
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
      toast(t("logs.settings.saved", { name: target.project.name }), { tone: "success" });
      editing = null;
    } catch (e) {
      showError(e, t("logs.settings.saveUnavailable"));
    } finally {
      saving = false;
    }
  }

  function sourceState(s: ProjectSummary["sources"][number]): string {
    if (s.state.kind === "missing") return t("logs.settings.sourceMissing");
    if (s.state.kind === "unreadable") return s.state.message;
    return t("logs.settings.files", { count: s.files.length });
  }
</script>

<div class="tab">
  {#if status === "loading"}
    <Skeleton lines={6} />
  {:else if status === "unavailable"}
    <EmptyState icon="logs" title={t("logs.settings.unavailable.title")} message={t("logs.settings.unavailable.message")} />
  {:else if status === "error"}
    <EmptyState icon="warning" title={t("logs.settings.loadError")} message={loadError ?? undefined} action={{ label: t("common.retry"), onclick: () => void load() }} />
  {:else}
    <Group
      title={t("logs.settings.roots.title")}
      footer={t("logs.settings.roots.footer")}
    >
      {#each roots as folder (folder.path)}
        <Row title={tildePath(folder.path, home)} subtitle={folder.projects.length > 0 ? folder.projects.map((p) => p.name).join(", ") : t("logs.settings.roots.noProjects")}>
          {#snippet trailing()}
            <Button variant="plain" busy={busyRoot === folder.path} disabled={busyRoot !== null} onclick={() => void removeRoot(folder)}>{t("logs.settings.roots.remove")}</Button>
          {/snippet}
        </Row>
      {:else}
        <Row title={t("logs.settings.roots.empty.title")} subtitle={t("logs.settings.roots.empty.message")} />
      {/each}
    </Group>
    <div class="add">
      <Button busy={adding} onclick={() => void addRoot()}>{t("logs.settings.roots.add")}</Button>
    </div>

    <Group title={t("logs.settings.projects.title")} footer={t("logs.settings.projects.footer")}>
      {#each summaries as s (s.project.id)}
        <Row title={s.project.name} subtitle={t("logs.settings.projects.subtitle", { origin: originLabel(s.project.origin), tz: s.project.log_tz })}>
          {#snippet trailing()}
            <Button variant="plain" onclick={() => go(`#/logs?project=${encodeURIComponent(s.project.id)}`)}>{t("logs.settings.projects.viewLogs")}</Button>
            <Button onclick={() => edit(s)}>{t("logs.settings.projects.adjust")}</Button>
          {/snippet}
        </Row>
      {:else}
        <Row title={t("logs.settings.projects.empty.title")} subtitle={t("logs.settings.projects.empty.message")} />
      {/each}
    </Group>
  {/if}
</div>

<Sheet open={editing !== null} title={t("logs.settings.sheetTitle", { name: editing?.project.name ?? "" })} onclose={() => (editing = null)}>
  {#if editing}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <label class="field">
        <span class="label">{t("logs.settings.tz.label")}</span>
        <input
          type="text"
          list="logs-timezones"
          bind:value={timezone}
          placeholder={t("logs.settings.tz.placeholder", { tz: editing.project.log_tz })}
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
          {tzError ?? t("logs.settings.tz.help")}
        </span>
      </label>

      <label class="field">
        <span class="label">{t("logs.settings.paths.label")}</span>
        <textarea
          rows="3"
          bind:value={globsText}
          placeholder={t("logs.settings.paths.placeholder")}
          spellcheck="false"
          aria-invalid={globErrors.length > 0}
          aria-describedby="globs-hint"
        ></textarea>
        <span class="hint" id="globs-hint">
          {#if globErrors.length > 0}
            {globErrors[0]}
          {:else}
            {t("logs.settings.paths.help")}
          {/if}
        </span>
      </label>

      {#if editing.sources.length > 0}
        <div class="sources">
          <span class="label">{t("logs.settings.readingNow")}</span>
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
        <Button onclick={() => (editing = null)}>{t("common.cancel")}</Button>
        <Button variant="primary" busy={saving} disabled={!valid} onclick={() => void save()}>{t("common.save")}</Button>
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
