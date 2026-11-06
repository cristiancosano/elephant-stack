<script lang="ts">
  // Ajustes ▸ General (01 §7.7). Cada cambio se guarda al momento con
  // `set_settings`; el valor bueno vuelve en el siguiente snapshot.
  import Group from "../../ui/Group.svelte";
  import Row from "../../ui/Row.svelte";
  import Switch from "../../ui/Switch.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Select from "../../ui/Select.svelte";
  import Button from "../../ui/Button.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";
  import ProgressRing from "../../ui/ProgressRing.svelte";
  import { toast } from "../../ui/toasts.svelte";
  import { errorMessage, phpSourceCheck, setSettings } from "../../lib/ipc";
  import { go } from "../../lib/router.svelte";
  import type { AppearanceMode } from "../../lib/bindings/AppearanceMode";
  import type { Editor } from "../../lib/bindings/Editor";
  import type { Settings } from "../../lib/bindings/Settings";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import type { SourceCheck } from "../../lib/bindings/SourceCheck";

  let { snapshot }: { snapshot: Snapshot | null } = $props();

  const APPEARANCE: { value: AppearanceMode; label: string }[] = [
    { value: "system", label: "Sistema" },
    { value: "light", label: "Claro" },
    { value: "dark", label: "Oscuro" },
  ];

  const EDITORS: { value: Editor; label: string }[] = [
    { value: "vscode", label: "Visual Studio Code" },
    { value: "phpstorm", label: "PhpStorm" },
    { value: "cursor", label: "Cursor" },
  ];

  // Mientras se guarda se enseña lo pedido, para que el control no vuelva atrás
  // hasta que llegue la respuesta.
  let pending = $state<Settings | null>(null);
  const settings = $derived(pending ?? snapshot?.settings ?? null);

  async function change<K extends keyof Settings>(key: K, value: Settings[K]) {
    if (!settings || settings[key] === value) return;
    const next = { ...settings, [key]: value };
    pending = next;
    try {
      await setSettings(next);
    } catch (e) {
      toast(`No se pudo guardar el ajuste: ${errorMessage(e)}`, { tone: "error" });
    } finally {
      if (pending === next) pending = null;
    }
  }

  // ── Fuente de las versiones de PHP ──
  // Una fuente propia solo se guarda después de probarla: así nunca queda
  // guardada una que no da versiones.
  const DEFAULT_SOURCE = "cristiancosano/php-builds";
  const SOURCE_MODES: { value: "default" | "custom"; label: string }[] = [
    { value: "default", label: "php-builds" },
    { value: "custom", label: "Personalizada" },
  ];
  let customOpen = $state(false);
  let draft = $state("");
  let draftLoaded = false;
  let checking = $state(false);
  let check = $state<SourceCheck | null>(null);
  let checkError = $state<string | null>(null);

  const sourceMode = $derived<"default" | "custom">(customOpen || settings?.php_source ? "custom" : "default");
  const draftIsSaved = $derived((settings?.php_source ?? "") === draft.trim());

  $effect(() => {
    if (draftLoaded || !settings) return;
    draft = settings.php_source ?? "";
    draftLoaded = true;
  });

  function resetCheck() {
    check = null;
    checkError = null;
  }

  function chooseMode(mode: "default" | "custom") {
    resetCheck();
    if (mode === "custom") {
      customOpen = true;
      return;
    }
    customOpen = false;
    draft = "";
    void change("php_source", null);
  }

  async function probe() {
    const source = draft.trim();
    if (!source || checking) return;
    checking = true;
    resetCheck();
    try {
      check = await phpSourceCheck(source);
    } catch (e) {
      checkError = errorMessage(e);
    } finally {
      checking = false;
    }
  }

  async function useSource() {
    await change("php_source", draft.trim());
    resetCheck();
    toast("Las versiones de PHP se descargarán de la nueva fuente.", { tone: "success" });
  }

  function versionsText(versions: string[]): string {
    const shown = versions.slice(0, 4).join(", ");
    const more = versions.length > 4 ? ` y ${versions.length - 4} más` : "";
    return `${versions.length === 1 ? "1 versión" : `${versions.length} versiones`}: ${shown}${more}`;
  }
</script>

