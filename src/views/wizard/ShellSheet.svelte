<script lang="ts">
  // Hoja con el diff de ~/.zshrc (04 §4.7): se ve exactamente lo que se añadiría,
  // se puede copiar o dejar que Elephant lo añada. Solo se aplica el hash visto.
  import * as ipc from "../../lib/ipc";
  import type { ShellPreview } from "../../lib/bindings/ShellPreview";
  import { copyText } from "../../lib/stores/wizard.svelte";
  import { addedBlock, diffLines } from "../../lib/wizard/shell";
  import Button from "../../ui/Button.svelte";
  import Sheet from "../../ui/Sheet.svelte";
  import { toast } from "../../ui/toasts.svelte";

  let {
    open,
    preview,
    onclose,
    onapplied,
  }: { open: boolean; preview: ShellPreview; onclose: () => void; onapplied: () => void } = $props();

  const lines = $derived(diffLines(preview.diff));
  let applying = $state(false);

  async function apply() {
    applying = true;
    try {
      await ipc.setupShellApply(preview.hash);
      toast("Añadido. Abre una terminal nueva para que surta efecto.", { tone: "success" });
      onapplied();
      onclose();
    } catch (e) {
      toast(`No se ha podido cambiar ${preview.path}: ${ipc.errorMessage(e)}`, { tone: "error" });
    } finally {
      applying = false;
    }
  }
</script>

<Sheet {open} title="Añadir Elephant al PATH" {onclose}>
  <p class="text">
    Para que el comando <code>php</code> de la terminal elija la versión de cada proyecto, hay que añadir este bloque al
    final de <span class="selectable path">{preview.path}</span>. Antes se guarda una copia del archivo, y no se toca
    ninguna otra línea (tampoco la de Herd).
  </p>

  <pre class="diff selectable" aria-label="Cambios en {preview.path}">{#each lines as line, i (i)}<span
        class="line {line.kind}">{line.text}</span
      >{"\n"}{/each}</pre>

  <div class="actions">
    <Button variant="plain" onclick={onclose}>Ahora no</Button>
    <span class="spacer"></span>
    <Button onclick={() => void copyText(addedBlock(preview.diff), "Bloque")}>Copiar</Button>
    <Button variant="primary" busy={applying} onclick={() => void apply()}>Añadirlo por mí</Button>
  </div>
</Sheet>

<style>
  .text {
    color: var(--label-2);
  }

  code,
  .path {
    font-family: var(--mono);
    color: var(--label);
  }

  .diff {
    margin: var(--s-3) 0;
    padding: var(--s-3);
    overflow: auto;
    max-height: 240px;
    border-radius: var(--r-sm);
    background: var(--fill);
    font: var(--t-log);
    white-space: pre;
  }

  .line.add {
    color: var(--green);
  }

  .line.remove {
    color: var(--red);
  }

  .line.header,
  .line.hunk {
    color: var(--label-2);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .spacer {
    flex: 1;
  }
</style>
