<script lang="ts" generics="T">
  let {
    options,
    value,
    label,
    onchange,
  }: {
    options: { value: T; label: string }[];
    value: T;
    label: string;
    onchange: (value: T) => void;
  } = $props();

  let buttons: HTMLButtonElement[] = $state([]);
  let pill = $state({ left: 0, width: 0 });
  // La primera colocación de la píldora no se anima.
  let placed = $state(false);

  const selected = $derived(options.findIndex((o) => Object.is(o.value, value)));

  function measure() {
    const el = buttons[selected];
    if (!el) return;
    pill = { left: el.offsetLeft, width: el.offsetWidth };
  }

  $effect(() => {
    // Se vuelve a medir al cambiar la selección o las opciones.
    void selected;
    void options.length;
    measure();
    if (!placed) requestAnimationFrame(() => (placed = true));
  });

  $effect(() => {
    const el = buttons[0]?.parentElement;
    if (!el) return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  });

  function choose(index: number) {
    const option = options[index];
    if (!option) return;
    if (!Object.is(option.value, value)) onchange(option.value);
  }

  function onkeydown(event: KeyboardEvent) {
    const n = options.length;
    if (n === 0) return;
    const from = selected < 0 ? 0 : selected;
    let next: number;
    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        next = (from + 1) % n;
        break;
      case "ArrowLeft":
      case "ArrowUp":
        next = (from - 1 + n) % n;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = n - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    choose(next);
    buttons[next]?.focus();
  }
</script>

<div class="segmented" role="radiogroup" aria-label={label} tabindex="-1" {onkeydown}>
  {#if selected >= 0 && pill.width > 0}
    <span
      class="pill"
      class:animated={placed}
      style:width="{pill.width}px"
      style:transform="translateX({pill.left}px)"
      aria-hidden="true"
    ></span>
  {/if}
  {#each options as option, i (i)}
    <button
      type="button"
      role="radio"
      class="segment"
      class:active={i === selected}
      aria-checked={i === selected}
      tabindex={i === selected || (selected < 0 && i === 0) ? 0 : -1}
      bind:this={buttons[i]}
      onclick={() => choose(i)}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .segmented {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
    height: var(--control-height);
    padding: 1px;
    border-radius: var(--r-sm);
    background: var(--fill-2);
  }

  .pill {
    position: absolute;
    top: 1px;
    bottom: 1px;
    left: 0;
    border-radius: calc(var(--r-sm) - 1px);
    background: var(--raised);
    box-shadow: var(--shadow-1);
  }

  .pill.animated {
    transition:
      transform var(--d-2) var(--ease),
      width var(--d-2) var(--ease);
  }

  .segment {
    position: relative;
    padding: 0 var(--s-3);
    border: none;
    border-radius: calc(var(--r-sm) - 1px);
    background: transparent;
    color: var(--label);
    font: var(--t-body);
    white-space: nowrap;
  }

  .segment.active {
    font-weight: 500;
  }

  /* El anillo de foco de la píldora lo pone el segmento. */
  .segment:focus-visible {
    outline-offset: -1px;
  }
</style>
