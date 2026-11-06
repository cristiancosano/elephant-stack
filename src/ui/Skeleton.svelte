<script lang="ts">
  // Esqueleto de carga: nunca un spinner a pantalla completa (01 §4.5).
  let { lines = 3 }: { lines?: number } = $props();

  // Anchos fijos que se repiten: parece texto y no salta entre renderizados.
  const WIDTHS = [92, 76, 84, 58, 70];
</script>

<div class="skeleton" aria-hidden="true">
  {#each { length: Math.max(1, lines) }, i (i)}
    <span class="line" style:width="{WIDTHS[i % WIDTHS.length]}%"></span>
  {/each}
</div>

<style>
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-2) 0;
  }

  .line {
    display: block;
    height: 12px;
    border-radius: var(--r-xs);
    background: linear-gradient(90deg, var(--fill-2) 0%, var(--fill) 50%, var(--fill-2) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.6s ease-in-out infinite;
  }

  @keyframes shimmer {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: -100% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .line {
      animation: none;
      background: var(--fill-2);
    }
  }
</style>