{#if !settings}
  <Skeleton lines={5} />
{:else}
  <Group
    footer="Al cerrar o minimizar la ventana, Elephant Stack desaparece del Dock y sigue en la barra de menús. Para salir del todo, usa «Salir» en su menú o ⌘Q."
  >
    <Row title="Seguir en la barra de menús al cerrar">
      {#snippet trailing()}
        <Switch
          checked={settings.keep_in_menu_bar}
          label="Seguir en la barra de menús al cerrar"
          onchange={(v) => void change("keep_in_menu_bar", v)}
        />
      {/snippet}
    </Row>
    <Row title="Mostrar la versión de PHP junto al icono">
      {#snippet trailing()}
        <Switch
          checked={settings.show_php_in_menu_bar}
          label="Mostrar la versión de PHP junto al icono de la barra de menús"
          onchange={(v) => void change("show_php_in_menu_bar", v)}
        />
      {/snippet}
    </Row>
    <Row title="Vigilar servicios en segundo plano" subtitle="Avisa en la barra de menús si un servicio se detiene">
      {#snippet trailing()}
        <Switch
          checked={settings.watch_services}
          label="Vigilar servicios en segundo plano"
          onchange={(v) => void change("watch_services", v)}
        />
      {/snippet}
    </Row>
  </Group>

  <Group>
    <Row title="Apariencia">
      {#snippet trailing()}
        <SegmentedControl
          options={APPEARANCE}
          value={settings.appearance}
          label="Apariencia"
          onchange={(v) => void change("appearance", v)}
        />
      {/snippet}
    </Row>
    <Row title="Editor para abrir archivos" subtitle="Los archivos de los logs y las carpetas de los proyectos">
      {#snippet trailing()}
        <Select
          options={EDITORS}
          value={settings.editor}
          label="Editor para abrir archivos"
          onchange={(v) => void change("editor", v)}
        />
      {/snippet}
    </Row>
  </Group>

  <Group
    title="Versiones de PHP"
    footer="Una fuente propia debe publicar igual que php-builds: un index.json en la release «catalog», o releases php-X.Y.Z con el paquete php-X.Y.Z-macos-arm64.tar.gz y su .sha256. Esos binarios se ejecutan como tu php, y el SHA-256 viene de la misma fuente: usa solo fuentes de confianza."
  >
    <Row
      title="Origen de las descargas"
      subtitle={settings.php_source ?? `${DEFAULT_SOURCE} (recomendado)`}
    >
      {#snippet trailing()}
        <SegmentedControl
          options={SOURCE_MODES}
          value={sourceMode}
          label="Origen de las versiones de PHP"
          onchange={chooseMode}
        />
      {/snippet}
    </Row>
    {#if sourceMode === "custom"}
      <Row title="Repositorio o índice">
        {#snippet trailing()}
          <input
            class="field"
            type="text"
            spellcheck="false"
            autocomplete="off"
            placeholder="usuario/repo o https://…/index.json"
            aria-label="Repositorio de GitHub o URL del índice de versiones de PHP"
            bind:value={draft}
            oninput={resetCheck}
            onkeydown={(event) => {
              if (event.key === "Enter") void probe();
            }}
          />
          <Button onclick={() => void probe()} disabled={checking || draft.trim() === ""}>Probar</Button>
        {/snippet}
      </Row>
      {#if checking}
        <Row title="Comprobando la fuente…">
          {#snippet trailing()}<ProgressRing label="Comprobando la fuente" />{/snippet}
        </Row>
      {:else if checkError}
        <Row title="No sirve como fuente" subtitle={checkError} />
      {:else if check}
        <Row title={versionsText(check.versions)} subtitle={check.url}>
          {#snippet trailing()}
            {#if draftIsSaved}
              <span class="saved">En uso</span>
            {:else}
              <Button variant="primary" onclick={() => void useSource()}>Usar esta fuente</Button>
            {/if}
          {/snippet}
        </Row>
      {/if}
    {/if}
  </Group>

  <Group footer="Revisa qué servicios y versiones de PHP quieres, sin perder lo que ya tienes.">
    <Row title="Asistente de configuración">
      {#snippet trailing()}
        <Button onclick={() => go("#/asistente")}>Volver a ejecutar el asistente…</Button>
      {/snippet}
    </Row>
  </Group>
{/if}

<style>
  .field {
    width: 280px;
    height: var(--control-height-large);
    padding: 0 var(--s-2);
    border: none;
    border-radius: var(--r-sm);
    background: var(--fill-2);
    color: var(--label);
    font: var(--t-body);
    box-shadow: 0 0 0 1px transparent;
    transition: box-shadow var(--d-1) var(--ease);
  }

  .field::placeholder {
    color: var(--label-3);
  }

  .field:focus-visible {
    outline: none;
    box-shadow: 0 0 0 3px var(--focus-ring);
  }

  .saved {
    color: var(--label-2);
    font: var(--t-callout);
  }
</style>
