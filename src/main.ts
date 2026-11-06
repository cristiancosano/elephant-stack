// Punto de entrada de la ventana. Los estilos se importan aquí, no se enlazan en
// index.html, para que Vite los empaquete con el resto.
import { mount } from "svelte";
import "./ui/tokens.css";
import "./ui/base.css";
import App from "./App.svelte";

const target = document.getElementById("app");
if (target) mount(App, { target });
