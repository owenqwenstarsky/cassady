use crate::state::DesktopState;
use crate::types::{
    ChatSummaryDto, ConversationInfoDto, ListChatsArgs, ModelOptionDto, NewSessionArgs,
    ResumeSessionArgs, SessionIdArgs, UpdateSessionSettingsArgs,
};
use cassady::config::{cass_root, load_or_create_default_model_registry, save_last_used_provider};
use cassady::conversation::list_chats as list_chats_fn;
use cassady::embedding::SessionBuilder;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn new_session(
    state: State<'_, DesktopState>,
    args: NewSessionArgs,
) -> Result<ConversationInfoDto, String> {
    let mut builder = SessionBuilder::new();
    if let Some(cwd) = args.cwd {
        builder = builder.cwd(cwd);
    }
    if let Some(mode) = args.access_mode {
        builder = builder.access_mode(mode);
    }
    if let Some(model) = args.model {
        builder = builder.model(model);
    }
    if let Some(base_url) = args.base_url {
        builder = builder.base_url(base_url);
    }
    if let Some(api_key_env) = args.api_key_env {
        builder = builder.api_key_env(api_key_env);
    }
    if let Some(effort) = args.reasoning_effort {
        builder = builder.reasoning_effort(effort);
    }

    let session = builder.new_session().await.map_err(|e| e.to_string())?;
    let info: ConversationInfoDto = session.info().into();
    state
        .sessions
        .lock()
        .map_err(|e| format!("sessions lock: {e}"))?
        .insert(info.id.clone(), session);
    Ok(info)
}

#[tauri::command]
pub async fn resume_session(
    state: State<'_, DesktopState>,
    args: ResumeSessionArgs,
) -> Result<ConversationInfoDto, String> {
    let mut builder = SessionBuilder::new();
    if let Some(cwd) = args.cwd {
        builder = builder.cwd(cwd);
    }

    let session = builder
        .resume(args.chat_id)
        .await
        .map_err(|e| e.to_string())?;
    let info: ConversationInfoDto = session.info().into();
    state
        .sessions
        .lock()
        .map_err(|e| format!("sessions lock: {e}"))?
        .insert(info.id.clone(), session);
    Ok(info)
}

#[tauri::command]
pub fn list_chats_cmd(
    _state: State<'_, DesktopState>,
    args: ListChatsArgs,
) -> Result<Vec<ChatSummaryDto>, String> {
    let root = cass_root();
    let conversations_dir = root.join("conversations");
    let cwd = match args.cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let cwd = cwd.canonicalize().map_err(|e| e.to_string())?;
    let summaries = list_chats_fn(&conversations_dir, &cwd).map_err(|e| e.to_string())?;
    Ok(summaries.into_iter().map(ChatSummaryDto::from).collect())
}

#[tauri::command]
pub fn session_info(
    state: State<'_, DesktopState>,
    args: SessionIdArgs,
) -> Result<ConversationInfoDto, String> {
    let info = {
        let sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        let session = sessions
            .get(&args.chat_id)
            .ok_or_else(|| format!("session {} not found", args.chat_id))?;
        session.info()
    };
    Ok(info.into())
}

#[tauri::command]
pub fn get_cwd() -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    Ok(cwd.display().to_string())
}

#[tauri::command]
pub fn session_records(
    state: State<'_, DesktopState>,
    args: SessionIdArgs,
) -> Result<serde_json::Value, String> {
    let records = {
        let sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        let session = sessions
            .get(&args.chat_id)
            .ok_or_else(|| format!("session {} not found", args.chat_id))?;
        session.records().to_vec()
    };
    serde_json::to_value(records).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_models_cmd() -> Result<Vec<ModelOptionDto>, String> {
    let root = cass_root();
    let models = load_or_create_default_model_registry(&root).map_err(|e| e.to_string())?;
    Ok(models
        .models
        .into_iter()
        .map(ModelOptionDto::from)
        .collect())
}

#[tauri::command]
pub fn update_session_settings(
    state: State<'_, DesktopState>,
    args: UpdateSessionSettingsArgs,
) -> Result<ConversationInfoDto, String> {
    {
        let turns = state.turns.lock().map_err(|e| format!("turns lock: {e}"))?;
        if turns.values().any(|entry| entry.chat_id == args.chat_id) {
            return Err("settings can be changed when idle".to_string());
        }
    }

    let info = {
        let mut sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        let session = sessions
            .get_mut(&args.chat_id)
            .ok_or_else(|| format!("session {} not found", args.chat_id))?;

        if let Some(mode) = args.access_mode {
            session.set_access_mode(mode);
        }
        if let Some(model) = args.model {
            session.set_model(model).map_err(|e| e.to_string())?;
        }
        if let Some(effort) = args.reasoning_effort {
            session.set_reasoning_effort(effort);
        }

        let info = session.info();
        let _ = save_last_used_provider(
            &cass_root(),
            session.provider_id(),
            session.model(),
            session.reasoning_effort(),
        );
        info
    };

    Ok(info.into())
}
