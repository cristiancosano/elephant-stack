//! App de Elephant Stack: ventana única, menú de la barra y comandos sobre
//! `elephant-core` y `elephant-logs`.

mod actions;
mod app_menu;
mod appearance;
mod commands;
mod sidecar;
mod state;
mod tray;
mod watch;
mod window;

use elephant_core::settings;
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;

use commands::logs::LogsState;
use commands::mcp::McpState;
use commands::setup::SetupState;
use state::AppState;

/// Argumento con el que arranca al iniciar sesión: sin ventana, solo en la
/// barra de menús (C24).
const HIDDEN: &str = "--hidden";

fn keep_in_menu_bar(app: &AppHandle) -> bool {
    app.state::<AppState>().settings().keep_in_menu_bar
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let hidden = std::env::args().any(|arg| arg == HIDDEN);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![HIDDEN])))
        .manage(AppState::new(settings::load()))
        .manage(LogsState::default())
        .manage(SetupState::default())
        .manage(McpState)
        .menu(app_menu::build)
        // Todos los manejadores globales reciben todos los eventos de menú,
        // también los de los menús contextuales del frontend (C25): los ids
        // desconocidos se ignoran sin registrarlos.
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if !app_menu::handle(app, id) {
                tray::handle(app, id);
            }
        })
        .setup(move |app| {
            let handle = app.handle();
            tray::build(handle)?;
            watch::start(handle);
            commands::mcp::repair_link_on_startup();
            state::refresh_in_background(handle);
            if hidden {
                window::show_in_dock(handle, false);
            } else {
                window::open_main_window(handle, None);
            }
            Ok(())
        })
        .on_window_event(window::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::core::get_snapshot,
            commands::core::refresh,
            commands::core::service_operation,
            commands::core::services_bulk,
            commands::core::install_component,
            commands::core::set_global_php,
            commands::core::set_project_php,
            commands::core::php_replace,
            commands::core::process_usage,
            commands::core::get_settings,
            commands::core::set_settings,
            commands::core::take_pending_route,
            commands::core::appearance,
            commands::core::close_if_minimized,
            commands::core::open_in_editor,
            commands::core::open_url,
            commands::core::reveal_in_finder,
            commands::core::open_terminal,
            commands::core::site_action,
            commands::core::open_service_log,
            commands::core::project_start_services,
            commands::core::clear_problems,
            commands::logs::logs_projects,
            commands::logs::logs_query,
            commands::logs::logs_entry,
            commands::logs::logs_groups,
            commands::logs::logs_subscribe,
            commands::logs::logs_unsubscribe,
            commands::logs::logs_roots,
            commands::logs::logs_add_root,
            commands::logs::logs_remove_root,
            commands::logs::logs_set_override,
            commands::logs::logs_clear,
            commands::logs::logs_mark_seen,
            commands::setup::setup_status,
            commands::setup::setup_detect,
            commands::setup::setup_scenarios,
            commands::setup::setup_profile,
            commands::setup::setup_preview,
            commands::setup::setup_apply,
            commands::setup::setup_cancel,
            commands::setup::setup_skip,
            commands::setup::setup_shell_preview,
            commands::setup::setup_shell_apply,
            commands::setup::php_catalog,
            commands::setup::php_source_check,
            commands::setup::php_download,
            commands::mcp::mcp_status,
            commands::mcp::mcp_enable,
            commands::mcp::mcp_disable,
            commands::mcp::mcp_connect_claude,
            commands::mcp::mcp_test,
            commands::mcp::mcp_snippets,
            commands::mcp::mcp_activity,
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
            RunEvent::Reopen { .. } => window::open_main_window(app, None),
            _ => {}
        });
}

#[cfg(test)]
mod system_tests {
    /// Consulta el sistema real sin modificarlo: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore = "depende de las herramientas instaladas"]
    fn reads_real_components() {
        let components = elephant_core::components::list().expect("debe listar los componentes");
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
        let usage = elephant_core::monitor::usage_for(&pids).expect("ps debe responder");
        println!("{usage:?}");
        assert_eq!(usage.len(), pids.len());
    }
}

#[cfg(test)]
mod contract_tests {
    /// 00 §5.6 es la lista completa de comandos: cada uno que registra
    /// `generate_handler!` tiene que salir en su tabla.
    #[test]
    fn every_registered_command_is_in_the_plan() {
        let lib = include_str!("lib.rs");
        let plan = include_str!("../../docs/decisions/00-plan.md");
        let start = plan.find("### 5.6 Comandos de Tauri").expect("falta 00 §5.6");
        let table = &plan[start..];
        let table = &table[..table[4..].find("\n### ").map_or(table.len(), |end| end + 4)];
        let handlers = &lib[lib.find("generate_handler![").expect("sin generate_handler")..];
        let handlers = &handlers[..handlers.find("])").expect("lista sin cerrar")];
        let missing: Vec<&str> = handlers
            .lines()
            .filter_map(|line| line.trim().trim_end_matches(',').rsplit("::").next())
            .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .filter(|name| !table.contains(&format!("`{name}`")))
            .collect();
        assert!(missing.is_empty(), "faltan en 00 §5.6: {missing:?}");
    }
}
