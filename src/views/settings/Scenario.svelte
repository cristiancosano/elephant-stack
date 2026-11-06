<script lang="ts">
  // Marcador del paquete P02: la vista real llega en su paquete. Recibe ya las
  // props de §5.8 del plan, sin tipar hasta que existan los bindings (con «_»
  // porque el marcador todavía no las usa).
  import EmptyState from "../../ui/EmptyState.svelte";

  let { snapshot: _snapshot } = $props();
</script>

<!-- Props: snapshot (Snapshot | null). Es una pestaña de Ajustes. -->
<EmptyState icon="sparkles" title="En construcción" message="El escenario de servicios y PHP elegido en el asistente." />
