<script lang="ts">
  // Marcador del paquete P02: la vista real llega en su paquete. Recibe ya las
  // props de §5.8 del plan, sin tipar hasta que existan los bindings (con «_»
  // porque el marcador todavía no las usa).
  import EmptyState from "../ui/EmptyState.svelte";

  let { snapshot: _snapshot, route: _route } = $props();
</script>

<!-- Props: snapshot (Snapshot | null) y route (Route). -->
<EmptyState icon="sitios" title="En construcción" message="Aquí verás los sitios que sirve Herd y podrás abrirlos, ver sus logs y elegir su PHP." />
