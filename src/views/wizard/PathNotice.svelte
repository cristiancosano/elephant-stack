<script lang="ts">
  // Aviso «Añadirlo por mí» del PATH (04 §4.4 regla 9): no es un paso del plan,
  // porque necesita su propia confirmación con el diff delante.
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
  <Group title="Terminal">
    <Row
      title="El comando php de Elephant no está en tu terminal"
      subtitle="Hay que añadir una línea a {preview.path}. Te enseñamos el cambio antes."
    >
      {#snippet leading()}<Icon name="terminal" />{/snippet}
      {#snippet trailing()}<Button onclick={() => (open = true)}>Ver el cambio…</Button>{/snippet}
    </Row>
  </Group>
  <ShellSheet {open} {preview} onclose={() => (open = false)} onapplied={() => wizard.refreshShell()} />
{/if}
