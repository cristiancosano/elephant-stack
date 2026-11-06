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
  import { toast } from "../../ui/toasts.svelte";
  import { errorMessage, setSettings } from "../../lib/ipc";
  import { go } from "../../lib/router.svelte";
  import type { AppearanceMode } from "../../lib/bindings/AppearanceMode";
  import type { Editor } from "../../lib/bindings/Editor";
  import type { Settings } from "../../lib/bindings/Settings";
  import type { Snapshot } from "../../lib/bindings/Snapshot";

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

  <Group footer="Revisa qué servicios y versiones de PHP quieres, sin perder lo que ya tienes.">
    <Row title="Asistente de configuración">
      {#snippet trailing()}
        <Button onclick={() => go("#/asistente")}>Volver a ejecutar el asistente…</Button>
      {/snippet}
    </Row>
  </Group>
{/if}
