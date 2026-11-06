<script lang="ts" module>
  export type BadgeTone = "neutral" | "accent" | "green" | "orange" | "red" | "purple";
</script>

<script lang="ts">
  let { tone = "neutral", label }: { tone?: BadgeTone; label: string } = $props();
</script>

<span class="badge {tone}">{label}</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    height: 16px;
    padding: 0 var(--s-2);
    border-radius: var(--r-pill);
    font: var(--t-caption);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    background: color-mix(in srgb, var(--tone) 16%, transparent);
    color: var(--tone);
  }

  .neutral {
    --tone: var(--label-2);
    background: var(--fill-2);
  }

  .accent {
    --tone: var(--accent);
  }

  .green {
    --tone: var(--green);
  }

  .orange {
    --tone: var(--orange);
  }

  .red {
    --tone: var(--red);
  }

  .purple {
    --tone: var(--purple);
  }
</style>
