mod commands;
mod error;
mod export;
mod models;
mod providers;

use providers::ProviderRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let registry = ProviderRegistry::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(registry)
        .invoke_handler(tauri::generate_handler![
            commands::list_all_sessions,
            commands::list_sessions,
            commands::get_session,
            commands::get_subagent_messages,
            commands::search_sessions,
            commands::export_session_jsonl,
            commands::export_session_markdown,
            commands::resume_session,
            commands::resume_session_with_auto_continue,
            commands::open_new_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
