<script lang="ts">
  // Pastilla de nivel con su color (tokens `--lvl-*`).
  import type { LogLevel } from "../../lib/bindings/LogLevel";
  import { levelLabel, levelTone } from "../../lib/logs/model";

  let { level }: { level: LogLevel } = $props();
</script>

<span class="pill lvl-{levelTone(level)}">{levelLabel(level)}</span>

<style>
  .pill {
    display: inline-block;
    min-width: 48px;
    padding: 0 var(--s-1);
    border-radius: var(--r-xs);
    font: var(--t-caption);
    line-height: 15px;
    text-align: center;
    white-space: nowrap;
    color: var(--lvl);
    background: color-mix(in srgb, var(--lvl) 14%, transparent);
  }

  /* --lvl-debug (--label-3) se queda corto de contraste para texto de 11 px. */
  .lvl-debug {
    --lvl: var(--label-2);
  }
  .lvl-info {
    --lvl: var(--lvl-info);
  }
  .lvl-notice {
    --lvl: var(--lvl-notice);
  }
  .lvl-warning {
    --lvl: var(--lvl-warning);
  }
  .lvl-error {
    --lvl: var(--lvl-error);
  }
  .lvl-critical {
    --lvl: var(--lvl-critical);
  }
</style>
