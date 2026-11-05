import { invoke } from "@tauri-apps/api/core";
import {
  CheckMenuItem,
  Menu,
  MenuItem,
  PredefinedMenuItem,
} from "@tauri-apps/api/menu";
import { open } from "@tauri-apps/plugin-dialog";

type Operation =
  | "turn_on"
  | "turn_off"
  | "restart"
  | "enable_autostart"
  | "disable_autostart";

interface Service {
  name: string;
  status: string;
  running: boolean;
  registered: boolean;
  pid: number | null;
  exit_code: number | null;
}

interface Component {
  id: string;
  kind: "service" | "tool";
  description: string;
  installed: boolean;
  version: string | null;
  origin: string | null;
  path: string | null;
  service: Service | null;
}

interface Usage {
  pid: number;
  memory_kb: number;
  cpu_percent: number;
  processes: number;
}

interface PhpVersion {
  key: string;
  id: string;
  source: string;
}

interface PhpStatus {
  versions: PhpVersion[];
  active: PhpVersion | null;
}

const USAGE_INTERVAL_MS = 5000;
/** Por debajo de este porcentaje, la CPU no aporta nada y no se muestra. */
const CPU_NOTICE_PERCENT = 5;
/** launchd puede tardar en reflejar un arranque; se vuelve a consultar después. */
const STATUS_RECHECK_MS = 2000;

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const servicesSection = byId("services-section");
const phpSection = byId("php-section");
const phpEl = byId<HTMLUListElement>("php");
const toolsSection = byId("tools-section");
const missingSection = byId("missing-section");
const servicesEl = byId<HTMLUListElement>("services");
const toolsEl = byId<HTMLUListElement>("tools");
const missingEl = byId<HTMLUListElement>("missing");
const missingTitle = byId("missing-title");
const summaryEl = byId("summary");
const errorEl = byId("error");
const refreshEl = byId<HTMLButtonElement>("refresh");

let components: Component[] = [];
let phpStatus: PhpStatus | null = null;
/** Un componente con una operación en curso. Homebrew admite solo una a la vez. */
let busyId: string | null = null;
const usageByPid = new Map<number, Usage>();

function showError(error: unknown) {
  errorEl.className = "error";
  errorEl.textContent = String(error);
  errorEl.hidden = false;
}

function showNotice(message: string) {
  errorEl.className = "notice";
  errorEl.textContent = message;
  errorEl.hidden = false;
}

function clearError() {
  errorEl.textContent = "";
  errorEl.hidden = true;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function formatMemory(kb: number): string {
  const mb = kb / 1024;
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

function serviceText(service: Service): string {
  const parts: string[] = [];
  if (service.running) {
    parts.push("En marcha");
    const usage = service.pid ? usageByPid.get(service.pid) : undefined;
    if (usage) {
      parts.push(formatMemory(usage.memory_kb));
      if (usage.cpu_percent >= CPU_NOTICE_PERCENT) {
        parts.push(`${Math.round(usage.cpu_percent)} % CPU`);
      }
    }
  } else if (service.status === "error") {
    parts.push(`Error · código ${service.exit_code ?? "desconocido"}`);
  } else {
    parts.push("Parado");
  }
  if (service.registered) parts.push("Al iniciar sesión");
  return parts.join(" · ");
}

/** Homebrew es el origen por defecto, así que solo se nombra cualquier otro. */
function installedText(component: Component): string {
  const origin = component.origin === "Homebrew" ? null : component.origin;
  return [component.version, origin].filter(Boolean).join(" · ") || "Instalado";
}

function glyph(service: Service): HTMLElement {
  const state = service.running ? "on" : service.status === "error" ? "bad" : "off";
  return el("span", `glyph ${state}`);
}

function secondary(component: Component): HTMLElement {
  const node = el("span", "secondary");
  node.dataset.id = component.id;
  node.textContent = secondaryText(component);
  if (component.path) node.title = component.path;
  return node;
}

function secondaryText(component: Component): string {
  if (busyId === component.id) return "Aplicando…";
  return component.service ? serviceText(component.service) : installedText(component);
}

function serviceRow(component: Component, service: Service): HTMLLIElement {
  const row = el("li", "row");
  row.append(glyph(service), el("span", "name", component.id), secondary(component));

  const toggle = el("input");
  toggle.type = "checkbox";
  toggle.setAttribute("switch", "");
  toggle.checked = service.running;
  toggle.disabled = busyId !== null;
  toggle.setAttribute(
    "aria-label",
    `${service.running ? "Detener" : "Arrancar"} ${component.id}`,
  );
  toggle.addEventListener("change", () =>
    operate(component.id, service.running ? "turn_off" : "turn_on"),
  );

  const more = el("button", "icon", "⋯");
  more.type = "button";
  more.disabled = busyId !== null;
  more.setAttribute("aria-label", `Más opciones de ${component.id}`);
  more.addEventListener("click", () => openMenu(component, service));
  row.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    if (busyId === null) openMenu(component, service);
  });

  row.append(toggle, more);
  return row;
}

