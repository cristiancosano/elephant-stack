// Menús contextuales nativos de la vista Logs (01 §6.6: siempre nativos). Se usa
// `@tauri-apps/api/menu` con import dinámico, como `Select`; fuera de Tauri (galería,
// tests) no hace nada.

export type MenuEntry = { text: string; action: () => void; enabled?: boolean } | "separator";

const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// El menú anterior se suelta al abrir otro: sus acciones pueden llegar después de
// que `popup` resuelva.
let lastMenu: { close(): Promise<void> } | null = null;

export function releaseMenu(): void {
  const menu = lastMenu;
  lastMenu = null;
  void menu?.close().catch(() => {});
}

/** Abre el menú en `at` (coordenadas de la ventana) o donde esté el puntero. */
export async function popupMenu(entries: readonly MenuEntry[], at?: { x: number; y: number }): Promise<void> {
  if (!inTauri() || entries.length === 0) return;
  const [{ Menu }, { LogicalPosition }] = await Promise.all([import("@tauri-apps/api/menu"), import("@tauri-apps/api/dpi")]);
  releaseMenu();
  const menu = await Menu.new({
    items: entries.map((e) =>
      e === "separator" ? { item: "Separator" as const } : { text: e.text, enabled: e.enabled ?? true, action: e.action },
    ),
  });
  lastMenu = menu;
  await menu.popup(at ? new LogicalPosition(at.x, at.y) : undefined);
}
