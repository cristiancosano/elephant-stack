<script lang="ts">
  // Ajustes ▸ Agentes (05 §6.4): el modo del canal, la orden `elephant`, el
  // plugin de Claude Code, la guía para otros agentes, los fragmentos MCP y la
  // actividad. La app nunca escribe la configuración de otros programas: solo
  // ejecuta, tras confirmarlo, los dos comandos de `claude plugin`.
  import { onMount } from "svelte";

  import * as ipc from "../../lib/ipc";
  import type { AuditEntry } from "../../lib/bindings/AuditEntry";
  import type { McpMode } from "../../lib/bindings/McpMode";
  import type { McpSnippets } from "../../lib/bindings/McpSnippets";
  import type { McpStatus } from "../../lib/bindings/McpStatus";
  import type { McpTestResult } from "../../lib/bindings/McpTestResult";
  import type { Settings } from "../../lib/bindings/Settings";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import { formatRelative } from "../../lib/format";
  import { copyText } from "../../lib/logs/clipboard";
  import Badge from "../../ui/Badge.svelte";
  import Button from "../../ui/Button.svelte";
  import Group from "../../ui/Group.svelte";
  import Icon from "../../ui/Icon.svelte";
  import Row from "../../ui/Row.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Sheet from "../../ui/Sheet.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";
  import Switch from "../../ui/Switch.svelte";
  import { toast } from "../../ui/toasts.svelte";

  let { snapshot }: { snapshot: Snapshot | null } = $props();

  const MODES: { value: McpMode; label: string }[] = [
    { value: "read_only", label: "Solo lectura" },
    { value: "confirm", label: "Confirmar" },
    { value: "trusted", label: "De confianza" },
  ];

  const MODE_HELP: Record<McpMode, string> = {
    read_only: "Los agentes pueden leer el estado y los logs, pero no cambiar nada.",
    confirm:
      "Cada cambio que pide un agente (reiniciar un servicio, cambiar el PHP, crear un proyecto) muestra un diálogo de Elephant Stack. Los scripts y git hooks cuentan como agentes.",
    trusted: "Los agentes cambian el entorno sin preguntar. Úsalo solo si confías en lo que ejecutan.",
  };

  const ALLOW_SNIPPET = `"permissions": {\n  "allow": ["Bash(elephant *)"]\n}`;
  const ASK_SNIPPET = `"permissions": {\n  "allow": ["Bash(elephant *)"],\n  "ask": ["Bash(elephant service *)", "Bash(elephant project create *)"]\n}`;

  const TOOL_NAMES: Record<string, string> = {
    initialize: "Conexión MCP",
    elephant_status: "Estado",
    service_control: "Servicio",
    php_set_global: "PHP global",
    php_set_project: "PHP del proyecto",
    component_install: "Instalar componente",
    project_start_services: "Arrancar servicios del proyecto",
    project_create: "Crear proyecto",
    logs_tail: "Leer logs",
    logs_search: "Buscar en logs",
    logs_errors_summary: "Resumen de errores",
    logs_entry: "Entrada de log",
    logs_sources: "Fuentes de logs",
    logs_watch: "Vigilar logs",
  };

  let status = $state.raw<McpStatus | null>(null);
  let statusError: string | null = $state(null);
  let snippets = $state.raw<McpSnippets | null>(null);
  let activity = $state.raw<AuditEntry[]>([]);
  let test = $state.raw<McpTestResult | null>(null);
  let testError: string | null = $state(null);
  let testing = $state(false);
  let linking = $state(false);
  let connectOpen = $state(false);
  let connecting = $state(false);
  let disableOpen = $state(false);
  let pendingMode = $state<McpMode | null>(null);

  const settings = $derived<Settings | null>(snapshot?.settings ?? null);
  const mode = $derived<McpMode>(pendingMode ?? settings?.mcp.mode ?? status?.mode ?? "confirm");
  const lastUsed = $derived(snapshot?.mcp.last_used_ms ?? status?.last_used_ms ?? null);
  const hasMcp = $derived(!!snippets?.claude_desktop);
  const claudeMcpCommand = $derived(
    status ? `claude mcp add-json --scope user elephant '{"type":"stdio","command":"${status.link_path}","args":["mcp"]}'` : "",
  );

  onMount(() => {
    void load();
  });

  async function load() {
    try {
      status = await ipc.mcpStatus();
      statusError = null;
    } catch (e) {
      statusError = ipc.errorMessage(e);
    }
    try {
      snippets = await ipc.mcpSnippets();
    } catch {
      snippets = null;
    }
    await loadActivity();
  }

  async function loadActivity() {
    try {
      activity = await ipc.mcpActivity(10);
    } catch {
      activity = [];
    }
  }

  async function changeMode(next: McpMode) {
    if (!settings || next === mode) return;
    pendingMode = next;
    try {
      await ipc.setSettings({ ...settings, mcp: { ...settings.mcp, mode: next } });
    } catch (e) {
      toast(`No se pudo cambiar el modo: ${ipc.errorMessage(e)}`, { tone: "error" });
    } finally {
      pendingMode = null;
    }
  }

  async function setLinked(on: boolean) {
    if (!on && status?.claude_registered) {
      disableOpen = true;
      return;
    }
    await runLink(on);
  }

  async function runLink(on: boolean) {
    linking = true;
    try {
      status = on ? await ipc.mcpEnable() : await ipc.mcpDisable();
      if (on) snippets = await ipc.mcpSnippets().catch(() => snippets);
    } catch (e) {
      toast(ipc.errorMessage(e), { tone: "error" });
    } finally {
      linking = false;
      disableOpen = false;
    }
  }

  async function connect() {
    connecting = true;
    try {
      status = await ipc.mcpConnectClaude();
      connectOpen = false;
      toast(
        status.claude_registered
          ? "Claude Code ya tiene el plugin de Elephant Stack. Funciona en las sesiones nuevas."
          : "Se han ejecutado los comandos, pero Claude Code no muestra el plugin todavía.",
        { tone: status.claude_registered ? "success" : "info" },
      );
    } catch (e) {
      toast(ipc.errorMessage(e), { tone: "error" });
    } finally {
      connecting = false;
    }
  }

  async function runTest() {
    testing = true;
    testError = null;
    try {
      test = await ipc.mcpTest();
    } catch (e) {
      test = null;
      testError = ipc.errorMessage(e);
    } finally {
      testing = false;
    }
  }

  async function copy(text: string, what: string) {
    try {
      await copyText(text);
      toast(`${what} copiado`, { tone: "success" });
    } catch (e) {
      toast(ipc.errorMessage(e), { tone: "error" });
    }
  }

  function modeLabel(m: McpMode): string {
    return MODES.find((o) => o.value === m)?.label ?? m;
  }

  function clientLabel(client: string | null): string {
    if (!client) return "Desconocido";
    if (client.startsWith("claude-code")) return client.replace("claude-code", "Claude Code");
    if (client.startsWith("mcp:")) return client.slice(4);
    if (client === "tty") return "Terminal";
    if (client === "script") return "Script";
    return client;
  }

  function entryTitle(e: AuditEntry): string {
    const name = TOOL_NAMES[e.tool] ?? e.tool;
    const args = (e.args ?? {}) as Record<string, unknown>;
    const detail = [args.operation, args.name, args.version, args.id, args.project]
      .filter((v): v is string => typeof v === "string" && v.length > 0)
      .join(" ");
    return detail ? `${name}: ${detail}` : name;
  }

  function entrySubtitle(e: AuditEntry): string {
    const when = Date.parse(e.at);
    const parts = [clientLabel(e.client)];
    if (!Number.isNaN(when)) parts.push(formatRelative(when));
    if (e.confirmed_by === "dialog" || e.confirmed_by === "elicitation") parts.push("confirmado");
    if (e.error) parts.push(e.error);
    return parts.join(" · ");
  }
