<script lang="ts" generics="T">
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";

  // Botón con aspecto de pop-up de macOS que abre un menú nativo
  // (@tauri-apps/api/menu). Fuera de Tauri (la galería en el navegador) usa un
  // <select> transparente encima del botón, para poder probarlo igual.
  let {
    options,
    value,
    label,
    onchange,
    disabled = false,
  }: {
    options: { value: T; label: string; badge?: string }[];
    value: T;
    label: string;
    onchange: (value: T) => void;
    disabled?: boolean;
  } = $props();

  const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  const selected = $derived(options.findIndex((o) => Object.is(o.value, value)));
  const current = $derived(options[selected]);

  let button: HTMLButtonElement | undefined = $state();
  // El menú anterior se suelta al abrir otro o al desmontar; no antes, porque sus
  // acciones pueden llegar después de que `popup` resuelva.
  let lastMenu: { close(): Promise<void> } | null = null;

  function release() {
    const menu = lastMenu;
    lastMenu = null;
    void menu?.close().catch(() => {});
  }

  onDestroy(release);

  function itemText(option: { label: string; badge?: string }): string {
    return option.badge ? `${option.label}  ·  ${option.badge}` : option.label;
  }

  async function open() {
    if (disabled || !button || options.length === 0) return;
    const [{ Menu }, { LogicalPosition }] = await Promise.all([
      import("@tauri-apps/api/menu"),
      import("@tauri-apps/api/dpi"),
    ]);
    release();
    const menu = await Menu.new({
      items: options.map((option, i) => ({
        text: itemText(option),
        checked: i === selected,
        action: () => {
          if (!Object.is(option.value, value)) onchange(option.value);
        },
      })),
    });
    lastMenu = menu;
    const rect = button.getBoundingClientRect();
    await menu.popup(new LogicalPosition(rect.left, rect.bottom + 2));
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      void open();
    }
  }

  function onFallbackChange(event: Event & { currentTarget: HTMLSelectElement }) {
    const option = options[Number(event.currentTarget.value)];
    if (option && !Object.is(option.value, value)) onchange(option.value);
  }
</script>

<span class="select" class:disabled>
  {#if inTauri}
    <button
      bind:this={button}
      type="button"
      class="popup"
      aria-haspopup="menu"
      {disabled}
      onclick={() => void open()}
      {onkeydown}
    >
      <span class="visually-hidden">{label}: </span>
      <span class="text">{current?.label ?? "—"}</span>
      {#if current?.badge}<span class="badge num">{current.badge}</span>{/if}
      <span class="arrows"><Icon name="chevron-down" /></span>
    </button>
  {:else}
    <span class="popup" aria-hidden="true">
      <span class="text">{current?.label ?? "—"}</span>
      {#if current?.badge}<span class="badge num">{current.badge}</span>{/if}
      <span class="arrows"><Icon name="chevron-down" /></span>
    </span>
    <select class="fallback" aria-label={label} {disabled} value={String(selected)} onchange={onFallbackChange}>
      {#each options as option, i (i)}
        <option value={String(i)}>{itemText(option)}</option>
      {/each}
    </select>
  {/if}
</span>

<style>
  .select {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
    min-width: 0;
  }

  .popup {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    min-width: 0;
    height: var(--control-height);
    padding: 0 var(--s-1) 0 var(--s-2);
    border: none;
    border-radius: var(--r-sm);
    background: var(--raised);
    box-shadow: var(--shadow-1);
    color: var(--label);
    font: var(--t-body);
  }

  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    padding: 0 var(--s-1);
    border-radius: var(--r-pill);
    background: var(--red);
    color: var(--on-hero);
    font: var(--t-caption);
  }

  .arrows {
    display: inline-flex;
    padding: 1px;
    border-radius: var(--r-xs);
    background: var(--accent);
    color: var(--accent-text);
    transform: scale(0.85);
  }

  .fallback {
    position: absolute;
    inset: 0;
    width: 100%;
    opacity: 0;
  }

  .select:has(.fallback:focus-visible) .popup {
    outline: 3px solid var(--focus-ring);
    outline-offset: 1px;
  }

  .disabled {
    opacity: 0.5;
  }
</style>
