// WebKit admite `<input type="checkbox" switch>` (un interruptor nativo de macOS),
// pero los tipos de Svelte no conocen el atributo y svelte-check avisaría.
import "svelte/elements";

declare module "svelte/elements" {
  export interface HTMLInputAttributes {
    switch?: boolean;
  }
}

export {};