function toolRow(component: Component): HTMLLIElement {
  const row = el("li", "row");
  row.append(el("span", "name", component.id), secondary(component));
  return row;
}

function phpRow(version: PhpVersion, active: boolean): HTMLLIElement {
  const row = el("li", "row");

  const radio = el("input");
  radio.type = "radio";
  radio.name = "php-global";
  radio.checked = active;
  radio.setAttribute("aria-label", `Usar PHP ${version.id} por defecto`);
  radio.addEventListener("change", () => setGlobalPhp(version.key));

  const source = version.source === "Elephant Stack" ? "Compilada por ti" : version.source;
  row.append(
    radio,
    el("span", "name", version.id),
    el("span", "secondary", active ? `${source} · Por defecto` : source),
  );

  const button = el("button", undefined, "Usar en proyecto…");
  button.type = "button";
  button.setAttribute("aria-label", `Usar PHP ${version.id} en un proyecto`);
  button.addEventListener("click", () => setProjectPhp(version));
  row.append(button);
  return row;
}

async function setGlobalPhp(key: string) {
  clearError();
  try {
    phpStatus = await invoke<PhpStatus>("set_global_php", { version: key });
  } catch (error) {
    showError(error);
  }
  render();
}

async function setProjectPhp(version: PhpVersion) {
  clearError();
  try {
    const dir = await open({
      directory: true,
      title: `Carpeta del proyecto que usará PHP ${version.id}`,
    });
    if (typeof dir !== "string") return;
    await invoke("set_project_php", { dir, version: version.key });
    showNotice(`${dir.split("/").pop()} usará PHP ${version.id} (.php-version).`);
  } catch (error) {
    showError(error);
  }
}

async function reloadPhp() {
  try {
    phpStatus = await invoke<PhpStatus>("php_status");
    render();
  } catch (error) {
    showError(error);
  }
}

function missingRow(component: Component): HTMLLIElement {
  const row = el("li", "row");
  row.append(el("span", "name", component.id));
  row.append(el("span", "secondary", component.description));

  const installing = busyId === component.id;
  const button = el("button", undefined, installing ? "Instalando…" : "Instalar");
  button.type = "button";
  button.disabled = busyId !== null;
  button.setAttribute("aria-label", `Instalar ${component.id}`);
  button.addEventListener("click", () => install(component.id));
  row.append(button);
  return row;
}

function render() {
  const installed = components.filter((component) => component.installed);
  const services = installed
    .filter((component) => component.kind === "service")
    // Primero lo que está en marcha; `sort` es estable y conserva el resto del orden.
    .sort((a, b) => Number(b.service?.running ?? false) - Number(a.service?.running ?? false));
  const phpVersions = phpStatus?.versions ?? [];
  const tools = installed.filter(
    // Con versiones detectadas, PHP tiene su propia sección.
    (component) =>
      component.kind === "tool" && !(component.id === "php" && phpVersions.length > 0),
  );
  const missing = components.filter((component) => !component.installed);

  servicesEl.replaceChildren(
    ...services.map((component) =>
      component.service ? serviceRow(component, component.service) : toolRow(component),
    ),
  );
  phpEl.replaceChildren(
    ...phpVersions.map((version) =>
      phpRow(version, version.key === phpStatus?.active?.key),
    ),
  );
  toolsEl.replaceChildren(...tools.map(toolRow));
  missingEl.replaceChildren(...missing.map(missingRow));
  missingTitle.textContent = `Sin instalar (${missing.length})`;

  servicesSection.hidden = services.length === 0;
  phpSection.hidden = phpVersions.length === 0;
  toolsSection.hidden = tools.length === 0;
  missingSection.hidden = missing.length === 0;
  renderSummary();
}

function renderSummary() {
  const running = components.flatMap((component) =>
    component.service?.running ? [component.service] : [],
  );
  if (running.length === 0) {
    summaryEl.textContent = "Nada en marcha";
    return;
  }
  const memory = running.reduce(
    (total, service) =>
      total + (service.pid ? (usageByPid.get(service.pid)?.memory_kb ?? 0) : 0),
    0,
  );
  summaryEl.textContent =
    memory > 0
      ? `${running.length} en marcha · ${formatMemory(memory)}`
      : `${running.length} en marcha`;
}

function renderUsage() {
  for (const node of document.querySelectorAll<HTMLElement>(".secondary[data-id]")) {
    const component = components.find((candidate) => candidate.id === node.dataset.id);
    if (component) node.textContent = secondaryText(component);
  }
  renderSummary();
}

function applyServices(services: Service[]) {
  for (const component of components) {
    component.service =
      services.find((service) => service.name === component.id) ?? null;
  }
}

async function reloadServices() {
  try {
    applyServices(await invoke<Service[]>("list_services"));
    render();
    await pollUsage();
  } catch (error) {
    showError(error);
  }
}

