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
  import { t } from "../../lib/i18n/index.svelte";
  import type { AppearanceMode } from "../../lib/bindings/AppearanceMode";
  import type { Editor } from "../../lib/bindings/Editor";
  import type { Language } from "../../lib/bindings/Language";
  import type { Settings } from "../../lib/bindings/Settings";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import type { SourceCheck } from "../../lib/bindings/SourceCheck";

  let { snapshot }: { snapshot: Snapshot | null } = $props();

  const APPEARANCE: { value: AppearanceMode; label: string }[] = $derived([
    { value: "system", label: t("settings.general.appearance.system") },
    { value: "light", label: t("settings.general.appearance.light") },
    { value: "dark", label: t("settings.general.appearance.dark") },
  ]);

  // Cada idioma con su propio nombre, como en macOS: no se traducen.
  const LANGUAGES: { value: Language; label: string }[] = $derived([
    { value: "system", label: t("settings.general.language.system") },
    { value: "es", label: "Español" },
    { value: "en", label: "English" },
  ]);

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
      toast(t("settings.general.saveFailed", { error: errorMessage(e) }), { tone: "error" });
    } finally {
      if (pending === next) pending = null;
    }
  }

  // ── Fuente de las versiones de PHP ──
  // Una fuente propia solo se guarda después de probarla: así nunca queda
  // guardada una que no da versiones.
  const DEFAULT_SOURCE = "cristiancosano/php-builds";
  const SOURCE_MODES: { value: "default" | "custom"; label: string }[] = $derived([
    { value: "default", label: "php-builds" },
    { value: "custom", label: t("settings.general.phpSource.custom") },
  ]);
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
    toast(t("settings.general.phpSource.saved"), { tone: "success" });
  }

  function versionsText(versions: string[]): string {
    const shown = versions.slice(0, 4).join(", ");
    const list =
      versions.length > 4 ? t("settings.general.phpSource.andMore", { list: shown, count: versions.length - 4 }) : shown;
    return t("settings.general.phpSource.versions", { count: versions.length, list });
  }
</script>

{#if !settings}
  <Skeleton lines={5} />
{:else}
  <Group footer={t("settings.general.menuBar.footer")}>
    <Row title={t("settings.general.menuBar.keep")}>
      {#snippet trailing()}
        <Switch
          checked={settings.keep_in_menu_bar}
          label={t("settings.general.menuBar.keep")}
          onchange={(v) => void change("keep_in_menu_bar", v)}
        />
      {/snippet}
    </Row>
    <Row title={t("settings.general.menuBar.showPhp")}>
      {#snippet trailing()}
        <Switch
          checked={settings.show_php_in_menu_bar}
          label={t("settings.general.menuBar.showPhpLabel")}
          onchange={(v) => void change("show_php_in_menu_bar", v)}
        />
      {/snippet}
    </Row>
    <Row title={t("settings.general.menuBar.watch")} subtitle={t("settings.general.menuBar.watchSubtitle")}>
      {#snippet trailing()}
        <Switch
          checked={settings.watch_services}
          label={t("settings.general.menuBar.watch")}
          onchange={(v) => void change("watch_services", v)}
        />
      {/snippet}
    </Row>
  </Group>

  <Group>
    <Row title={t("settings.general.appearance.label")}>
      {#snippet trailing()}
        <SegmentedControl
          options={APPEARANCE}
          value={settings.appearance}
          label={t("settings.general.appearance.label")}
          onchange={(v) => void change("appearance", v)}
        />
      {/snippet}
    </Row>
    <Row title={t("settings.general.language.label")} subtitle={t("settings.general.language.subtitle")}>
      {#snippet trailing()}
        <Select
          options={LANGUAGES}
          value={settings.language}
          label={t("settings.general.language.label")}
          onchange={(v) => void change("language", v)}
        />
      {/snippet}
    </Row>
    <Row title={t("settings.general.editor.label")} subtitle={t("settings.general.editor.subtitle")}>
      {#snippet trailing()}
        <Select
          options={EDITORS}
          value={settings.editor}
          label={t("settings.general.editor.label")}
          onchange={(v) => void change("editor", v)}
        />
      {/snippet}
    </Row>
  </Group>

  <Group title={t("settings.general.phpSource.title")} footer={t("settings.general.phpSource.footer")}>
    <Row
      title={t("settings.general.phpSource.origin")}
      subtitle={settings.php_source ?? t("settings.general.phpSource.recommended", { source: DEFAULT_SOURCE })}
    >
      {#snippet trailing()}
        <SegmentedControl
          options={SOURCE_MODES}
          value={sourceMode}
          label={t("settings.general.phpSource.modeLabel")}
          onchange={chooseMode}
        />
      {/snippet}
    </Row>
    {#if sourceMode === "custom"}
      <Row title={t("settings.general.phpSource.field")}>
        {#snippet trailing()}
          <input
            class="field"
            type="text"
            spellcheck="false"
            autocomplete="off"
            placeholder={t("settings.general.phpSource.placeholder")}
            aria-label={t("settings.general.phpSource.fieldLabel")}
            bind:value={draft}
            oninput={resetCheck}
            onkeydown={(event) => {
              if (event.key === "Enter") void probe();
            }}
          />
          <Button onclick={() => void probe()} disabled={checking || draft.trim() === ""}>{t("settings.general.phpSource.test")}</Button>
        {/snippet}
      </Row>
      {#if checking}
        <Row title={t("settings.general.phpSource.checking")}>
          {#snippet trailing()}<ProgressRing label={t("settings.general.phpSource.checkingLabel")} />{/snippet}
        </Row>
      {:else if checkError}
        <Row title={t("settings.general.phpSource.invalid")} subtitle={checkError} />
      {:else if check}
        <Row title={versionsText(check.versions)} subtitle={check.url}>
          {#snippet trailing()}
            {#if draftIsSaved}
              <span class="saved">{t("settings.general.phpSource.inUse")}</span>
            {:else}
              <Button variant="primary" onclick={() => void useSource()}>{t("settings.general.phpSource.use")}</Button>
            {/if}
          {/snippet}
        </Row>
      {/if}
    {/if}
  </Group>

  <Group footer={t("settings.general.assistant.footer")}>
    <Row title={t("settings.general.assistant.title")}>
      {#snippet trailing()}
        <Button onclick={() => go("#/asistente")}>{t("settings.general.assistant.rerun")}</Button>
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
