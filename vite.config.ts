import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [svelte()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  // El build parte solo de index.html; la galería (src/dev/gallery.html)
  // únicamente se sirve con `pnpm dev`.
  //
  // Chunks (presupuesto de 01 §11): todo lo que carga la ventana al abrirse va en
  // un solo chunk inicial, y lo que solo comparten las vistas diferidas (Logs,
  // asistente, Sitios y pestañas de Ajustes) en otro aparte. Sin esto, Rolldown
  // mete en el chunk común piezas de Svelte que solo usan las vistas diferidas.
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            { name: "app", tags: ["$initial" as const], priority: 2 },
            { name: "shared", priority: 1, minShareCount: 2 },
          ],
        },
      },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    // Español por defecto en todos los tests (docs/i18n.md).
    setupFiles: ["src/lib/i18n/test-setup.ts"],
  },
}));
