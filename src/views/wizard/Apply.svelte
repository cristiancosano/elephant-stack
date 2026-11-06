<script lang="ts">
  // Aplicar y resultado (04 §4.11 paso 4): una fila por paso con su estado, la
  // barra de las descargas y la última línea de brew; al final, el resumen.
  import * as ipc from "../../lib/ipc";
  import { formatBytes } from "../../lib/format";
  import { wizard } from "../../lib/stores/wizard.svelte";
  import { canRetry, downloadFraction, headline, resultLine, stateLabel, summarize, type RunStep } from "../../lib/wizard/run";
  import Button from "../../ui/Button.svelte";
  import Icon from "../../ui/Icon.svelte";
  import ProgressRing from "../../ui/ProgressRing.svelte";
  import { toast } from "../../ui/toasts.svelte";
  import Done from "./Done.svelte";

  const run = $derived(wizard.s.run);
  const summary = $derived(run ? summarize(run) : null);
  const logPath = $derived(run?.report?.log_path || null);

  let expanded: Record<string, boolean> = $state({});

  function secondary(step: RunStep): string {
    if (step.state === "running" && step.bytes) {
      const [done, total] = step.bytes;
      return `${formatBytes(done)} de ${formatBytes(total)}`;
    }
    if (step.state === "running" && step.line) return step.line;
    return stateLabel(step);
  }

  async function openLog() {
    if (!logPath) return;
    try {
      await ipc.openInEditor(logPath);
    } catch {
      // Sin editor configurado, al menos se enseña el archivo en el Finder.
      await ipc.revealInFinder(logPath).catch((e: unknown) => {
        toast(`No se ha podido abrir el log: ${ipc.errorMessage(e)}`, { tone: "error" });
      });
    }
  }
</script>

{#if run && summary}
  <section class="apply">
    <header class="head">
      <h1 class="title">{headline(run)}</h1>
      <div class="progress" aria-hidden={run.finished}>
        <div class="bar"><div class="fill" style:width="{Math.round(summary.fraction * 100)}%"></div></div>
      </div>
      <p class="lead num" aria-live="polite">
        {#if run.error}
          {run.error}
        {:else if run.finished}
          {resultLine(run)}
        {:else}
          {summary.ok + summary.failed + summary.skipped} de {run.steps.length}
        {/if}
      </p>
    </header>

    {#if run.steps.length}
      <ol class="steps" aria-label="Pasos">
        {#each run.steps as step (step.key)}
          {@const fraction = downloadFraction(step)}
          {@const open = expanded[step.key] ?? false}
          <li class="step {step.state}">
            <span class="status" aria-hidden="true">
              {#if step.state === "running"}
                <ProgressRing size={16} value={fraction ?? undefined} />
              {:else if step.state === "ok"}
                <Icon name="check" />
              {:else if step.state === "failed"}
                <Icon name="error" />
              {:else if step.state === "skipped"}
                <Icon name="x" />
              {:else}
                <span class="waiting"></span>
              {/if}
            </span>
            <span class="text">
              <span class="name">{step.title}</span>
              <span class="detail" class:mono={step.state === "running" && !!step.line && !step.bytes}>{secondary(step)}</span>
              {#if step.state === "running" && fraction !== null}
                <span class="dl"><span class="dl-fill" style:width="{Math.round(fraction * 100)}%"></span></span>
              {/if}
            </span>
            {#if step.log.length}
              <Button variant="plain" onclick={() => (expanded[step.key] = !open)}>
                {open ? "Ocultar salida" : "Ver salida"}
              </Button>
            {/if}
            {#if open}
              <pre class="log selectable">{step.log.join("\n")}</pre>
            {/if}
          </li>
        {/each}
      </ol>
    {/if}

    <div class="actions">
      {#if !run.finished}
        <Button disabled={run.cancelling} onclick={() => void wizard.cancel()}>
          {run.cancelling ? "Cancelando…" : "Cancelar"}
        </Button>
        {#if run.cancelling}
          <span class="hint">Se termina el paso en curso y se salta el resto. Nunca se corta Homebrew a medias.</span>
        {/if}
      {:else}
        {#if canRetry(run)}
          <Button variant="primary" onclick={() => wizard.retry()}>Reintentar lo que falta</Button>
        {/if}
        {#if logPath}
          <Button onclick={() => void openLog()}>Ver log completo</Button>
        {/if}
      {/if}
    </div>

    {#if run.finished}
      <Done primaryAction={!canRetry(run)} />
    {/if}
  </section>
{/if}

<style>
  .apply {
    display: flex;
    flex-direction: column;
    gap: var(--group-gap);
    padding-top: var(--s-4);
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .title {
    font: var(--t-hero);
  }

  .lead {
    color: var(--label-2);
  }

  .bar {
    height: 4px;
    overflow: hidden;
    border-radius: var(--r-pill);
    background: var(--fill-2);
  }

  .fill {
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width var(--d-3) var(--ease);
  }

  .steps {
    overflow: hidden;
    border-radius: var(--r-md);
    background: var(--group);
    box-shadow: var(--shadow-1);
  }

  .step {
    position: relative;
    display: grid;
    grid-template-columns: 16px 1fr auto;
    align-items: center;
    gap: 0 var(--s-3);
    min-height: var(--row-height-subtitle);
    padding: var(--s-1) var(--s-3);
  }

  .step + .step::before {
    content: "";
    position: absolute;
    top: 0;
    left: calc(var(--s-3) + 16px + var(--s-3));
    right: 0;
    border-top: 0.5px solid var(--separator);
  }

  .status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--label-3);
  }

  .ok .status {
    color: var(--green);
  }

  .failed .status {
    color: var(--red);
  }

  .running .status {
    color: var(--accent);
  }

  .waiting {
    width: 8px;
    height: 8px;
    border-radius: var(--r-pill);
    border: 1.5px solid currentColor;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .name {
    font: var(--t-headline);
  }

  .step.pending .name,
  .step.skipped .name {
    color: var(--label-2);
  }

  .detail {
    overflow: hidden;
    font: var(--t-callout);
    color: var(--label-2);
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .failed .detail {
    color: var(--red);
    white-space: normal;
  }

  .skipped .detail {
    white-space: normal;
  }

  .mono {
    font-family: var(--mono);
  }

  .dl {
    display: block;
    height: 3px;
    margin-top: 2px;
    overflow: hidden;
    border-radius: var(--r-pill);
    background: var(--fill-2);
  }

  .dl-fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width var(--d-2) linear;
  }

  .log {
    grid-column: 2 / -1;
    max-height: 200px;
    margin: var(--s-1) 0 var(--s-2);
    padding: var(--s-2) var(--s-3);
    overflow: auto;
    border-radius: var(--r-sm);
    background: var(--fill);
    font: var(--t-log);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .hint {
    font: var(--t-callout);
    color: var(--label-2);
  }
</style>
