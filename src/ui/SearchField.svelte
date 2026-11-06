<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n/index.svelte";

  const DEBOUNCE_MS = 150;

  let {
    value,
    placeholder,
    regex = false,
    invalid = false,
    oninput,
    focusToken,
    label,
  }: {
    value: string;
    placeholder: string;
    /** El texto se interpreta como expresión regular (solo cambia la pista visual). */
    regex?: boolean;
    invalid?: boolean;
    /** Se llama 150 ms después de la última tecla; borrar o pulsar Intro no esperan. */
    oninput: (value: string) => void;
    /** Al cambiar de valor, el campo toma el foco (⌘F llega por `ui:command`). */
    focusToken?: number;
    label?: string;
  } = $props();

  let input: HTMLInputElement | undefined = $state();
  let text = $state("");
  let emitted = "";
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Si el valor cambia desde fuera (otro filtro, la ruta), el campo lo refleja.
  $effect.pre(() => {
    if (value !== emitted) {
      emitted = value;
      text = value;
    }
  });

  // El valor inicial no da el foco: solo los cambios posteriores.
  let lastToken = untrack(() => focusToken);
  $effect(() => {
    if (focusToken !== undefined && focusToken !== lastToken) {
      input?.focus();
      input?.select();
    }
    lastToken = focusToken;
  });

  function emit(next: string) {
    clearTimeout(timer);
    timer = undefined;
    if (next === emitted) return;
    emitted = next;
    oninput(next);
  }

  function handleInput(event: Event & { currentTarget: HTMLInputElement }) {
    text = event.currentTarget.value;
    clearTimeout(timer);
    const next = text;
    timer = setTimeout(() => emit(next), DEBOUNCE_MS);
  }

  function clear() {
    text = "";
    emit("");
    input?.focus();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      emit(text);
    } else if (event.key === "Escape" && text !== "") {
      event.preventDefault();
      event.stopPropagation();
      clear();
    }
  }

  onDestroy(() => clearTimeout(timer));
</script>

<span class="search" class:invalid>
  <span class="lens"><Icon name="search" /></span>
  <input
    bind:this={input}
    type="text"
    spellcheck="false"
    autocomplete="off"
    autocapitalize="off"
    aria-label={label ?? placeholder}
    aria-invalid={invalid || undefined}
    {placeholder}
    value={text}
    oninput={handleInput}
    {onkeydown}
  />
  {#if regex}
    <span class="regex mono" aria-hidden="true">.*</span>
  {/if}
  {#if text !== ""}
    <button type="button" class="clear" aria-label={t("ui.search.clear")} title={t("ui.search.clear")} onclick={clear}>
      <Icon name="x" />
    </button>
  {/if}
</span>

<style>
  .search {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    min-width: 0;
    height: var(--control-height-large);
    padding: 0 var(--s-1) 0 var(--s-2);
    border-radius: var(--r-sm);
    background: var(--fill-2);
    color: var(--label-2);
    box-shadow: 0 0 0 1px transparent;
    transition: box-shadow var(--d-1) var(--ease);
  }

  .search:focus-within {
    box-shadow: 0 0 0 3px var(--focus-ring);
  }

  .search.invalid {
    box-shadow: 0 0 0 1.5px var(--red);
  }

  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--label);
    font: var(--t-body);
  }

  input::placeholder {
    color: var(--label-3);
  }

  input:focus-visible {
    outline: none;
  }

  .lens {
    display: inline-flex;
    color: var(--label-3);
  }

  .regex {
    font: var(--t-caption);
    font-family: var(--mono);
    color: var(--label-3);
  }

  .clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 1px;
    border: none;
    border-radius: var(--r-pill);
    background: var(--fill-3);
    color: var(--label-2);
  }
</style>
