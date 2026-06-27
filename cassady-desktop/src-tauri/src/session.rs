use crate::state::DesktopState;
use crate::types::{
    ApplyProviderLoginArgs, AutoFillMenuDto, BranchResultDto, ChatSummaryDto, CommandOutcomeDto,
    CommandSpecDto, ConversationInfoDto, CreateBranchArgs, DiscoverModelsArgs, ListChatsArgs,
    LogoutResultDto, ModelOptionDto, NewSessionArgs, ProviderApplyResultDto,
    ProviderLogoutCandidateDto, RemoveProvidersArgs, RestoreReportDto, ResumeSessionArgs,
    RunSlashCommandArgs, SessionIdArgs, SlashAutofillArgs, UpdateSessionSettingsArgs,
};
use cassady::commands::{self, CommandContext, CommandOutcome};
use cassady::config::{cass_root, load_or_create_default_model_registry, save_last_used_provider};
use cassady::conversation::list_chats as list_chats_fn;
use cassady::embedding::SessionBuilder;
use cassady::setup::SetupSelection;
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

/// Reload the session's resolved config from disk, used after `/login` or
/// `/logout` change the active provider/model outside the session. Returns
/// the updated conversation info so the host can refresh its display.
#[tauri::command]
pub fn reload_session_config(
    state: State<'_, DesktopState>,
    args: SessionIdArgs,
) -> Result<ConversationInfoDto, String> {
    let info = {
        let mut sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        let session = sessions
            .get_mut(&args.chat_id)
            .ok_or_else(|| format!("session {} not found", args.chat_id))?;
        session.reload_config().map_err(|e| e.to_string())?;
        session.info()
    };
    Ok(info.into())
}

// --- Slash commands ---------------------------------------------------------

#[tauri::command]
pub fn list_slash_commands() -> Result<Vec<CommandSpecDto>, String> {
    Ok(commands::COMMANDS
        .iter()
        .map(|spec| (*spec).into())
        .collect())
}

#[tauri::command]
pub fn list_provider_catalog() -> Result<Vec<crate::types::ProviderCatalogEntryDto>, String> {
    Ok(commands::login_catalog()
        .into_iter()
        .map(crate::types::ProviderCatalogEntryDto::from)
        .collect())
}

#[tauri::command]
pub fn slash_autofill(args: SlashAutofillArgs) -> Result<Option<AutoFillMenuDto>, String> {
    let root = cass_root();
    let config = commands::reload_config(&root).map_err(|e| e.to_string())?;
    let cwd = match args.cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let cwd = cwd.canonicalize().map_err(|e| e.to_string())?;
    let menu =
        commands::build_autofill(&args.input, 0, &config, &cwd).map_err(|e| e.to_string())?;
    Ok(menu.map(AutoFillMenuDto::from))
}

#[tauri::command]
pub fn run_slash_command(
    state: State<'_, DesktopState>,
    args: RunSlashCommandArgs,
) -> Result<CommandOutcomeDto, String> {
    let busy = {
        let turns = state.turns.lock().map_err(|e| format!("turns lock: {e}"))?;
        turns.values().any(|entry| entry.chat_id == args.chat_id)
    };

    // Clone the session state out so `execute` can mutate a owned config copy
    // without holding simultaneous &mut config / &conversation borrows of the
    // session. Mutations are written back after the call.
    let (mut config, conversation, mut reasoning_effort, cwd, access_mode) = {
        let sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        let session = sessions
            .get(&args.chat_id)
            .ok_or_else(|| format!("session {} not found", args.chat_id))?;
        (
            session.config().clone(),
            session.conversation().clone(),
            session.reasoning_effort(),
            session.cwd().to_path_buf(),
            session.access_mode(),
        )
    };

    let mut ctx = CommandContext {
        config: &mut config,
        reasoning_effort: &mut reasoning_effort,
        conversation: &conversation,
        cwd: &cwd,
        access_mode,
        busy,
        status: "",
    };
    let outcome = commands::execute(&args.input, &mut ctx);

    // Write back config + reasoning effort mutations (e.g. /fast, /model).
    {
        let mut sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        let session = sessions
            .get_mut(&args.chat_id)
            .ok_or_else(|| format!("session {} not found", args.chat_id))?;
        *session.config_mut() = config;
        session.set_reasoning_effort(reasoning_effort);
    }

    match outcome {
        CommandOutcome::Status { title, content } => Ok(CommandOutcomeDto::Status {
            title: title.into(),
            content,
        }),
        CommandOutcome::NewChat {
            conversation: new_conv,
            status,
        } => {
            let info = rekey_session(&state, &args.chat_id, new_conv)?;
            Ok(CommandOutcomeDto::NewChat {
                info: info.into(),
                status,
            })
        }
        CommandOutcome::ResumedChat {
            conversation: new_conv,
            warning,
            status,
        } => {
            let info = rekey_session(&state, &args.chat_id, new_conv)?;
            Ok(CommandOutcomeDto::ResumedChat {
                info: info.into(),
                warning,
                status,
            })
        }
        CommandOutcome::OpenBranchPicker { family } => Ok(CommandOutcomeDto::OpenBranchPicker {
            family: family.into(),
        }),
        CommandOutcome::OpenLoginWizard => Ok(CommandOutcomeDto::OpenLoginWizard),
        CommandOutcome::OpenLogoutPicker { candidates } => {
            Ok(CommandOutcomeDto::OpenLogoutPicker {
                candidates: candidates
                    .into_iter()
                    .map(ProviderLogoutCandidateDto::from)
                    .collect(),
            })
        }
        CommandOutcome::Busy(msg) => Ok(CommandOutcomeDto::Busy { message: msg }),
        CommandOutcome::ParseError(msg) => Ok(CommandOutcomeDto::ParseError { message: msg }),
        CommandOutcome::Error { title, message } => Ok(CommandOutcomeDto::Error {
            title: title.into(),
            message,
        }),
    }
}

