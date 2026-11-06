// Copiar texto al portapapeles desde la vista. WKWebView permite
// `navigator.clipboard` tras un gesto del usuario; si falla, se usa el camino
// clásico con un `<textarea>` y `execCommand`.

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Sigue con el respaldo.
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  if (!ok) throw new Error("No se ha podido copiar al portapapeles.");
}
