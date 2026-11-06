<script lang="ts">
  // Contexto JSON formateado con resaltado propio mínimo (01 §7.5): claves, cadenas,
  // números y literales con los colores del sistema. Sin librería.
  import { prettyJson, tokenize } from "../../lib/logs/json";
  import "../../lib/i18n/locales/logs";
  import { t as tr } from "../../lib/i18n/index.svelte";

  let { value }: { value: unknown } = $props();

  const pretty = $derived(prettyJson(value));
  const tokens = $derived(tokenize(pretty.text));
</script>

<pre class="json selectable mono">{#each tokens as t, i (i)}{#if t.kind === "space" || t.kind === "punct" || t.kind === "text"}{t.text}{:else}<span class={t.kind}>{t.text}</span>{/if}{/each}{#if pretty.truncated}<span class="note">
{tr("logs.json.truncated")}</span>{/if}</pre>

<style>
  .json {
    margin: 0;
    padding: var(--s-2) var(--s-3);
    overflow-x: auto;
    border-radius: var(--r-sm);
    background: var(--fill);
    font: var(--t-log);
    line-height: 17px;
    color: var(--label-2);
    white-space: pre;
  }

  .key {
    color: var(--purple);
  }

  .string {
    color: var(--red);
  }

  .number {
    color: var(--blue);
  }

  .literal {
    color: var(--orange);
  }

  .note {
    color: var(--label-3);
  }
</style>
