<script lang="ts">
  // Marcador del paquete P02: la vista real llega en su paquete. Recibe ya las
  // props de §5.8 del plan, sin tipar hasta que existan los bindings (con «_»
  // porque el marcador todavía no las usa).
  import EmptyState from "../../ui/EmptyState.svelte";

  let { snapshot: _snapshot } = $props();
</script>

<!-- Props: snapshot (Snapshot | null). Es una pestaña de Ajustes. -->
<EmptyState icon="ajustes" title="En construcción" message="Ajustes generales de la app: apariencia, barra de menús y editor." />