</script>

<Group
  title="Modo"
  footer={`${MODE_HELP[mode]} Las lecturas nunca piden confirmación. Ni los agentes ni la orden elephant pueden cambiar el modo.`}
>
  <Row title="Qué pueden hacer los agentes">
    {#snippet leading()}<Icon name="lock" />{/snippet}
    {#snippet trailing()}
      <SegmentedControl options={MODES} value={mode} label="Modo del canal de agentes" onchange={(v) => void changeMode(v)} />
    {/snippet}
  </Row>
</Group>

{#if statusError}
  <Group title="Orden elephant">
    <Row title="No se ha podido leer el estado" subtitle={statusError} />
  </Group>
{:else if !status}
  <Skeleton lines={4} />
{:else}
  <Group
    title="Orden elephant"
    footer="Los agentes usan la orden elephant para leer logs, ver el estado y, con tu permiso, cambiar servicios o el PHP. No queda ningún proceso abierto: cada orden se ejecuta y termina."
  >
    <Row
      title="Activar la orden elephant"
      subtitle={status.linked ? `${status.link_path}${status.target ? ` → ${status.target}` : ""}` : "Desactivada"}
    >
      {#snippet leading()}<Icon name="terminal" />{/snippet}
      {#snippet trailing()}
        <Switch checked={status?.linked ?? false} disabled={linking} label="Activar la orden elephant" onchange={(v) => void setLinked(v)} />
      {/snippet}
    </Row>
    <Row
      title="Probar conexión"
      subtitle={testError ??
        (test
          ? `${test.tools} operaciones · modo ${modeLabel(test.mode).toLowerCase()} · ${test.latency_ms} ms`
          : lastUsed
            ? `Usado ${formatRelative(lastUsed)}`
            : "Todavía no lo ha usado ningún agente")}
    >
      {#snippet trailing()}
        <Button busy={testing} onclick={() => void runTest()}>Probar</Button>
      {/snippet}
    </Row>
  </Group>

  <Group
    title="Claude Code"
    footer="El plugin trae una skill que enseña a Claude Code a usar elephant y no arranca ningún servidor. Durante la skill, las órdenes elephant no piden permiso en Claude Code; los cambios siguen pasando por el diálogo de Elephant Stack."
  >
    <Row
      title="Plugin de Elephant Stack"
      subtitle={status.claude_path ? status.claude_path : "No se encuentra la orden claude"}
    >
      {#snippet leading()}<Icon name="agent" />{/snippet}
      {#snippet trailing()}
        <span class="state">
          {#if status?.claude_registered}
            <Badge tone="green" label="Conectado" />
          {:else}
            <Badge label="Sin conectar" />
          {/if}
          <Button
            variant={status?.claude_registered ? "secondary" : "primary"}
            disabled={!status?.claude_path}
            onclick={() => (connectOpen = true)}
          >
            {status?.claude_registered ? "Volver a conectar…" : "Conectar…"}
          </Button>
        </span>
      {/snippet}
    </Row>
    <Row
      title="Permisos fuera de la skill"
      subtitle="Pégalo en ~/.claude/settings.json para que elephant no pida permiso cada vez."
    >
      {#snippet trailing()}
        <span class="state">
          <Button variant="plain" onclick={() => void copy(ASK_SNIPPET, "Fragmento con más avisos")}>Con más avisos</Button>
          <Button onclick={() => void copy(ALLOW_SNIPPET, "Fragmento de permisos")}>Copiar</Button>
        </span>
      {/snippet}
    </Row>
  </Group>

  <Group
    title="Otros agentes con terminal"
    footer="Cursor, Codex o Gemini CLI pueden usar la misma orden elephant. Pega la guía en AGENTS.md o en las reglas del agente."
  >
    <Row title="Guía de uso de elephant">
      {#snippet leading()}<Icon name="copy" />{/snippet}
      {#snippet trailing()}
        <Button disabled={!snippets?.shell_guide} onclick={() => snippets && void copy(snippets.shell_guide, "Guía")}>Copiar guía</Button>
      {/snippet}
    </Row>
  </Group>

  {#if hasMcp && snippets}
    <Group
      title="Clientes MCP"
      footer="Para los clientes que solo hablan MCP. Se arranca con elephant mcp cuando el cliente lo pide y aplica el mismo modo y las mismas confirmaciones."
    >
      <Row title="Claude Desktop" subtitle="claude_desktop_config.json">
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.claude_desktop, "Fragmento de Claude Desktop")}>Copiar</Button>{/snippet}
      </Row>
      <Row title="Cursor" subtitle="~/.cursor/mcp.json">
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.cursor, "Fragmento de Cursor")}>Copiar</Button>{/snippet}
      </Row>
      <Row title="VS Code" subtitle=".vscode/mcp.json">
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.vscode, "Fragmento de VS Code")}>Copiar</Button>{/snippet}
      </Row>
      <Row title="Proyecto (.mcp.json)" subtitle="Sin rutas personales: se puede subir al repositorio">
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.project_json, "Fragmento .mcp.json")}>Copiar</Button>{/snippet}
      </Row>
      <Row title="Avanzado: Claude Code por MCP" subtitle="Duplica el plugin y abre un proceso en cada sesión: no hace falta si usas el plugin.">
        {#snippet trailing()}<Button variant="plain" onclick={() => void copy(claudeMcpCommand, "Comando")}>Copiar comando</Button>{/snippet}
      </Row>
    </Group>
  {/if}

  <Group title="Actividad" footer="Se registran los cambios (también los denegados) y las lecturas de logs, sin su contenido.">
    {#if activity.length === 0}
      <Row title="Sin actividad todavía" />
    {:else}
      {#each activity as entry, i (`${entry.at}-${i}`)}
        <Row title={entryTitle(entry)} subtitle={entrySubtitle(entry)}>
          {#snippet trailing()}
            {#if !entry.ok}
              <Badge tone="red" label={entry.error?.includes("deneg") || entry.error?.includes("Solo lectura") ? "Denegado" : "Error"} />
            {/if}
          {/snippet}
        </Row>
      {/each}
    {/if}
  </Group>
{/if}

<Sheet open={connectOpen} title="Conectar Claude Code" onclose={() => (connectOpen = false)}>
  <p class="text">Elephant Stack escribirá el plugin en su carpeta y ejecutará estos comandos de Claude Code:</p>
  <pre class="code">{snippets?.claude_command ?? "claude plugin marketplace add ~/.elephant-stack/share/claude-plugin\nclaude plugin install elephant@elephant-stack -s user"}</pre>
  <p class="text">No se modifica ningún otro ajuste de Claude Code. El plugin funciona en las sesiones nuevas.</p>
  <div class="actions">
    <Button onclick={() => (connectOpen = false)} disabled={connecting}>Cancelar</Button>
    <Button variant="primary" busy={connecting} onclick={() => void connect()}>Conectar</Button>
  </div>
</Sheet>

<Sheet open={disableOpen} title="Desactivar la orden elephant" onclose={() => (disableOpen = false)}>
  <p class="text">
    Se quitará la orden elephant y se desinstalará el plugin de Claude Code (claude plugin uninstall
    elephant@elephant-stack). Los agentes dejarán de poder usar Elephant Stack.
  </p>
  <div class="actions">
    <Button onclick={() => (disableOpen = false)} disabled={linking}>Cancelar</Button>
    <Button variant="destructive" busy={linking} onclick={() => void runLink(false)}>Desactivar</Button>
  </div>
</Sheet>

<style>
  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
  }

  .text {
    margin: 0 0 var(--s-3);
    font: var(--t-body);
    color: var(--label-2);
  }

  .code {
    margin: 0 0 var(--s-3);
    padding: var(--s-3);
    border-radius: var(--r-md);
    background: var(--fill);
    font: var(--t-log);
    font-family: var(--mono);
    color: var(--label);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s-2);
    margin-top: var(--s-4);
  }
</style>
