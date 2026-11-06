// Galería del sistema de diseño. Solo en `pnpm dev`:
// http://localhost:1420/src/dev/gallery.html (no entra en el build).
import { mount } from "svelte";
import "../ui/tokens.css";
import "../ui/base.css";
import Gallery from "./Gallery.svelte";

const target = document.getElementById("gallery");
if (target) mount(Gallery, { target });
