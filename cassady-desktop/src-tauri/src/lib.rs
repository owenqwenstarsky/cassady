mod session;
mod state;
mod turn;
mod types;

use state::DesktopState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::new())
        .invoke_handler(tauri::generate_handler![
            session::new_session,
            session::resume_session,
            session::get_cwd,
            session::list_chats_cmd,
            session::session_info,
            session::session_records,
            session::list_models_cmd,
            session::update_session_settings,
            session::reload_session_config,
            session::list_slash_commands,
            session::slash_autofill,
            session::run_slash_command,
            session::create_branch_from_checkpoint,
            session::apply_provider_login,
            session::discover_models,
            session::remove_providers,
            session::list_provider_catalog,
            turn::start_turn,
            turn::approve,
            turn::deny,
            turn::cancel_turn,
        ])
        .run(tauri::generate_context!())
        .expect("error while running cassady desktop");
}