/// Replace a session's conversation and re-key it in the sessions map under
/// the new conversation id. Used by `/new` and `/resume`.
fn rekey_session(
    state: &State<'_, DesktopState>,
    old_chat_id: &str,
    new_conversation: cassady::conversation::Conversation,
) -> Result<cassady::embedding::ConversationInfo, String> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| format!("sessions lock: {e}"))?;
    let mut session = sessions
        .remove(old_chat_id)
        .ok_or_else(|| format!("session {} not found", old_chat_id))?;
    session.set_conversation(new_conversation);
    let info = session.info();
    sessions.insert(info.id.clone(), session);
    Ok(info)
}

#[tauri::command]
pub fn create_branch_from_checkpoint(
    state: State<'_, DesktopState>,
    args: CreateBranchArgs,
) -> Result<BranchResultDto, String> {
    let checkpoint: cassady::branch::Checkpoint = args.checkpoint.into();
    let (info, source_chat_id, status, restore) = {
        let mut sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        let mut session = sessions
            .remove(&args.chat_id)
            .ok_or_else(|| format!("session {} not found", args.chat_id))?;
        let config = session.config().clone();
        let outcome =
            commands::create_branch_from_checkpoint(&config, &checkpoint, args.restore_files)
                .map_err(|e| e.to_string())?;
        let source_chat_id = outcome.source_chat_id;
        let status = outcome.status;
        let restore = outcome.restore.map(|r| RestoreReportDto {
            summary: r.summary,
            applied: r.applied,
            skipped: r.skipped,
            conflicts: r.conflicts,
        });
        session.set_conversation(outcome.conversation);
        let info = session.info();
        sessions.insert(info.id.clone(), session);
        (info, source_chat_id, status, restore)
    };
    Ok(BranchResultDto {
        info: info.into(),
        source_chat_id,
        status,
        restore,
    })
}

#[tauri::command]
pub async fn apply_provider_login(
    args: ApplyProviderLoginArgs,
) -> Result<ProviderApplyResultDto, String> {
    let root = cass_root();
    let selections: Vec<SetupSelection> = args
        .selections
        .into_iter()
        .map(SetupSelection::from)
        .collect();
    commands::apply_login(&root, &selections, args.active_index).map_err(|e| e.to_string())?;
    let config = commands::reload_config(&root).map_err(|e| e.to_string())?;
    Ok(ProviderApplyResultDto {
        active_provider: config.provider_id,
        active_model: config.model,
    })
}

#[tauri::command]
pub async fn discover_models(args: DiscoverModelsArgs) -> Result<Vec<String>, String> {
    commands::discover_models(&args.base_url, &args.api_key)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_providers(args: RemoveProvidersArgs) -> Result<LogoutResultDto, String> {
    let root = cass_root();
    let result =
        commands::remove_providers(&root, &args.provider_ids).map_err(|e| e.to_string())?;
    Ok(result.into())
}
