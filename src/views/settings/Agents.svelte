<script lang="ts">
  // Ajustes ▸ Agentes (05 §6.4): el modo del canal, la orden `elephant`, el
  // plugin de Claude Code, la guía para otros agentes, los fragmentos MCP y la
  // actividad. La app nunca escribe la configuración de otros programas: solo
  // ejecuta, tras confirmarlo, los dos comandos de `claude plugin`.
  // Textos del área diferida «agents» (docs/i18n.md): viajan con esta vista.
  import "../../lib/i18n/locales/agents";
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
  import { t } from "../../lib/i18n/index.svelte";
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

  const modes = $derived<{ value: McpMode; label: string }[]>([
    { value: "read_only", label: t("agents.mode.readOnly") },
    { value: "confirm", label: t("agents.mode.confirm") },
    { value: "trusted", label: t("agents.mode.trusted") },
  ]);

  function modeHelp(m: McpMode): string {
    if (m === "read_only") return t("agents.mode.help.readOnly");
    if (m === "trusted") return t("agents.mode.help.trusted");
    return t("agents.mode.help.confirm");
  }

  const ALLOW_SNIPPET = `"permissions": {\n  "allow": ["Bash(elephant *)"]\n}`;
  const ASK_SNIPPET = `"permissions": {\n  "allow": ["Bash(elephant *)"],\n  "ask": ["Bash(elephant service *)", "Bash(elephant project create *)"]\n}`;

  const TOOL_KEYS = {
    initialize: "agents.audit.tool.initialize",
    elephant_status: "agents.audit.tool.elephantStatus",
    service_control: "agents.audit.tool.serviceControl",
    php_set_global: "agents.audit.tool.phpSetGlobal",
    php_set_project: "agents.audit.tool.phpSetProject",
    component_install: "agents.audit.tool.componentInstall",
    project_start_services: "agents.audit.tool.projectStartServices",
    project_create: "agents.audit.tool.projectCreate",
    logs_tail: "agents.audit.tool.logsTail",
    logs_search: "agents.audit.tool.logsSearch",
    logs_errors_summary: "agents.audit.tool.logsErrorsSummary",
    logs_entry: "agents.audit.tool.logsEntry",
    logs_sources: "agents.audit.tool.logsSources",
    logs_watch: "agents.audit.tool.logsWatch",
  } as const;

  function toolName(tool: string): string {
    const key = TOOL_KEYS[tool as keyof typeof TOOL_KEYS];
    return key ? t(key) : tool;
  }

  /** La denegación la escribe el proceso que la decide, en su idioma: se reconoce en los dos. */
  function isDenied(error: string | null): boolean {
    const text = (error ?? "").toLowerCase();
    return ["deneg", "denied", "solo lectura", "read-only"].some((word) => text.includes(word));
  }

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
      toast(t("agents.mode.changeFailed", { error: ipc.errorMessage(e) }), { tone: "error" });
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
          ? t("agents.plugin.connected")
          : t("agents.plugin.notVisible"),
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

  async function copy(text: string, done: string) {
    try {
      await copyText(text);
      toast(done, { tone: "success" });
    } catch (e) {
      toast(ipc.errorMessage(e), { tone: "error" });
    }
  }

  function modeLabel(m: McpMode): string {
    return modes.find((o) => o.value === m)?.label ?? m;
  }

  function clientLabel(client: string | null): string {
    if (!client) return t("agents.audit.client.unknown");
    if (client.startsWith("claude-code")) return client.replace("claude-code", "Claude Code");
    if (client.startsWith("mcp:")) return client.slice(4);
    if (client === "tty") return t("agents.audit.client.terminal");
    if (client === "script") return t("agents.audit.client.script");
    return client;
  }

  function entryTitle(e: AuditEntry): string {
    const name = toolName(e.tool);
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
    if (e.confirmed_by === "dialog" || e.confirmed_by === "elicitation") parts.push(t("agents.audit.confirmed"));
    if (e.error) parts.push(e.error);
    return parts.join(" · ");
  }
</script>

<Group
  title={t("agents.mode.title")}
  footer={t("agents.mode.footer", { help: modeHelp(mode) })}