async function operate(name: string, operation: Operation) {
  busyId = name;
  clearError();
  render();
  try {
    applyServices(
      await invoke<Service[]>("service_operation", { name, operation }),
    );
  } catch (error) {
    showError(error);
  } finally {
    busyId = null;
    render();
    void pollUsage();
    setTimeout(() => {
      if (busyId === null) void reloadServices();
    }, STATUS_RECHECK_MS);
  }
}

async function openMenu(component: Component, service: Service) {
  const usage = service.pid ? usageByPid.get(service.pid) : undefined;
  const items: (MenuItem | CheckMenuItem | PredefinedMenuItem)[] = [
    await MenuItem.new({
      text: "Reiniciar",
      enabled: service.running,
      action: () => operate(component.id, "restart"),
    }),
    await CheckMenuItem.new({
      text: "Arrancar al iniciar sesión",
      checked: service.registered,
      action: () =>
        operate(
          component.id,
          service.registered ? "disable_autostart" : "enable_autostart",
        ),
    }),
  ];

  const details: string[] = [];
  if (component.version) details.push(`Versión ${component.version}`);
  if (usage) {
    details.push(usage.processes === 1 ? "1 proceso" : `${usage.processes} procesos`);
  }
  if (details.length > 0) {
    items.push(await PredefinedMenuItem.new({ item: "Separator" }));
    items.push(await MenuItem.new({ text: details.join(" · "), enabled: false }));
  }

  try {
    const menu = await Menu.new({ items });
    await menu.popup();
  } catch (error) {
    showError(error);
  }
}

async function install(id: string) {
  busyId = id;
  clearError();
  render();
  try {
    components = await invoke<Component[]>("install_component", { id });
  } catch (error) {
    showError(error);
  } finally {
    busyId = null;
    render();
    void pollUsage();
  }
}

/** Mide solo con la ventana visible, para que el monitor no gaste recursos. */
async function pollUsage() {
  if (document.hidden) return;
  const pids = components.flatMap((component) =>
    component.service?.running && component.service.pid ? [component.service.pid] : [],
  );
  if (pids.length === 0) {
    renderSummary();
    return;
  }

  try {
    const usages = await invoke<Usage[]>("process_usage", { pids });
    usageByPid.clear();
    for (const usage of usages) usageByPid.set(usage.pid, usage);
    renderUsage();
    // Un PID desaparecido significa que el servicio se ha parado o reiniciado.
    if (usages.length < pids.length && busyId === null) await reloadServices();
  } catch (error) {
    showError(error);
  }
}

async function refresh() {
  refreshEl.disabled = true;
  clearError();
  try {
    [components, phpStatus] = await Promise.all([
      invoke<Component[]>("list_components"),
      invoke<PhpStatus>("php_status"),
    ]);
    render();
    await pollUsage();
  } catch (error) {
    showError(error);
  } finally {
    refreshEl.disabled = false;
  }
}

interface Settings {
  keep_in_menu_bar: boolean;
}

const mainView = byId("main-view");
const settingsView = byId("settings-view");
const settingsToggle = byId<HTMLButtonElement>("settings-toggle");
const keepInMenuBar = byId<HTMLInputElement>("keep-in-menu-bar");

function showSettings(open: boolean) {
  settingsView.hidden = !open;
  mainView.hidden = open;
  refreshEl.hidden = open;
  settingsToggle.setAttribute("aria-expanded", String(open));
  settingsToggle.title = open ? "Volver" : "Ajustes";
  settingsToggle.setAttribute("aria-label", settingsToggle.title);
}

async function loadSettings() {
  try {
    const settings = await invoke<Settings>("get_settings");
    keepInMenuBar.checked = settings.keep_in_menu_bar;
  } catch (error) {
    showError(error);
  }
}

keepInMenuBar.addEventListener("change", async () => {
  const wanted = keepInMenuBar.checked;
  keepInMenuBar.disabled = true;
  try {
    const saved = await invoke<Settings>("set_settings", {
      settings: { keep_in_menu_bar: wanted },
    });
    keepInMenuBar.checked = saved.keep_in_menu_bar;
  } catch (error) {
    keepInMenuBar.checked = !wanted;
    showError(error);
  } finally {
    keepInMenuBar.disabled = false;
  }
});

settingsToggle.addEventListener("click", () => showSettings(settingsView.hidden === true));
void loadSettings();

refreshEl.addEventListener("click", refresh);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    // Si se ha minimizado, el backend la cierra para que no quede en el Dock.
    void invoke("close_if_minimized").catch(() => {});
  } else {
    void pollUsage();
  }
});
// Las versiones pueden cambiar fuera de la app, por ejemplo al compilar una nueva.
window.addEventListener("focus", () => void reloadPhp());
setInterval(() => void pollUsage(), USAGE_INTERVAL_MS);
refresh();
