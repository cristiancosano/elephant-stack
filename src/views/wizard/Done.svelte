<script lang="ts">
  // Final del asistente (01 §7.8 paso 5 y C26): abrir el panel, «Conectar
  // agentes» si el canal está disponible y el aviso de la barra de menús.
  import { wizard } from "../../lib/stores/wizard.svelte";
  import Button from "../../ui/Button.svelte";
  import { iconSvg } from "../../ui/icons";
  import AgentsSheet from "./AgentsSheet.svelte";
  import PathNotice from "./PathNotice.svelte";

  let { primaryAction = true }: { primaryAction?: boolean } = $props();

  let agentsOpen = $state(false);
  const snippets = $derived(wizard.snippets);
</script>

<PathNotice />

<div class="done">
  <span class="mark" aria-hidden="true">{@html iconSvg("elephant")}</span>
  <p class="text">
    El elefante se queda en la barra de menús: desde ahí enciendes servicios, cambias de PHP y ves los errores
    recientes. Puedes volver a este asistente desde Ajustes ▸ Escenario.
  </p>
  <div class="actions">
    {#if snippets}
      <Button onclick={() => (agentsOpen = true)}>Conectar agentes…</Button>
    {/if}
    <Button variant={primaryAction ? "primary" : "secondary"} onclick={() => wizard.close()}>Abrir el panel</Button>
  </div>
</div>

{#if snippets}
  <AgentsSheet open={agentsOpen} {snippets} onclose={() => (agentsOpen = false)} />
{/if}

<style>
  .done {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: var(--s-3) var(--s-4);
    padding: var(--s-4);
    border-radius: var(--r-md);
    background: var(--group);
    box-shadow: var(--shadow-1);
  }

  .mark {
    display: inline-flex;
    width: 40px;
    height: 40px;
    color: var(--accent);
  }

  .mark :global(svg) {
    width: 100%;
    height: 100%;
  }

  .text {
    color: var(--label-2);
  }

  .actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: var(--s-2);
  }
</style>
