<script lang="ts">
  // «Conectar agentes» (C26; antes «Copiar configuración del MCP»): los comandos
  // del plugin de Claude Code y, si existen, los fragmentos MCP de otros clientes.
  // La conexión con un clic y la confirmación están en Ajustes ▸ Agentes.
  import type { McpSnippets } from "../../lib/bindings/McpSnippets";
  import { go } from "../../lib/router.svelte";
  import { copyText } from "../../lib/stores/wizard.svelte";
  import Button from "../../ui/Button.svelte";
  import Sheet from "../../ui/Sheet.svelte";

  let { open, snippets, onclose }: { open: boolean; snippets: McpSnippets; onclose: () => void } = $props();

  const mcp = $derived(snippets.project_json.trim());
  const guide = $derived(snippets.shell_guide.trim());

  function settings() {
    onclose();
    go("#/ajustes/agentes");
  }
</script>

<Sheet {open} title="Conectar agentes" {onclose}>
  <p class="text">
    Claude Code puede consultar los logs de tus proyectos y encender servicios con el comando <code>elephant</code>. Lo que
    cambia algo te pide confirmación, salvo que elijas otro modo en Ajustes ▸ Agentes. Para instalar el plugin, ejecuta en
    la terminal:
  </p>
  <pre class="code selectable">{snippets.claude_command}</pre>
  <div class="row">
    <Button onclick={() => void copyText(snippets.claude_command, "Comando")}>Copiar comandos</Button>
    {#if guide}
      <Button variant="plain" onclick={() => void copyText(guide, "Guía")}>Copiar guía para otros agentes</Button>
    {/if}
  </div>

  {#if mcp}
    <p class="text">Para otros clientes con MCP (Cursor, VS Code…), este es el fragmento del proyecto:</p>
    <pre class="code selectable">{mcp}</pre>
    <div class="row">
      <Button onclick={() => void copyText(mcp, "Fragmento")}>Copiar fragmento</Button>
    </div>
  {/if}

  <div class="actions">
    <Button variant="plain" onclick={settings}>Abrir Ajustes ▸ Agentes</Button>
    <span class="spacer"></span>
    <Button variant="primary" onclick={onclose}>Listo</Button>
  </div>
</Sheet>

<style>
  .text {
    margin-bottom: var(--s-2);
    color: var(--label-2);
  }

  code {
    font-family: var(--mono);
    color: var(--label);
  }

  .code {
    margin: 0 0 var(--s-2);
    padding: var(--s-3);
    overflow: auto;
    max-height: 160px;
    border-radius: var(--r-sm);
    background: var(--fill);
    font: var(--t-log);
    white-space: pre;
  }

  .row {
    display: flex;
    gap: var(--s-2);
    margin-bottom: var(--s-4);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .spacer {
    flex: 1;
  }
</style>
