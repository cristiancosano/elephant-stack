<script lang="ts">
  // «Conectar agentes» (C26; antes «Copiar configuración del MCP»): los comandos
  // del plugin de Claude Code y, si existen, los fragmentos MCP de otros clientes.
  // La conexión con un clic y la confirmación están en Ajustes ▸ Agentes.
  import "../../lib/i18n/locales/agents";
  import { t } from "../../lib/i18n/index.svelte";
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

<Sheet {open} title={t("agents.sheet.title")} {onclose}>
  <p class="text">
    {t("agents.sheet.bodyBefore")} <code>elephant</code>. {t("agents.sheet.bodyAfter")}
  </p>
  <pre class="code selectable">{snippets.claude_command}</pre>
  <div class="row">
    <Button onclick={() => void copyText(snippets.claude_command, "command")}>{t("agents.sheet.copyCommands")}</Button>
    {#if guide}
      <Button variant="plain" onclick={() => void copyText(guide, "guide")}>{t("agents.sheet.copyGuide")}</Button>
    {/if}
  </div>

  {#if mcp}
    <p class="text">{t("agents.sheet.mcpIntro")}</p>
    <pre class="code selectable">{mcp}</pre>
    <div class="row">
      <Button onclick={() => void copyText(mcp, "snippet")}>{t("agents.sheet.copySnippet")}</Button>
    </div>
  {/if}

  <div class="actions">
    <Button variant="plain" onclick={settings}>{t("agents.sheet.openSettings")}</Button>
    <span class="spacer"></span>
    <Button variant="primary" onclick={onclose}>{t("agents.sheet.done")}</Button>
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
