mod commands;
mod error;
mod models;
mod providers;

use providers::ProviderRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let registry = ProviderRegistry::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(registry)
        .invoke_handler(tauri::generate_handler![
            commands::list_all_sessions,
            commands::list_sessions,
            commands::get_session,
            commands::get_subagent_messages,
            commands::search_sessions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
