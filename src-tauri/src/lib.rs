mod commands;
mod config;
mod error;
mod export;
mod models;
mod providers;

use std::sync::{Arc, RwLock};
use tauri::Manager;

use providers::ProviderRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 启动时读配置 + 初始化 registry（先读，再 manage）
            // 用 Arc<RwLock<...>> 包装，让后续运行时也能重新加载（Task 5）
            let cfg = config::load(app.handle());
            let registry = ProviderRegistry::new(&cfg.provider_paths);
            app.manage(Arc::new(RwLock::new(registry)));
            Ok(())
        })
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
            // commands::get_provider_config,         // Task 5
            // commands::update_provider_config,      // Task 5
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
