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
  import "../../lib/i18n/locales/wizard";
  import { t } from "../../lib/i18n/index.svelte";

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
      toast(t("wizard.shell.added"), { tone: "success" });
      onapplied();
      onclose();
    } catch (e) {
      toast(t("wizard.shell.failed", { path: preview.path, error: ipc.errorMessage(e) }), { tone: "error" });
    } finally {
      applying = false;
    }
  }
</script>

<Sheet {open} title={t("wizard.shell.title")} {onclose}>
  <p class="text">
    {t("wizard.shell.bodyBefore")} <code>php</code>
    {t("wizard.shell.bodyMiddle")} <span class="selectable path">{preview.path}</span>.
    {t("wizard.shell.bodyAfter")}
  </p>

  <pre class="diff selectable" aria-label={t("wizard.shell.diffLabel", { path: preview.path })}>{#each lines as line, i (i)}<span
        class="line {line.kind}">{line.text}</span
      >{"\n"}{/each}</pre>

  <div class="actions">
    <Button variant="plain" onclick={onclose}>{t("wizard.shell.notNow")}</Button>
    <span class="spacer"></span>
    <Button onclick={() => void copyText(addedBlock(preview.diff), "block")}>{t("common.copy")}</Button>
    <Button variant="primary" busy={applying} onclick={() => void apply()}>{t("wizard.shell.apply")}</Button>
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
