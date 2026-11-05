mod brew;
mod components;
mod monitor;
mod paths;
mod php;
mod settings;
mod tools;

use std::sync::Mutex;

use brew::{Operation, Service};
use components::Component;
use monitor::Usage;
use php::PhpStatus;
use settings::Settings;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindowBuilder, Window, WindowEvent};

const MAIN_WINDOW: &str = "main";

struct AppSettings(Mutex<Settings>);

fn keep_in_menu_bar(app: &AppHandle) -> bool {
    app.state::<AppSettings>()
        .0
        .lock()
        .map(|settings| settings.keep_in_menu_bar)
        .unwrap_or(true)
}

/// Con la ventana abierta la app sale en el Dock y en Cmd+Tab; sin ella, solo
/// en la barra de menús.
fn show_in_dock(app: &AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if visible {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}

/// La ventana se destruye al cerrarla para liberar WebKit, así que puede que
/// haya que crearla de nuevo.
fn open_main_window(app: &AppHandle) {
    show_in_dock(app, true);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let Some(config) = app.config().app.windows.first() else {
        return;
    };
    match WebviewWindowBuilder::from_config(app, config).and_then(|builder| builder.build()) {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(error) => eprintln!("No se pudo abrir la ventana: {error}"),
    }
}

fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    match event {
        // Minimizar deja la ventana en el Dock, que es justo lo que se quiere evitar.
        // macOS no siempre avisa con Resized al minimizar, pero sí quita el foco;
        // se comprueba al terminar la animación.
        WindowEvent::Resized(_) | WindowEvent::Focused(false) if keep_in_menu_bar(app) => {
            let window = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(600));
                if window.is_minimized().unwrap_or(false) {
                    let _ = window.close();
                }
            });
        }
        WindowEvent::Destroyed if keep_in_menu_bar(app) => show_in_dock(app, false),
        _ => {}
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir Elephant Stack", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Salir de Elephant Stack", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("Elephant Stack")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => open_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Respaldo del evento de foco: la interfaz avisa cuando deja de verse.
#[tauri::command]
fn close_if_minimized(window: Window) {
    if keep_in_menu_bar(window.app_handle()) && window.is_minimized().unwrap_or(false) {
        let _ = window.close();
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppSettings>) -> Result<Settings, String> {
    state.0.lock().map(|settings| *settings).map_err(|error| error.to_string())
}

#[tauri::command]
fn set_settings(settings: Settings, state: State<'_, AppSettings>) -> Result<Settings, String> {
    settings::save(&settings)?;
    *state.0.lock().map_err(|error| error.to_string())? = settings;
    Ok(settings)
}

/// Ejecuta trabajo bloqueante fuera del hilo principal para no congelar la UI.
async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn list_components() -> Result<Vec<Component>, String> {
    blocking(components::list).await
}

#[tauri::command]
async fn list_services() -> Result<Vec<Service>, String> {
    blocking(brew::list_services).await
}

#[tauri::command]
async fn service_operation(name: String, operation: Operation) -> Result<Vec<Service>, String> {
    blocking(move || brew::apply(&name, operation)).await
}

#[tauri::command]
async fn install_component(id: String) -> Result<Vec<Component>, String> {
    blocking(move || components::install(&id)).await
}

#[tauri::command]
async fn process_usage(pids: Vec<i64>) -> Result<Vec<Usage>, String> {
    blocking(move || monitor::usage_for(&pids)).await
}

#[tauri::command]
async fn php_status() -> Result<PhpStatus, String> {
    blocking(php::status).await
}

#[tauri::command]
async fn set_global_php(version: String) -> Result<PhpStatus, String> {
    blocking(move || php::set_global(&version)).await
}

#[tauri::command]
async fn set_project_php(dir: String, version: String) -> Result<(), String> {
    blocking(move || php::set_project(&dir, &version)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppSettings(Mutex::new(settings::load())))
        .setup(|app| {
            build_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(on_window_event)
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            close_if_minimized,
            php_status,
            set_global_php,
            set_project_php,
            list_components,
            list_services,
            service_operation,
            install_component,
            process_usage
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| match event {
            // Cerrar la última ventana pide salir sin código; «Salir» y Cmd+Q
            // llevan código y siempre cierran la app.
            RunEvent::ExitRequested { api, code: None, .. } if keep_in_menu_bar(app) => {
                api.prevent_exit();
            }
            // Clic en el icono del Dock o volver a abrir la app desde Finder.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => open_main_window(app),
            _ => {}
        });
}

#[cfg(test)]
mod system_tests {
    /// Consulta el sistema real sin modificarlo: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore = "depende de las herramientas instaladas"]
    fn reads_real_components() {
        let components = crate::components::list().expect("debe listar los componentes");
        for component in &components {
            println!(
                "{:?} {} installed={} version={:?} origin={:?} service={}",
                component.kind,
                component.id,
                component.installed,
                component.version,
                component.origin,
                component.service.is_some()
            );
        }

        let pids: Vec<i64> = components
            .iter()
            .filter_map(|component| component.service.as_ref()?.pid)
            .collect();
        let usage = crate::monitor::usage_for(&pids).expect("ps debe responder");
        println!("{usage:?}");
        assert_eq!(usage.len(), pids.len());
    }
}
