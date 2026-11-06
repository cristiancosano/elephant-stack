<script lang="ts">
  // Bienvenida y detección (04 §4.11, paso 1).
  import { wizard } from "../../lib/stores/wizard.svelte";
  import { findings, herdBanner } from "../../lib/wizard/detection";
  import Icon from "../../ui/Icon.svelte";
  import { iconSvg } from "../../ui/icons";
  import Skeleton from "../../ui/Skeleton.svelte";

  const list = $derived(wizard.detection ? findings(wizard.detection) : null);
  const banner = $derived(herdBanner(wizard.detection));
</script>

<section class="welcome">
  <span class="mark" aria-hidden="true">{@html iconSvg("elephant")}</span>
  <h1 class="title">Te damos la bienvenida a Elephant Stack</h1>
  <p class="lead">
    Vamos a dejar tu Mac listo para trabajar con PHP y Laravel. Primero, esto es lo que hay ahora mismo:
  </p>

  {#if banner}
    <p class="banner" role="note">
      <Icon name="info" />
      <span>{banner}</span>
    </p>
  {/if}

  <div class="found" aria-live="polite" aria-busy={!list && !wizard.detectionError}>
    {#if list}
      <ul class="findings">
        {#each list as f (f.id)}
          <li class="finding {f.tone}">
            <Icon name={f.tone === "warning" ? "warning" : f.tone === "ok" ? "check" : "info"} />
            <span>{f.text}</span>
          </li>
        {/each}
      </ul>
    {:else if wizard.detectionError}
      <p class="error">No se ha podido revisar el Mac: {wizard.detectionError}. Puedes seguir igualmente.</p>
    {:else}
      <p class="visually-hidden">Revisando tu Mac…</p>
      <Skeleton lines={4} />
    {/if}
  </div>

  <p class="note">Nada cambia hasta que revises el plan y pulses «Aplicar».</p>
</section>

<style>
  .welcome {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--s-4);
    padding-top: var(--s-6);
  }

  .mark {
    display: inline-flex;
    width: 56px;
    height: 56px;
    color: var(--accent);
  }

  .mark :global(svg) {
    width: 100%;
    height: 100%;
  }

  .title {
    font: var(--t-hero);
  }

  .lead {
    max-width: 520px;
    color: var(--label-2);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    width: 100%;
    padding: var(--s-3);
    border-radius: var(--r-md);
    background: var(--accent-soft);
    color: var(--label);
  }

  .banner :global(svg) {
    color: var(--accent);
  }

  .found {
    width: 100%;
    padding: var(--s-3) var(--s-4);
    border-radius: var(--r-md);
    background: var(--group);
    box-shadow: var(--shadow-1);
  }

  .findings {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .finding {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    animation: appear var(--d-2) var(--ease) both;
  }

  .finding.ok :global(svg) {
    color: var(--green);
  }

  .finding.info :global(svg) {
    color: var(--label-2);
  }

  .finding.warning :global(svg) {
    color: var(--orange);
  }

  /* La lista «se rellena»: cada fila aparece un poco después de la anterior. */
  .finding:nth-child(2) {
    animation-delay: 40ms;
  }
  .finding:nth-child(3) {
    animation-delay: 80ms;
  }
  .finding:nth-child(4) {
    animation-delay: 120ms;
  }
  .finding:nth-child(n + 5) {
    animation-delay: 160ms;
  }

  @keyframes appear {
    from {
      opacity: 0;
      transform: translateY(2px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .finding {
      animation: none;
    }
  }

  .error {
    color: var(--label-2);
  }

  .note {
    font: var(--t-callout);
    color: var(--label-2);
  }
</style>
