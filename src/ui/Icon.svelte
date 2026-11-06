<script lang="ts">
  import { iconSvg, type IconName } from "./icons";

  let { name, size = 16 }: { name: IconName; size?: 16 | 20 } = $props();
</script>

<!-- Decorativo: quien lo usa pone el texto o la etiqueta accesible. -->
<span class="icon" style:--size="{size}px" aria-hidden="true">{@html iconSvg(name)}</span>

<style>
  .icon {
    display: inline-flex;
    width: var(--size);
    height: var(--size);
    flex-shrink: 0;
  }

  .icon :global(svg) {
    width: 100%;
    height: 100%;
  }
</style>
