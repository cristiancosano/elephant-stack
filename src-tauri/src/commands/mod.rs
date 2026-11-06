//! Comandos de Tauri (00 §5.6). Todos son `async`, devuelven
//! `Result<_, String>` y hacen el trabajo bloqueante en `spawn_blocking`.

pub mod core;
pub mod logs;
pub mod mcp;
pub mod setup;

/// Ejecuta trabajo bloqueante fuera del hilo principal para no congelar la UI.
pub(crate) async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}
