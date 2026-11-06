<script lang="ts" module>
  export type DotState = "running" | "stopped" | "error" | "busy";
</script>

<script lang="ts">
  // El color nunca va solo: `label` («En marcha», «Error · código 1») es lo que lee
  // VoiceOver, y la fila suele repetirlo como texto visible.
  let { state, label }: { state: DotState; label: string } = $props();
</script>

<span class="dot {state}" role="img" aria-label={label}></span>

<style>
  .dot {
    display: inline-block;
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: var(--r-pill);
  }

  .running {
    background: var(--green);
  }

  .stopped {
    background: var(--label-3);
  }

  .error {
    background: var(--red);
  }

  .busy {
    background: var(--orange);
    animation: pulse 1.2s var(--ease) infinite alternate;
  }

  @keyframes pulse {
    from {
      opacity: 1;
    }
    to {
      opacity: 0.35;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .busy {
      animation: none;
    }
  }
</style>
