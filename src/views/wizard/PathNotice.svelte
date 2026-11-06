<script lang="ts">
  // Aviso «Añadirlo por mí» del PATH (04 §4.4 regla 9): no es un paso del plan,
  // porque necesita su propia confirmación con el diff delante.
  import "../../lib/i18n/locales/wizard";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/stores/wizard.svelte";
  import Button from "../../ui/Button.svelte";
  import Group from "../../ui/Group.svelte";
  import Icon from "../../ui/Icon.svelte";
  import Row from "../../ui/Row.svelte";
  import ShellSheet from "./ShellSheet.svelte";

  let open = $state(false);
  const preview = $derived(wizard.shell);
</script>

{#if preview}
  <Group title={t("wizard.pathNotice.group")}>
    <Row
      title={t("wizard.pathNotice.title")}
      subtitle={t("wizard.pathNotice.subtitle", { path: preview.path })}
    >
      {#snippet leading()}<Icon name="terminal" />{/snippet}
      {#snippet trailing()}<Button onclick={() => (open = true)}>{t("wizard.pathNotice.viewChange")}</Button>{/snippet}
    </Row>
  </Group>
  <ShellSheet {open} {preview} onclose={() => (open = false)} onapplied={() => wizard.refreshShell()} />
{/if}
