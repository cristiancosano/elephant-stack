<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  // `label` es obligatoria: es lo único que leen VoiceOver y el tooltip.
  let {
    icon,
    label,
    onclick,
    disabled = false,
  }: {
    icon: IconName;
    label: string;
    onclick?: (event: MouseEvent) => void;
    disabled?: boolean;
  } = $props();
</script>

<button type="button" class="icon-button" aria-label={label} title={label} {disabled} {onclick}>
  <Icon name={icon} />
</button>

<style>
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 28px;
    height: var(--control-height);
    padding: 0;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--label-2);
    transition: background-color var(--d-1) var(--ease);
  }

  /* Como en las barras de herramientas de macOS: el bisel solo aparece bajo el ratón. */
  .icon-button:hover:not(:disabled) {
    background: var(--fill-2);
    color: var(--label);
  }

  .icon-button:active:not(:disabled) {
    background: var(--fill-3);
  }

  .icon-button:disabled {
    opacity: 0.4;
  }
</style>
