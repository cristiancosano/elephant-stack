<script lang="ts">
  // Interruptor nativo de WebKit (`<input type="checkbox" switch>`).
  // `label` lleva verbo y objeto («Detener mysql»): es lo que lee VoiceOver.
  let {
    checked,
    label,
    disabled = false,
    onchange,
  }: {
    checked: boolean;
    label: string;
    disabled?: boolean;
    onchange?: (value: boolean) => void;
  } = $props();

  function handle(event: Event & { currentTarget: HTMLInputElement }) {
    const next = event.currentTarget.checked;
    // El estado lo decide quien escucha: si no lo cambia, el control vuelve atrás.
    event.currentTarget.checked = checked;
    onchange?.(next);
  }
</script>

<input
  class="switch"
  type="checkbox"
  switch
  aria-label={label}
  {checked}
  {disabled}
  onchange={handle}
/>

<style>
  .switch {
    margin: 0;
    flex-shrink: 0;
    accent-color: var(--accent);
  }

  .switch:disabled {
    opacity: 0.5;
  }
</style>
