// Punto de entrada de la ventana. Los estilos se importan aquí, no se enlazan en
// index.html, para que Vite los empaquete con el resto.
import { mount } from "svelte";
import "./ui/tokens.css";
import "./ui/base.css";
import App from "./App.svelte";
import { startLocale } from "./lib/i18n/index.svelte";

// Primero los textos del idioma inicial (el inglés va en su propio chunk), para
// no pintar la ventana en otro idioma.
const target = document.getElementById("app");
void startLocale().finally(() => {
  if (target) mount(App, { target });
});
