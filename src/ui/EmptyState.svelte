<script lang="ts">
  import Button from "./Button.svelte";
  import { iconSvg, type IconName } from "./icons";

  let {
    icon,
    title,
    message,
    action,
  }: {
    icon: IconName;
    title: string;
    message?: string;
    action?: { label: string; onclick: () => void };
  } = $props();
</script>

<div class="empty">
  <span class="glyph" aria-hidden="true">{@html iconSvg(icon)}</span>
  <h2 class="title">{title}</h2>
  {#if message}<p class="message">{message}</p>{/if}
  {#if action}
    <div class="action">
      <Button onclick={action.onclick}>{action.label}</Button>
    </div>
  {/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--s-2);
    padding: var(--s-8) var(--s-6);
    text-align: center;
  }

  .glyph {
    display: inline-flex;
    width: 40px;
    height: 40px;
    margin-bottom: var(--s-1);
    color: var(--label-3);
  }

  /* A 40 px el trazo de 1,5 de la rejilla de 16 sería demasiado grueso. */
  .glyph :global(svg) {
    width: 100%;
    height: 100%;
    stroke-width: 0.9;
  }

  .title {
    font: var(--t-headline);
    color: var(--label);
  }

  .message {
    max-width: 360px;
    font: var(--t-callout);
    color: var(--label-2);
  }

  .action {
    margin-top: var(--s-2);
  }
</style>
