<script lang="ts">
  // Ajustes (C18): General · Escenario · Logs · Agentes, en `#/ajustes/<pestaña>`.
  // Cada pestaña recibe `{ snapshot }` y guarda sus cambios al instante.
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import General from "./settings/General.svelte";
  import { go } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { Route } from "../lib/route";

  let { snapshot, route }: { snapshot: Snapshot | null; route: Route } = $props();

  type Tab = "general" | "escenario" | "logs" | "agentes";

  const TABS: { value: Tab; label: string }[] = $derived([
    { value: "general", label: t("settings.tabs.general") },
    { value: "escenario", label: t("settings.tabs.scenario") },
    { value: "logs", label: t("settings.tabs.logs") },
    { value: "agentes", label: t("settings.tabs.agents") },
  ]);

  const tab: Tab = $derived(TABS.find((option) => option.value === route.sub)?.value ?? "general");

  // Las pestañas de otros dominios se cargan al abrirlas: no entran en el chunk inicial.
  let scenario: Promise<typeof import("./settings/Scenario.svelte")> | null = null;
  let logs: Promise<typeof import("./settings/LogsSettings.svelte")> | null = null;
  let agents: Promise<typeof import("./settings/Agents.svelte")> | null = null;
  const loadScenario = () => (scenario ??= import("./settings/Scenario.svelte"));
  const loadLogs = () => (logs ??= import("./settings/LogsSettings.svelte"));
  const loadAgents = () => (agents ??= import("./settings/Agents.svelte"));
</script>

<div class="view">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>{t("settings.title")}</h1>
  </header>

  <div class="scroll">
    <div class="content">
      <div class="tabs">
        <SegmentedControl
          options={TABS}
          value={tab}
          label={t("settings.tabs.label")}
          onchange={(next) => go({ view: "ajustes", sub: next, params: {} })}
        />
      </div>

      <div class="panel">
        {#if tab === "general"}
          <General {snapshot} />
        {:else if tab === "escenario"}
          {#await loadScenario() then { default: Scenario }}<Scenario {snapshot} />{/await}
        {:else if tab === "logs"}
          {#await loadLogs() then { default: LogsSettings }}<LogsSettings {snapshot} />{/await}
        {:else}
          {#await loadAgents() then { default: Agents }}<Agents {snapshot} />{/await}
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .tabs {
    display: flex;
    justify-content: center;
  }
</style>
