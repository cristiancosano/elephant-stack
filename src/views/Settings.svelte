<script lang="ts">
  // Ajustes (C18): General · Escenario · Logs · Agentes, en `#/ajustes/<pestaña>`.
  // Cada pestaña recibe `{ snapshot }` y guarda sus cambios al instante.
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import General from "./settings/General.svelte";
  import { go } from "../lib/router.svelte";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { Route } from "../lib/route";

  let { snapshot, route }: { snapshot: Snapshot | null; route: Route } = $props();

  type Tab = "general" | "escenario" | "logs" | "agentes";

  const TABS: { value: Tab; label: string }[] = [
    { value: "general", label: "General" },
    { value: "escenario", label: "Escenario" },
    { value: "logs", label: "Logs" },
    { value: "agentes", label: "Agentes" },
  ];

  const tab: Tab = $derived(TABS.find((t) => t.value === route.sub)?.value ?? "general");

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
    <h1 data-tauri-drag-region>Ajustes</h1>
  </header>

  <div class="scroll">
    <div class="content">
      <div class="tabs">
        <SegmentedControl
          options={TABS}
          value={tab}
          label="Sección de los ajustes"
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