>
  <Row title={t("agents.mode.rowTitle")}>
    {#snippet leading()}<Icon name="lock" />{/snippet}
    {#snippet trailing()}
      <SegmentedControl options={modes} value={mode} label={t("agents.mode.label")} onchange={(v) => void changeMode(v)} />
    {/snippet}
  </Row>
</Group>

{#if statusError}
  <Group title={t("agents.link.title")}>
    <Row title={t("agents.link.statusError")} subtitle={statusError} />
  </Group>
{:else if !status}
  <Skeleton lines={4} />
{:else}
  <Group
    title={t("agents.link.title")}
    footer={t("agents.link.footer")}
  >
    <Row
      title={t("agents.link.switch")}
      subtitle={status.linked ? `${status.link_path}${status.target ? ` → ${status.target}` : ""}` : t("agents.link.off")}
    >
      {#snippet leading()}<Icon name="terminal" />{/snippet}
      {#snippet trailing()}
        <Switch checked={status?.linked ?? false} disabled={linking} label={t("agents.link.switch")} onchange={(v) => void setLinked(v)} />
      {/snippet}
    </Row>
    <Row
      title={t("agents.test.title")}
      subtitle={testError ??
        (test
          ? t("agents.test.result", {
              tools: test.tools,
              mode: modeLabel(test.mode).toLowerCase(),
              ms: test.latency_ms,
            })
          : lastUsed
            ? t("agents.test.lastUsed", { when: formatRelative(lastUsed) })
            : t("agents.test.neverUsed"))}
    >
      {#snippet trailing()}
        <Button busy={testing} onclick={() => void runTest()}>{t("agents.test.action")}</Button>
      {/snippet}
    </Row>
  </Group>

  <Group
    title={t("agents.claude.title")}
    footer={t("agents.claude.footer")}
  >
    <Row
      title={t("agents.claude.plugin")}
      subtitle={status.claude_path ? status.claude_path : t("agents.claude.notFound")}
    >
      {#snippet leading()}<Icon name="agent" />{/snippet}
      {#snippet trailing()}
        <span class="state">
          {#if status?.claude_registered}
            <Badge tone="green" label={t("agents.claude.connected")} />
          {:else}
            <Badge label={t("agents.claude.notConnected")} />
          {/if}
          <Button
            variant={status?.claude_registered ? "secondary" : "primary"}
            disabled={!status?.claude_path}
            onclick={() => (connectOpen = true)}
          >
            {status?.claude_registered ? t("agents.claude.reconnect") : t("agents.claude.connect")}
          </Button>
        </span>
      {/snippet}
    </Row>
    <Row
      title={t("agents.permissions.title")}
      subtitle={t("agents.permissions.subtitle")}
    >
      {#snippet trailing()}
        <span class="state">
          <Button variant="plain" onclick={() => void copy(ASK_SNIPPET, t("agents.copied.askSnippet"))}
            >{t("agents.permissions.moreAsks")}</Button
          >
          <Button onclick={() => void copy(ALLOW_SNIPPET, t("agents.copied.allowSnippet"))}>{t("common.copy")}</Button>
        </span>
      {/snippet}
    </Row>
  </Group>

  <Group
    title={t("agents.guide.title")}
    footer={t("agents.guide.footer")}
  >
    <Row title={t("agents.guide.row")}>
      {#snippet leading()}<Icon name="copy" />{/snippet}
      {#snippet trailing()}
        <Button disabled={!snippets?.shell_guide} onclick={() => snippets && void copy(snippets.shell_guide, t("agents.copied.guide"))}
          >{t("agents.guide.copy")}</Button
        >
      {/snippet}
    </Row>
  </Group>

  {#if hasMcp && snippets}
    <Group
      title={t("agents.mcp.title")}
      footer={t("agents.mcp.footer")}
    >
      <Row title="Claude Desktop" subtitle="claude_desktop_config.json">
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.claude_desktop, t("agents.copied.claudeDesktop"))}
            >{t("common.copy")}</Button
          >{/snippet}
      </Row>
      <Row title="Cursor" subtitle="~/.cursor/mcp.json">
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.cursor, t("agents.copied.cursor"))}
            >{t("common.copy")}</Button
          >{/snippet}
      </Row>
      <Row title="VS Code" subtitle=".vscode/mcp.json">
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.vscode, t("agents.copied.vscode"))}
            >{t("common.copy")}</Button
          >{/snippet}
      </Row>
      <Row title={t("agents.mcp.project")} subtitle={t("agents.mcp.projectSubtitle")}>
        {#snippet trailing()}<Button onclick={() => snippets && void copy(snippets.project_json, t("agents.copied.projectJson"))}
            >{t("common.copy")}</Button
          >{/snippet}
      </Row>
      <Row title={t("agents.mcp.advanced")} subtitle={t("agents.mcp.advancedSubtitle")}>
        {#snippet trailing()}<Button variant="plain" onclick={() => void copy(claudeMcpCommand, t("agents.copied.command"))}
            >{t("agents.mcp.copyCommand")}</Button
          >{/snippet}
      </Row>
    </Group>
  {/if}

  <Group title={t("agents.audit.title")} footer={t("agents.audit.footer")}>
    {#if activity.length === 0}
      <Row title={t("agents.audit.empty")} />
    {:else}
      {#each activity as entry, i (`${entry.at}-${i}`)}
        <Row title={entryTitle(entry)} subtitle={entrySubtitle(entry)}>
          {#snippet trailing()}
            {#if !entry.ok}
              <Badge tone="red" label={isDenied(entry.error) ? t("agents.audit.denied") : t("agents.audit.error")} />
            {/if}
          {/snippet}
        </Row>
      {/each}
    {/if}
  </Group>
{/if}

<Sheet open={connectOpen} title={t("agents.connectSheet.title")} onclose={() => (connectOpen = false)}>
  <p class="text">{t("agents.connectSheet.intro")}</p>
  <pre class="code">{snippets?.claude_command ?? "claude plugin marketplace add ~/.elephant-stack/share/claude-plugin\nclaude plugin install elephant@elephant-stack -s user"}</pre>
  <p class="text">{t("agents.connectSheet.note")}</p>
  <div class="actions">
    <Button onclick={() => (connectOpen = false)} disabled={connecting}>{t("common.cancel")}</Button>
    <Button variant="primary" busy={connecting} onclick={() => void connect()}>{t("agents.connectSheet.confirm")}</Button>
  </div>
</Sheet>

<Sheet open={disableOpen} title={t("agents.disableSheet.title")} onclose={() => (disableOpen = false)}>
  <p class="text">
    {t("agents.disableSheet.text")}
  </p>
  <div class="actions">
    <Button onclick={() => (disableOpen = false)} disabled={linking}>{t("common.cancel")}</Button>
    <Button variant="destructive" busy={linking} onclick={() => void runLink(false)}>{t("agents.disableSheet.confirm")}</Button>
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
