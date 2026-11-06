<script lang="ts">
  // Traza de una excepción (02 §9): los frames propios resaltados, cada uno con
  // «Abrir en el editor», y los de vendor/ plegados en «N frames de vendor».
  import type { Frame } from "../../lib/bindings/Frame";
  import { foldFrames } from "../../lib/logs/markdown";
  import { t } from "../../lib/i18n/index.svelte";
  import Icon from "../../ui/Icon.svelte";
  import IconButton from "../../ui/IconButton.svelte";

  let { frames, root, onopen }: { frames: Frame[]; root: string | null; onopen: (file: string, line: number) => void } = $props();

  const segments = $derived(foldFrames(frames));
  // Bloques de vendor desplegados, por posición en la lista.
  let expanded: Record<number, boolean> = $state({});

  /** Ruta relativa a la raíz del proyecto, que es lo que se lee de un vistazo. */
  function short(file: string): string {
    if (root && file.startsWith(`${root}/`)) return file.slice(root.length + 1);
    return file;
  }
</script>

<ol class="frames">
  {#each segments as seg, i (i)}
    {#if seg.kind === "own"}
      <li class="frame own">
        <span class="index num">#{seg.frame.index}</span>
        <span class="where selectable" title={seg.frame.file}>
          <span class="file">{short(seg.frame.file)}</span>{#if seg.frame.line > 0}<span class="line">:{seg.frame.line}</span>{/if}
          {#if seg.frame.call}<span class="call">{seg.frame.call}</span>{/if}
        </span>
        {#if seg.frame.file.startsWith("/")}
          <IconButton icon="editor" label={t("common.openInEditor")} onclick={() => onopen(seg.frame.file, seg.frame.line)} />
        {/if}
      </li>
    {:else}
      <li class="vendor">
        <button type="button" class="toggle" aria-expanded={expanded[i] ?? false} onclick={() => (expanded[i] = !(expanded[i] ?? false))}>
          <Icon name={expanded[i] ? "chevron-down" : "chevron-right"} />
          {t("logs.frames.vendor", { count: seg.frames.length })}
        </button>
        {#if expanded[i]}
          <ol class="frames nested">
            {#each seg.frames as f (f.index)}
              <li class="frame" class:framework={f.framework}>
                <span class="index num">#{f.index}</span>
                <span class="where selectable" title={f.file}>
                  <span class="file">{short(f.file)}</span>{#if f.line > 0}<span class="line">:{f.line}</span>{/if}
                  {#if f.call}<span class="call">{f.call}</span>{/if}
                </span>
              </li>
            {/each}
          </ol>
        {/if}
      </li>
    {/if}
  {/each}
</ol>

<style>
  .frames {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font: var(--t-log);
    line-height: 16px;
  }

  .nested {
    padding-left: var(--s-5);
  }

  .frame {
    display: flex;
    align-items: flex-start;
    gap: var(--s-2);
    min-height: 22px;
    padding: 2px 0;
  }

  .index {
    flex: 0 0 auto;
    width: 28px;
    color: var(--label-3);
    text-align: right;
  }

  .where {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--label-2);
  }

  .own .file {
    color: var(--label);
    font-weight: 600;
  }

  .line {
    color: var(--label-2);
  }

  .call {
    display: block;
    color: var(--label-2);
  }

  .framework .where {
    color: var(--label-3);
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    padding: 2px var(--s-1);
    border: none;
    border-radius: var(--r-xs);
    background: none;
    font: var(--t-callout);
    color: var(--label-2);
  }

  .toggle:hover {
    background: var(--fill);
  }
</style>
