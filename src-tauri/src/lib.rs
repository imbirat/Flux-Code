use tauri::Manager;

mod agent;
mod commands;
mod creds;
mod db;
mod fsops;
mod llm;
mod permissions;
mod providers;
mod terminal;
mod tools;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // chats + messages
            commands::list_chats,
            commands::create_chat,
            commands::rename_chat,
            commands::delete_chat,
            commands::update_chat,
            commands::get_messages,
            commands::send_message,
            commands::cancel_chat,
            // permissions + questions
            commands::respond_permission,
            commands::respond_questions,
            // todos
            commands::get_todos,
            commands::set_todos,
            // settings
            commands::get_settings,
            commands::set_settings,
            commands::get_default_workspace,
            // providers
            commands::provider_status,
            commands::provider_set_key,
            commands::provider_remove_key,
            commands::provider_test,
            commands::provider_list_models,
            commands::provider_get_config,
            commands::provider_set_config,
            commands::custom_provider_save,
            commands::custom_provider_delete,
            commands::model_capabilities,
            // workspace + files
            commands::list_dir_tree,
            commands::read_text_file,
            commands::read_file_base64,
            commands::open_external,
            // processes
            commands::list_processes,
            commands::stop_process,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            db::init(&handle)?;
            // One-time migration of the legacy single "custom" provider row.
            if let Err(e) = db::migrate_legacy_custom(&handle) {
                eprintln!("custom provider migration failed: {e}");
            }
            app.manage(agent::Agents(Default::default()));
            app.manage(agent::Pending(Default::default()));
            app.manage(agent::SessionAllow(Default::default()));
            app.manage(terminal::Procs(Default::default()));
            agent::ensure_default_workspace();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Flux Code");
}
