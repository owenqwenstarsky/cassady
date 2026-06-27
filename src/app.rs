use crate::agent::{self, AgentCommand, AgentEvent, AgentSettings};
use crate::cli::{self, Cli, Command};
use crate::config::{self, Config, ReasoningEffort};
use crate::conversation::{self, Conversation, Record};
use crate::ui::events::poll_event;
use crate::ui::render::{self, TranscriptBlock, TranscriptKind};
use crate::ui::terminal;
use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const TURN_CANCELLED_MESSAGE: &str = "Turn cancelled by user.";
const TOOL_CANCELLED_MESSAGE: &str = "Tool execution cancelled by user.";

pub async fn run() -> Result<()> {
    let mut cli = cli::parse();
    if let Some(Command::Update(args)) = cli.command.clone() {
        return crate::update::run(args).await;
    }

    if let Some(Command::Desktop(args)) = cli.command.clone() {
        return crate::desktop::launch(&cli, &args);
    }

    if matches!(cli.command, Some(Command::Check)) {
        let report = crate::check::run(&cli)?;
        print!("{}", report.render());
        if report.has_errors() {
            std::process::exit(1);
        }
        return Ok(());
    }

    if matches!(cli.command, Some(Command::Login)) {
        let _ = crate::setup::run(&cli, crate::setup::SetupMode::Login).await?;
        return Ok(());
    }

    if matches!(cli.command, Some(Command::Logout)) {
        let _ = crate::setup::logout(&config::cass_root())?;
        return Ok(());
    }

    if matches!(cli.command, Some(Command::Setup)) {
        let outcome = crate::setup::run(&cli, crate::setup::SetupMode::Explicit).await?;
        if !outcome.start_session {
            return Ok(());
        }
        cli.command = None;
    }

    if cli.command.is_none()
        && cli.resume.is_none()
        && crate::setup::needs_initial_setup(&config::cass_root())
    {
        let outcome = crate::setup::run(&cli, crate::setup::SetupMode::Auto).await?;
        if !outcome.start_session {
            return Ok(());
        }
    }

    let mut config = match Config::load(&cli) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Cassady is not ready to start: {err:#}\n");
            let outcome = crate::setup::run(&cli, crate::setup::SetupMode::Auto).await?;
            if !outcome.start_session {
                return Ok(());
            }
            Config::load(&cli)?
        }
    };
    let cwd = resolve_cwd(cli.cwd.clone())?;

    if matches!(cli.resume, Some(None)) {
        list_chats(&config, &cwd)?;
        return Ok(());
    }

    if config.ensure_provider_auth().is_err() {
        let outcome = crate::setup::run(&cli, crate::setup::SetupMode::Auto).await?;
        if !outcome.start_session {
            return Ok(());
        }
        config = Config::load(&cli)?;
    }

    let (conversation, warning) = if let Some(Some(id)) = cli.resume.clone() {
        Conversation::load(&config.conversations_dir(), &id)?
    } else {
        (crate::commands::create_new(&config, &cwd)?, None)
    };

    run_tui(config, cwd, conversation, warning, cli).await
}

fn resolve_cwd(cwd: Option<PathBuf>) -> Result<PathBuf> {
    let cwd = cwd.unwrap_or(std::env::current_dir()?);
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("resolving cwd {}", cwd.display()))?;
    std::env::set_current_dir(&cwd)?;
    Ok(cwd)
}

fn list_chats(config: &Config, cwd: &std::path::Path) -> Result<()> {
    let chats = conversation::list_chats(&config.conversations_dir(), cwd)?;
    if chats.is_empty() {
        println!("No chats found for {}", cwd.display());
    } else {
        for c in chats {
            println!(
                "{}  {}  {}  {}",
                c.id, c.created_at, c.model, c.first_user_preview
            );
        }
    }
    Ok(())
}

async fn run_login_menu_from_tui(
    terminal: &mut terminal::CassTerminal,
    cli: &Cli,
) -> Result<crate::setup::SetupOutcome> {
    terminal::suspend(terminal)?;
    let result = crate::setup::run(cli, crate::setup::SetupMode::Login).await;
    let resume_result = terminal::resume(terminal);
    match (result, resume_result) {
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Err(err), Ok(())) => Err(err),
        (Ok(_), Err(err)) => Err(err),
        (Err(err), Err(resume_err)) => Err(err.context(format!(
            "also failed to restore the chat screen: {resume_err}"
        ))),
    }
}

fn run_logout_menu_from_tui(
    terminal: &mut terminal::CassTerminal,
    root: &Path,
) -> Result<crate::setup::LogoutResult> {
    terminal::suspend(terminal)?;
    let result = crate::setup::logout(root);
    let resume_result = terminal::resume(terminal);
    match (result, resume_result) {
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Err(err), Ok(())) => Err(err),
        (Ok(_), Err(err)) => Err(err),
        (Err(err), Err(resume_err)) => Err(err.context(format!(
            "also failed to restore the chat screen: {resume_err}"
        ))),
    }
}

fn finalize_cancelled_turn(
    config: &Config,
    chat_id: &str,
    turn_start_len: Option<usize>,
    turn_message: Option<&str>,
) -> Result<Conversation> {
    let (mut conversation, _) = Conversation::load(&config.conversations_dir(), chat_id)?;

    if let (Some(start_len), Some(message)) = (turn_start_len, turn_message) {
        if conversation.records.len() <= start_len {
            conversation.append(Record::User {
                content: message.to_string(),
                ts: conversation::now_ts(),
            })?;
        }
    }

    for (id, name) in pending_tool_calls(&conversation.records) {
        conversation.append(Record::Tool {
            tool_call_id: id,
            name,
            ok: false,
            content: TOOL_CANCELLED_MESSAGE.to_string(),
            ts: conversation::now_ts(),
        })?;
    }

    if !matches!(
        conversation.records.last(),
        Some(Record::Assistant { content, tool_calls, .. })
            if content == TURN_CANCELLED_MESSAGE && tool_calls.is_empty()
    ) {
        conversation.append(Record::Assistant {
            content: TURN_CANCELLED_MESSAGE.to_string(),
            reasoning: String::new(),
            reasoning_field: None,
            tool_calls: Vec::new(),
            ts: conversation::now_ts(),
        })?;
    }

    Ok(conversation)
}

fn pending_tool_calls(records: &[Record]) -> Vec<(String, String)> {
    let mut pending = Vec::new();
    for record in records {
        match record {
            Record::Assistant { tool_calls, .. } => {
                pending = tool_calls
                    .iter()
                    .map(|call| (call.id.clone(), call.name.clone()))
                    .collect();
            }
            Record::Tool { tool_call_id, .. } => {
                pending.retain(|(id, _)| id != tool_call_id);
            }
            Record::User { .. } => pending.clear(),
            _ => {}
        }
    }
    pending
}

async fn run_tui(
    mut config: Config,
    cwd: PathBuf,
    mut conversation: Conversation,
    warning: Option<String>,
    cli: Cli,
) -> Result<()> {
    let mut terminal = terminal::enter()?;
    let mut transcript = Vec::new();
    if let Some(w) = warning {
        transcript.push(TranscriptBlock {
            kind: TranscriptKind::Error,
            title: "warning".into(),
            content: w,
        });
    }
    transcript.extend(blocks_from_conversation(&conversation));

    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
    let mut agent_command_tx: Option<mpsc::UnboundedSender<AgentCommand>> = None;
    let mut input = String::new();
    let mut mode = config.default_access_mode;
    let mut status = String::new();
    let mut show_full_tools = false;
    let mut show_reasoning = config.show_reasoning;
    let mut reasoning_effort = config.reasoning_effort;
    let mut scroll: u16 = 0;
    let mut last_ctrl_c: Option<Instant> = None;
    let mut last_esc: Option<Instant> = None;
    let mut branch_menu: Option<BranchMenuState> = None;
    let mut handle: Option<JoinHandle<Result<Conversation>>> = None;
    let mut cancel_requested = false;
    let mut current_turn_start_len: Option<usize> = None;
    let mut current_turn_message: Option<String> = None;
    let mut active_assistant: Option<usize> = None;
    let mut active_reasoning: Option<usize> = None;
    let mut active_tools: HashMap<String, usize> = HashMap::new();
    let mut stick_to_bottom = true;
    let mut chat_id = conversation.id.clone();
    let mut autofill_selected = 0usize;
    let mut pending_approval: Option<PendingApproval> = None;
    let mut provider_ready = true;

    loop {
        drain_agent_events(
            &mut rx,
            &mut AgentEventContext {
                terminal: &terminal,
                input: &input,
                transcript: &mut transcript,
                active_assistant: &mut active_assistant,
                active_reasoning: &mut active_reasoning,
                active_tools: &mut active_tools,
                pending_approval: &mut pending_approval,
                status: &mut status,
                stick_to_bottom,
                show_full_tools,
                show_reasoning,
                scroll: &mut scroll,
            },
        )?;

        if handle.as_ref().map(|h| h.is_finished()).unwrap_or(false) {
            let h = handle.take().unwrap();
            let result = h.await;
            drain_agent_events(
                &mut rx,
                &mut AgentEventContext {
                    terminal: &terminal,
                    input: &input,
                    transcript: &mut transcript,
                    active_assistant: &mut active_assistant,
                    active_reasoning: &mut active_reasoning,
                    active_tools: &mut active_tools,
                    pending_approval: &mut pending_approval,
                    status: &mut status,
                    stick_to_bottom,
                    show_full_tools,
                    show_reasoning,
                    scroll: &mut scroll,
                },
            )?;
            let mut finished_status = "idle".to_string();
            match result {
                Ok(Ok(updated)) => {
                    conversation = updated;
                    ensure_final_assistant_visible(&mut transcript, &conversation);
                }
                Ok(Err(err)) => transcript.push(TranscriptBlock {
                    kind: TranscriptKind::Error,
                    title: "agent error".into(),
                    content: err.to_string(),
                }),
                Err(err) if err.is_cancelled() && cancel_requested => {
                    mark_active_tool_blocks_cancelled(&mut transcript, &active_tools);
                    match finalize_cancelled_turn(
                        &config,
                        &chat_id,
                        current_turn_start_len,
                        current_turn_message.as_deref(),
                    ) {
                        Ok(updated) => conversation = updated,
                        Err(err) => transcript.push(TranscriptBlock {
                            kind: TranscriptKind::Error,
                            title: "cancel".into(),
                            content: format!(
                                "turn cancelled, but updating the conversation failed: {err}"
                            ),
                        }),
                    }
                    transcript.push(TranscriptBlock {
                        kind: TranscriptKind::Status,
                        title: "cancelled".into(),
                        content: TURN_CANCELLED_MESSAGE.to_string(),
                    });
                    finished_status = "turn cancelled".into();
                }
                Err(err) => transcript.push(TranscriptBlock {
                    kind: TranscriptKind::Error,
                    title: "agent task error".into(),
                    content: err.to_string(),
                }),
            }
            active_assistant = None;
            active_reasoning = None;
            active_tools.clear();
            cancel_requested = false;
            current_turn_start_len = None;
            current_turn_message = None;
            agent_command_tx = None;
            pending_approval = None;
            status = finished_status;
        }

        let autofill = crate::commands::build_autofill(&input, autofill_selected, &config, &cwd)?;
        autofill_selected = autofill.as_ref().map(|m| m.selected).unwrap_or(0);
        scroll = if stick_to_bottom {
            bottom_scroll(
                &terminal,
                &input,
                &transcript,
                show_full_tools,
                show_reasoning,
            )?
        } else {
            clamp_scroll(
                &terminal,
                &input,
                &transcript,
                show_full_tools,
                show_reasoning,
                scroll,
            )?
        };

        let overlay_view = branch_menu.as_ref().map(BranchMenuState::overlay_view);
        terminal.draw(|f| {
            render::render(
                f,
                &render::RenderState {
                    app_name: "Cassady",
                    chat_id: &chat_id,
                    model: &config.model,
                    mode,
                    cwd: &cwd,
                    transcript: &transcript,
                    input: &input,
                    status: &status,
                    busy: handle.is_some(),
                    show_full_tools,
                    show_reasoning,
                    reasoning_effort,
                    fast_mode_active: config.fast_mode_state().active,
                    scroll,
                    autofill: if branch_menu.is_some() {
                        None
                    } else {
                        autofill.as_ref()
                    },
                    overlay: overlay_view.as_ref(),
                },
            )
        })?;

        if let Some(event) = poll_event(Duration::from_millis(40))? {
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let busy = handle.is_some();
                    if branch_menu.is_some() {
                        match handle_branch_menu_key(
                            key.code,
                            &mut branch_menu,
                            &config,
                            &cwd,
                            &mut conversation,
                            &mut chat_id,
                            &mut transcript,
                            &mut active_assistant,
                            &mut active_reasoning,
                            &mut active_tools,
                            &mut status,
                        ) {
                            Ok(BranchMenuOutcome::None) => {}
                            Ok(BranchMenuOutcome::Changed) => {
                                stick_to_bottom = true;
                                scroll = bottom_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                )?;
                            }
                            Err(err) => {
                                branch_menu = None;
                                status = format!("branch menu failed: {err}");
                                transcript.push(TranscriptBlock {
                                    kind: TranscriptKind::Error,
                                    title: "branch".into(),
                                    content: err.to_string(),
                                });
                            }
                        }
                        continue;
                    }
                    if busy {
                        if let Some(pending) = pending_approval.clone() {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    if let Some(tx) = &agent_command_tx {
                                        let _ = tx.send(AgentCommand::ApprovalDecision {
                                            request_id: pending.request_id,
                                            approved: true,
                                        });
                                    }
                                    hide_pending_approval(
                                        &mut pending_approval,
                                        &mut transcript,
                                        &mut active_assistant,
                                        &mut active_reasoning,
                                        &mut active_tools,
                                    );
                                    status = "approval sent".into();
                                    continue;
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    if let Some(tx) = &agent_command_tx {
                                        let _ = tx.send(AgentCommand::ApprovalDecision {
                                            request_id: pending.request_id,
                                            approved: false,
                                        });
                                    }
                                    hide_pending_approval(
                                        &mut pending_approval,
                                        &mut transcript,
                                        &mut active_assistant,
                                        &mut active_reasoning,
                                        &mut active_tools,
                                    );
                                    status = "approval denied".into();
                                    continue;
                                }
                                _ => {}
                            }
                        }
                    }
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                            let now = Instant::now();
                            if last_ctrl_c
                                .map(|t| now.duration_since(t) <= Duration::from_millis(1500))
                                .unwrap_or(false)
                            {
                                if busy {
                                    if let Some(handle) = &handle {
                                        handle.abort();
                                    }
                                    if cancel_requested {
                                        let _ = finalize_cancelled_turn(
                                            &config,
                                            &chat_id,
                                            current_turn_start_len,
                                            current_turn_message.as_deref(),
                                        );
                                    }
                                }
                                terminal::leave(terminal)?;
                                println!("Resume this chat with: cass --resume {}", chat_id);
                                return Ok(());
                            }
                            if busy {
                                if let Some(handle) = &handle {
                                    handle.abort();
                                }
                                cancel_requested = true;
                                last_ctrl_c = Some(now);
                                last_esc = None;
                                status = "turn cancellation requested; press Ctrl-C again within 1.5s to exit".into();
                            } else {
                                input.clear();
                                autofill_selected = 0;
                                last_ctrl_c = Some(now);
                                last_esc = None;
                                status = "press Ctrl-C again within 1.5s to exit".into();
                            }
                        }
                        (KeyCode::Esc, _) if busy => {
                            if let Some(handle) = &handle {
                                handle.abort();
                            }
                            cancel_requested = true;
                            last_ctrl_c = None;
                            last_esc = None;
                            status = "turn cancellation requested".into();
                        }
                        (KeyCode::Esc, _) => {
                            let now = Instant::now();
                            if last_esc
                                .map(|t| now.duration_since(t) <= Duration::from_millis(1500))
                                .unwrap_or(false)
                            {
                                match crate::commands::branch_family(&config, &conversation) {
                                    Ok(family) => {
                                        branch_menu = Some(BranchMenuState::from_family(family));
                                        last_esc = None;
                                        status = "branch/restore menu".into();
                                    }
                                    Err(err) => {
                                        last_esc = None;
                                        status = format!("branch menu failed: {err}");
                                    }
                                }
                            } else {
                                last_esc = Some(now);
                                status = "press Esc again within 1.5s to branch or restore".into();
                            }
                        }
                        (KeyCode::BackTab, _) => {
                            if busy {
                                status = "mode can be changed when idle".into();
                            } else {
                                mode = mode.next();
                                status = format!("mode: {mode}");
                            }
                        }
                        (KeyCode::Tab, _) => {
                            if busy {
                                status = "reasoning effort can be changed when idle".into();
                            } else {
                                let next =
                                    reasoning_effort.next_for_model(config.model_metadata.as_ref());
                                if next == reasoning_effort
                                    && next == ReasoningEffort::Off
                                    && !config
                                        .model_metadata
                                        .as_ref()
                                        .is_some_and(|model| model.reasoning.supported)
                                {
                                    status = "reasoning unsupported for this model".into();
                                } else {
                                    reasoning_effort = next;
                                    let _ = crate::config::save_last_used(
                                        &config.root,
                                        &config.model,
                                        reasoning_effort,
                                    );
                                    status.clear();
                                }
                            }
                        }
                        (KeyCode::Char('o'), m) if m.contains(KeyModifiers::CONTROL) => {
                            show_full_tools = !show_full_tools;
                            scroll = if stick_to_bottom {
                                bottom_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                )?
                            } else {
                                clamp_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                    scroll,
                                )?
                            };
                            status = if show_full_tools {
                                "showing full tool output"
                            } else {
                                "showing compact tool output"
                            }
                            .into();
                        }
                        (KeyCode::Char('R'), m) if m.contains(KeyModifiers::CONTROL) => {
                            show_reasoning = !show_reasoning;
                            scroll = if stick_to_bottom {
                                bottom_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                )?
                            } else {
                                clamp_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                    scroll,
                                )?
                            };
                            status = if show_reasoning {
                                "showing reasoning"
                            } else {
                                "hiding reasoning"
                            }
                            .into();
                        }
                        (KeyCode::Char('r'), m)
                            if m.contains(KeyModifiers::CONTROL)
                                && m.contains(KeyModifiers::SHIFT) =>
                        {
                            show_reasoning = !show_reasoning;
                            scroll = if stick_to_bottom {
                                bottom_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                )?
                            } else {
                                clamp_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                    scroll,
                                )?
                            };
                            status = if show_reasoning {
                                "showing reasoning"
                            } else {
                                "hiding reasoning"
                            }
                            .into();
                        }
                        (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => {
                            input.push('\n');
                            autofill_selected = 0;
                        }
                        (KeyCode::Enter, m) if m.contains(KeyModifiers::CONTROL) => {
                            input.push('\n');
                            autofill_selected = 0;
                        }
                        (KeyCode::Enter, _) => {
                            if let Some(menu) = &autofill {
                                if let Some(next_input) = menu.apply(&input) {
                                    input = next_input;
                                    autofill_selected = 0;
                                }
                            } else if input.trim().is_empty() {
                                input.clear();
                            } else if input.trim_start().starts_with('/') {
                                let mut ctx = crate::commands::CommandContext {
                                    config: &mut config,
                                    reasoning_effort: &mut reasoning_effort,
                                    conversation: &conversation,
                                    cwd: &cwd,
                                    access_mode: mode,
                                    busy,
                                    status: &status,
                                };
                                let outcome = crate::commands::execute(&input, &mut ctx);
                                match outcome {
                                    crate::commands::CommandOutcome::Status { title, content } => {
                                        input.clear();
                                        autofill_selected = 0;
                                        transcript.push(TranscriptBlock {
                                            kind: TranscriptKind::Status,
                                            title: title.into(),
                                            content: content.clone(),
                                        });
                                        status = content;
                                        if stick_to_bottom {
                                            scroll = bottom_scroll(
                                                &terminal,
                                                &input,
                                                &transcript,
                                                show_full_tools,
                                                show_reasoning,
                                            )?;
                                        }
                                    }
                                    crate::commands::CommandOutcome::NewChat {
                                        conversation: new_conversation,
                                        status: new_status,
                                    } => {
                                        conversation = new_conversation;
                                        chat_id = conversation.id.clone();
                                        transcript.clear();
                                        input.clear();
                                        autofill_selected = 0;
                                        active_assistant = None;
                                        active_reasoning = None;
                                        active_tools.clear();
                                        status = new_status;
                                        stick_to_bottom = true;
                                        scroll = bottom_scroll(
                                            &terminal,
                                            &input,
                                            &transcript,
                                            show_full_tools,
                                            show_reasoning,
                                        )?;
                                    }
                                    crate::commands::CommandOutcome::ResumedChat {
                                        conversation: new_conversation,
                                        warning,
                                        status: new_status,
                                    } => {
                                        conversation = new_conversation;
                                        chat_id = conversation.id.clone();
                                        transcript = transcript_from_loaded(&conversation, warning);
                                        input.clear();
                                        autofill_selected = 0;
                                        active_assistant = None;
                                        active_reasoning = None;
                                        active_tools.clear();
                                        status = new_status;
                                        stick_to_bottom = true;
                                        scroll = bottom_scroll(
                                            &terminal,
                                            &input,
                                            &transcript,
                                            show_full_tools,
                                            show_reasoning,
                                        )?;
                                    }
                                    crate::commands::CommandOutcome::OpenBranchPicker {
                                        family,
                                    } => {
                                        branch_menu = Some(BranchMenuState::from_family(family));
                                        input.clear();
                                        autofill_selected = 0;
                                        status = "branch/restore menu".into();
                                    }
                                    crate::commands::CommandOutcome::OpenLoginWizard => {
                                        input.clear();
                                        autofill_selected = 0;
                                        match run_login_menu_from_tui(&mut terminal, &cli).await {
                                            Ok(_) => match Config::load(&cli) {
                                                Ok(updated) => {
                                                    config = updated;
                                                    reasoning_effort = config.reasoning_effort;
                                                    provider_ready = true;
                                                    let content = format!(
                                                        "active provider: {}\nactive model: {}",
                                                        config.provider_id, config.model
                                                    );
                                                    transcript.push(TranscriptBlock {
                                                        kind: TranscriptKind::Status,
                                                        title: "login".into(),
                                                        content,
                                                    });
                                                    status = "login updated".into();
                                                }
                                                Err(err) => {
                                                    provider_ready = false;
                                                    status =
                                                        format!("login saved with issues: {err}");
                                                    transcript.push(TranscriptBlock {
                                                        kind: TranscriptKind::Error,
                                                        title: "login".into(),
                                                        content: format!(
                                                            "Provider config could not be loaded: {err}"
                                                        ),
                                                    });
                                                }
                                            },
                                            Err(err) => {
                                                status = format!("login cancelled: {err}");
                                                transcript.push(TranscriptBlock {
                                                    kind: TranscriptKind::Error,
                                                    title: "login".into(),
                                                    content: err.to_string(),
                                                });
                                            }
                                        }
                                        if stick_to_bottom {
                                            scroll = bottom_scroll(
                                                &terminal,
                                                &input,
                                                &transcript,
                                                show_full_tools,
                                                show_reasoning,
                                            )?;
                                        }
                                    }
                                    crate::commands::CommandOutcome::OpenLogoutPicker {
                                        ..
                                    } => {
                                        input.clear();
                                        autofill_selected = 0;
                                        match run_logout_menu_from_tui(&mut terminal, &config.root)
                                        {
                                            Ok(result) => {
                                                if result.removed_provider_ids.is_empty() {
                                                    transcript.push(TranscriptBlock {
                                                        kind: TranscriptKind::Status,
                                                        title: "logout".into(),
                                                        content: "logout cancelled".into(),
                                                    });
                                                    status = "logout cancelled".into();
                                                } else if result.remaining_provider_count == 0 {
                                                    provider_ready = false;
                                                    transcript.push(TranscriptBlock {
                                                        kind: TranscriptKind::Status,
                                                        title: "logout".into(),
                                                        content: format!(
                                                            "removed providers: {}\nremoved model entries: {}\nno providers remain; run /login before sending another message",
                                                            result.removed_provider_ids.join(", "),
                                                            result.removed_model_count
                                                        ),
                                                    });
                                                    status = "no provider configured".into();
                                                } else {
                                                    match Config::load(&cli) {
                                                        Ok(updated) => {
                                                            config = updated;
                                                            reasoning_effort =
                                                                config.reasoning_effort;
                                                            provider_ready = true;
                                                            transcript.push(TranscriptBlock {
                                                                kind: TranscriptKind::Status,
                                                                title: "logout".into(),
                                                                content: format!(
                                                                    "removed providers: {}\nremoved model entries: {}\nactive provider: {}\nactive model: {}",
                                                                    result.removed_provider_ids.join(", "),
                                                                    result.removed_model_count,
                                                                    config.provider_id,
                                                                    config.model
                                                                ),
                                                            });
                                                            status = "logout updated".into();
                                                        }
                                                        Err(err) => {
                                                            provider_ready = false;
                                                            status = format!(
                                                                "logout saved with issues: {err}"
                                                            );
                                                            transcript.push(TranscriptBlock {
                                                                kind: TranscriptKind::Error,
                                                                title: "logout".into(),
                                                                content: format!(
                                                                    "Provider config could not be loaded: {err}"
                                                                ),
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                            Err(err) => {
                                                status = format!("logout cancelled: {err}");
                                                transcript.push(TranscriptBlock {
                                                    kind: TranscriptKind::Error,
                                                    title: "logout".into(),
                                                    content: err.to_string(),
                                                });
                                            }
                                        }
                                        if stick_to_bottom {
                                            scroll = bottom_scroll(
                                                &terminal,
                                                &input,
                                                &transcript,
                                                show_full_tools,
                                                show_reasoning,
                                            )?;
                                        }
                                    }
                                    crate::commands::CommandOutcome::Busy(msg) => {
                                        status = msg;
                                    }
                                    crate::commands::CommandOutcome::ParseError(msg) => {
                                        status = msg;
                                    }
                                    crate::commands::CommandOutcome::Error { title, message } => {
                                        status = message.clone();
                                        transcript.push(TranscriptBlock {
                                            kind: TranscriptKind::Error,
                                            title: title.into(),
                                            content: message,
                                        });
                                        if stick_to_bottom {
                                            scroll = bottom_scroll(
                                                &terminal,
                                                &input,
                                                &transcript,
                                                show_full_tools,
                                                show_reasoning,
                                            )?;
                                        }
                                    }
                                }
                            } else if busy {
                                status = "agent is still running".into();
                            } else if !provider_ready {
                                status = "run /login before sending a message".into();
                                transcript.push(TranscriptBlock {
                                    kind: TranscriptKind::Error,
                                    title: "provider".into(),
                                    content:
                                        "No active provider is configured. Run /login before sending another message."
                                            .into(),
                                });
                                if stick_to_bottom {
                                    scroll = bottom_scroll(
                                        &terminal,
                                        &input,
                                        &transcript,
                                        show_full_tools,
                                        show_reasoning,
                                    )?;
                                }
                            } else {
                                let msg = input.trim_end().to_string();
                                current_turn_start_len = Some(conversation.records.len());
                                current_turn_message = Some(msg.clone());
                                cancel_requested = false;
                                last_ctrl_c = None;
                                input.clear();
                                autofill_selected = 0;
                                transcript.push(TranscriptBlock {
                                    kind: TranscriptKind::User,
                                    title: "message".into(),
                                    content: msg.clone(),
                                });
                                active_assistant = None;
                                active_reasoning = None;
                                active_tools.clear();
                                status = "running".into();
                                let settings = AgentSettings {
                                    config: config.clone(),
                                    cwd: cwd.clone(),
                                    mode,
                                    reasoning_effort,
                                };
                                let convo = conversation.clone();
                                let tx2 = tx.clone();
                                let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<AgentCommand>();
                                agent_command_tx = Some(cmd_tx);
                                handle = Some(tokio::spawn(async move {
                                    agent::run_turn_with_commands(convo, msg, settings, tx2, cmd_rx)
                                        .await
                                }));
                                stick_to_bottom = true;
                                scroll = bottom_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                )?;
                            }
                        }
                        (KeyCode::Backspace, _) => {
                            input.pop();
                            autofill_selected = 0;
                        }
                        (KeyCode::Up, _) => {
                            if let Some(menu) = &autofill {
                                autofill_selected = menu.previous_index();
                            } else {
                                scroll = scroll.saturating_sub(1);
                                stick_to_bottom = false;
                            }
                        }
                        (KeyCode::Down, _) => {
                            if let Some(menu) = &autofill {
                                autofill_selected = menu.next_index();
                            } else {
                                let max = bottom_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    show_reasoning,
                                )?;
                                scroll = scroll.saturating_add(1).min(max);
                                stick_to_bottom = scroll >= max;
                            }
                        }
                        (KeyCode::PageUp, _) => {
                            scroll = scroll.saturating_sub(10);
                            stick_to_bottom = false;
                        }
                        (KeyCode::PageDown, _) => {
                            let max = bottom_scroll(
                                &terminal,
                                &input,
                                &transcript,
                                show_full_tools,
                                show_reasoning,
                            )?;
                            scroll = scroll.saturating_add(10).min(max);
                            stick_to_bottom = scroll >= max;
                        }
                        (KeyCode::Char(ch), m)
                            if !m.contains(KeyModifiers::CONTROL)
                                && !m.contains(KeyModifiers::ALT) =>
                        {
                            input.push(ch);
                            autofill_selected = 0;
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        scroll = scroll.saturating_sub(3);
                        stick_to_bottom = false;
                    }
                    MouseEventKind::ScrollDown => {
                        let max = bottom_scroll(
                            &terminal,
                            &input,
                            &transcript,
                            show_full_tools,
                            show_reasoning,
                        )?;
                        scroll = scroll.saturating_add(3).min(max);
                        stick_to_bottom = scroll >= max;
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        if last_ctrl_c
            .map(|t| t.elapsed() > Duration::from_millis(1500))
            .unwrap_or(false)
        {
            last_ctrl_c = None;
        }
        if last_esc
            .map(|t| t.elapsed() > Duration::from_millis(1500))
            .unwrap_or(false)
        {
            last_esc = None;
        }
    }
}

#[derive(Debug, Clone)]
struct PendingApproval {
    request_id: String,
    block_index: usize,
}

#[derive(Debug, Clone)]
enum BranchMenuMode {
    Main,
    Actions(crate::branch::Checkpoint),
}

#[derive(Debug, Clone)]
struct BranchMenuState {
    mode: BranchMenuMode,
    selected: usize,
    family: crate::branch::BranchFamily,
}

#[derive(Debug, Clone)]
enum BranchMenuItem {
    Switch(String),
    Checkpoint(crate::branch::Checkpoint),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchMenuOutcome {
    None,
    Changed,
}

impl BranchMenuState {
    fn from_family(family: crate::branch::BranchFamily) -> Self {
        Self {
            mode: BranchMenuMode::Main,
            selected: 0,
            family,
        }
    }

    fn overlay_view(&self) -> render::OverlayView {
        match &self.mode {
            BranchMenuMode::Main => render::OverlayView {
                title: "Branch / Restore".into(),
                help: "Enter select · Esc cancel · ↑/↓ move".into(),
                selected: self.selected,
                items: self
                    .main_items()
                    .into_iter()
                    .map(|item| match item {
                        BranchMenuItem::Switch(id) => {
                            let branch = self.family.branches.iter().find(|b| b.id == id);
                            let mut label = if branch.is_some_and(|b| b.current) {
                                format!("current branch {id}")
                            } else {
                                format!("switch to {id}")
                            };
                            if branch.and_then(|b| b.parent_chat_id.as_ref()).is_none() {
                                label.push_str(" (root)");
                            }
                            render::OverlayItem {
                                label,
                                detail: branch.and_then(|b| b.branch_label.clone()).unwrap_or_else(
                                    || {
                                        branch
                                            .map(|b| format!("{} records", b.record_count))
                                            .unwrap_or_default()
                                    },
                                ),
                            }
                        }
                        BranchMenuItem::Checkpoint(checkpoint) => render::OverlayItem {
                            label: format!("{} · {}", checkpoint.chat_id, checkpoint.label),
                            detail: checkpoint.detail,
                        },
                    })
                    .collect(),
            },
            BranchMenuMode::Actions(checkpoint) => render::OverlayView {
                title: "Branch from checkpoint".into(),
                help: format!(
                    "{} · Enter select · Esc back",
                    crate::branch::checkpoint_title(checkpoint)
                ),
                selected: self.selected,
                items: vec![
                    render::OverlayItem {
                        label: "Branch conversation only".into(),
                        detail: "safe default; leaves files unchanged".into(),
                    },
                    render::OverlayItem {
                        label: "Branch conversation and restore tracked files".into(),
                        detail: "applies safe Cassady write/edit snapshots; conflicts are skipped"
                            .into(),
                    },
                    render::OverlayItem {
                        label: "Preview tracked file restore plan".into(),
                        detail: "show file actions in transcript".into(),
                    },
                    render::OverlayItem {
                        label: "Cancel".into(),
                        detail: String::new(),
                    },
                ],
            },
        }
    }

    fn main_items(&self) -> Vec<BranchMenuItem> {
        let mut items = Vec::new();
        for branch in &self.family.branches {
            items.push(BranchMenuItem::Switch(branch.id.clone()));
        }
        for checkpoint in &self.family.checkpoints {
            items.push(BranchMenuItem::Checkpoint(checkpoint.clone()));
        }
        items
    }

    fn len(&self) -> usize {
        match self.mode {
            BranchMenuMode::Main => self.main_items().len(),
            BranchMenuMode::Actions(_) => 4,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_branch_menu_key(
    code: KeyCode,
    menu: &mut Option<BranchMenuState>,
    config: &Config,
    _cwd: &Path,
    conversation: &mut Conversation,
    chat_id: &mut String,
    transcript: &mut Vec<TranscriptBlock>,
    active_assistant: &mut Option<usize>,
    active_reasoning: &mut Option<usize>,
    active_tools: &mut HashMap<String, usize>,
    status: &mut String,
) -> Result<BranchMenuOutcome> {
    let Some(state) = menu.as_mut() else {
        return Ok(BranchMenuOutcome::None);
    };
    match code {
        KeyCode::Esc => match state.mode {
            BranchMenuMode::Main => {
                *menu = None;
                *status = "branch menu cancelled".into();
            }
            BranchMenuMode::Actions(_) => {
                state.mode = BranchMenuMode::Main;
                state.selected = 0;
            }
        },
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = state.len().saturating_sub(1);
            state.selected = state.selected.saturating_add(1).min(max);
        }
        KeyCode::Enter => {
            return apply_branch_menu_selection(
                menu,
                config,
                conversation,
                chat_id,
                transcript,
                active_assistant,
                active_reasoning,
                active_tools,
                status,
            );
        }
        _ => {}
    }
    Ok(BranchMenuOutcome::None)
}

#[allow(clippy::too_many_arguments)]
fn apply_branch_menu_selection(
    menu: &mut Option<BranchMenuState>,
    config: &Config,
    conversation: &mut Conversation,
    chat_id: &mut String,
    transcript: &mut Vec<TranscriptBlock>,
    active_assistant: &mut Option<usize>,
    active_reasoning: &mut Option<usize>,
    active_tools: &mut HashMap<String, usize>,
    status: &mut String,
) -> Result<BranchMenuOutcome> {
    let Some(state) = menu.as_mut() else {
        return Ok(BranchMenuOutcome::None);
    };
    match &state.mode {
        BranchMenuMode::Main => {
            let items = state.main_items();
            let Some(item) = items.get(state.selected).cloned() else {
                return Ok(BranchMenuOutcome::None);
            };
            match item {
                BranchMenuItem::Switch(id) => {
                    let (loaded, warning) = Conversation::load(&config.conversations_dir(), &id)?;
                    *conversation = loaded;
                    *chat_id = conversation.id.clone();
                    *transcript = transcript_from_loaded(conversation, warning);
                    *active_assistant = None;
                    *active_reasoning = None;
                    active_tools.clear();
                    *status = format!("switched to branch {chat_id}");
                    *menu = None;
                    Ok(BranchMenuOutcome::Changed)
                }
                BranchMenuItem::Checkpoint(checkpoint) => {
                    state.mode = BranchMenuMode::Actions(checkpoint);
                    state.selected = 0;
                    Ok(BranchMenuOutcome::None)
                }
            }
        }
        BranchMenuMode::Actions(checkpoint) => {
            let selected = state.selected;
            let checkpoint = checkpoint.clone();
            match selected {
                0 => branch_from_checkpoint(
                    menu,
                    config,
                    &checkpoint,
                    false,
                    conversation,
                    chat_id,
                    transcript,
                    active_assistant,
                    active_reasoning,
                    active_tools,
                    status,
                ),
                1 => branch_from_checkpoint(
                    menu,
                    config,
                    &checkpoint,
                    true,
                    conversation,
                    chat_id,
                    transcript,
                    active_assistant,
                    active_reasoning,
                    active_tools,
                    status,
                ),
                2 => {
                    let summary = crate::commands::preview_restore_plan(config, &checkpoint)?;
                    transcript.push(TranscriptBlock {
                        kind: TranscriptKind::Status,
                        title: "restore preview".into(),
                        content: summary,
                    });
                    *status = "restore plan previewed".into();
                    *menu = None;
                    Ok(BranchMenuOutcome::Changed)
                }
                _ => {
                    state.mode = BranchMenuMode::Main;
                    state.selected = 0;
                    Ok(BranchMenuOutcome::None)
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn branch_from_checkpoint(
    menu: &mut Option<BranchMenuState>,
    config: &Config,
    checkpoint: &crate::branch::Checkpoint,
    restore_files: bool,
    conversation: &mut Conversation,
    chat_id: &mut String,
    transcript: &mut Vec<TranscriptBlock>,
    active_assistant: &mut Option<usize>,
    active_reasoning: &mut Option<usize>,
    active_tools: &mut HashMap<String, usize>,
    status: &mut String,
) -> Result<BranchMenuOutcome> {
    let outcome =
        crate::commands::create_branch_from_checkpoint(config, checkpoint, restore_files)?;
    *conversation = outcome.conversation;
    *chat_id = conversation.id.clone();
    *transcript = transcript_from_loaded(conversation, None);
    *active_assistant = None;
    *active_reasoning = None;
    active_tools.clear();

    if let Some(restore) = outcome.restore {
        transcript.push(TranscriptBlock {
            kind: if restore.is_error() {
                TranscriptKind::Error
            } else {
                TranscriptKind::Status
            },
            title: "file restore".into(),
            content: restore.transcript_content(),
        });
    }

    *status = outcome.status;
    *menu = None;
    Ok(BranchMenuOutcome::Changed)
}

struct AgentEventContext<'a> {
    terminal: &'a terminal::CassTerminal,
    input: &'a str,
    transcript: &'a mut Vec<TranscriptBlock>,
    active_assistant: &'a mut Option<usize>,
    active_reasoning: &'a mut Option<usize>,
    active_tools: &'a mut HashMap<String, usize>,
    pending_approval: &'a mut Option<PendingApproval>,
    status: &'a mut String,
    stick_to_bottom: bool,
    show_full_tools: bool,
    show_reasoning: bool,
    scroll: &'a mut u16,
}

fn drain_agent_events(
    rx: &mut mpsc::UnboundedReceiver<AgentEvent>,
    ctx: &mut AgentEventContext<'_>,
) -> Result<()> {
    while let Ok(event) = rx.try_recv() {
        apply_agent_event(event, ctx)?;
    }
    Ok(())
}

fn apply_agent_event(event: AgentEvent, ctx: &mut AgentEventContext<'_>) -> Result<()> {
    match event {
        AgentEvent::AssistantChunk(s) => {
            if ctx.active_assistant.is_none() && s.trim().is_empty() {
                return Ok(());
            }
            let idx = match *ctx.active_assistant {
                Some(i) => i,
                None => {
                    ctx.transcript.push(TranscriptBlock {
                        kind: TranscriptKind::Assistant,
                        title: "response".into(),
                        content: String::new(),
                    });
                    let i = ctx.transcript.len() - 1;
                    *ctx.active_assistant = Some(i);
                    i
                }
            };
            ctx.transcript[idx].content.push_str(&s);
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::ReasoningChunk(s) => {
            if ctx.active_reasoning.is_none() && s.trim().is_empty() {
                return Ok(());
            }
            let idx = match *ctx.active_reasoning {
                Some(i) => i,
                None => {
                    ctx.transcript.push(TranscriptBlock {
                        kind: TranscriptKind::Reasoning,
                        title: "reasoning".into(),
                        content: String::new(),
                    });
                    let i = ctx.transcript.len() - 1;
                    *ctx.active_reasoning = Some(i);
                    i
                }
            };
            ctx.transcript[idx].content.push_str(&s);
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::ToolCallStarted {
            id,
            name,
            arguments,
        } => {
            *ctx.active_assistant = None;
            *ctx.active_reasoning = None;
            ctx.transcript.push(TranscriptBlock {
                kind: TranscriptKind::Tool,
                title: format!("{name} … ({})", short_call_id(&id)),
                content: summarize_tool_arguments(&name, &arguments),
            });
            ctx.active_tools.insert(id, ctx.transcript.len() - 1);
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::ToolOutputChunk {
            id,
            name,
            stream,
            content,
        } => {
            *ctx.active_assistant = None;
            *ctx.active_reasoning = None;
            let idx = active_tool_block(ctx, &id, &name);
            append_tool_output_chunk(&mut ctx.transcript[idx].content, &stream, &content);
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::ToolResult {
            id,
            name,
            ok,
            content,
        } => {
            if name == "shell" {
                if let Some(idx) = ctx.active_tools.remove(&id) {
                    ctx.transcript[idx].kind = if ok {
                        TranscriptKind::Tool
                    } else {
                        TranscriptKind::Error
                    };
                    ctx.transcript[idx].title = format!(
                        "{name} {} ({})",
                        if ok { "✓" } else { "✗" },
                        short_call_id(&id)
                    );
                    ctx.transcript[idx].content = content;
                    update_bottom_scroll(ctx)?;
                    return Ok(());
                }
            }
            if let Some(idx) = ctx.active_tools.remove(&id) {
                remove_transcript_block(ctx, idx);
            }
            ctx.transcript.push(TranscriptBlock {
                kind: if ok {
                    TranscriptKind::Tool
                } else {
                    TranscriptKind::Error
                },
                title: format!(
                    "{name} {} ({})",
                    if ok { "✓" } else { "✗" },
                    short_call_id(&id)
                ),
                content,
            });
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::ApprovalRequested {
            request_id,
            tool_call_id,
            name,
            arguments,
            reason,
        } => {
            *ctx.active_assistant = None;
            *ctx.active_reasoning = None;
            let args = summarize_tool_arguments(&name, &arguments);
            let block_index = ctx.transcript.len();
            *ctx.pending_approval = Some(PendingApproval {
                request_id: request_id.clone(),
                block_index,
            });
            ctx.transcript.push(TranscriptBlock {
                kind: TranscriptKind::Status,
                title: format!("approval required ({})", short_call_id(&tool_call_id)),
                content: format!(
                    "{name} requires approval before execution.\n\nReason: {reason}\n\nArguments:\n{args}\n\nPress y to approve, n or Esc to deny, Ctrl-C to cancel the turn."
                ),
            });
            *ctx.status = "approval required: press y to approve, n to deny".into();
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::ApprovalResolved {
            request_id,
            approved,
        } => {
            if ctx
                .pending_approval
                .as_ref()
                .is_some_and(|pending| pending.request_id == request_id)
            {
                hide_pending_approval(
                    ctx.pending_approval,
                    ctx.transcript,
                    ctx.active_assistant,
                    ctx.active_reasoning,
                    ctx.active_tools,
                );
            }
            *ctx.status = if approved {
                "approval accepted"
            } else {
                "approval denied"
            }
            .into();
        }
        AgentEvent::Status(s) => {
            ctx.transcript.push(TranscriptBlock {
                kind: TranscriptKind::Status,
                title: "status".into(),
                content: s,
            });
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::TurnFinished => {
            *ctx.active_assistant = None;
            *ctx.active_reasoning = None;
            ctx.active_tools.clear();
            *ctx.status = "turn finished".into();
        }
    }
    Ok(())
}

fn active_tool_block(ctx: &mut AgentEventContext<'_>, id: &str, name: &str) -> usize {
    if let Some(idx) = ctx.active_tools.get(id).copied() {
        return idx;
    }
    ctx.transcript.push(TranscriptBlock {
        kind: TranscriptKind::Tool,
        title: format!("{name} … ({})", short_call_id(id)),
        content: String::new(),
    });
    let idx = ctx.transcript.len() - 1;
    ctx.active_tools.insert(id.to_string(), idx);
    idx
}

fn hide_pending_approval(
    pending_approval: &mut Option<PendingApproval>,
    transcript: &mut Vec<TranscriptBlock>,
    active_assistant: &mut Option<usize>,
    active_reasoning: &mut Option<usize>,
    active_tools: &mut HashMap<String, usize>,
) {
    let Some(pending) = pending_approval.take() else {
        return;
    };
    let idx = pending.block_index;
    if idx >= transcript.len() {
        return;
    }
    transcript.remove(idx);
    adjust_index_after_remove(active_assistant, idx);
    adjust_index_after_remove(active_reasoning, idx);
    adjust_active_tool_indices_after_remove(active_tools, idx);
}

fn remove_transcript_block(ctx: &mut AgentEventContext<'_>, idx: usize) {
    if idx >= ctx.transcript.len() {
        return;
    }
    ctx.transcript.remove(idx);
    adjust_index_after_remove(ctx.active_assistant, idx);
    adjust_index_after_remove(ctx.active_reasoning, idx);
    adjust_pending_approval_after_remove(ctx.pending_approval, idx);
    adjust_active_tool_indices_after_remove(ctx.active_tools, idx);
}

fn adjust_active_tool_indices_after_remove(
    active_tools: &mut HashMap<String, usize>,
    removed: usize,
) {
    active_tools.retain(|_, tool_idx| {
        if *tool_idx == removed {
            false
        } else {
            if *tool_idx > removed {
                *tool_idx -= 1;
            }
            true
        }
    });
}

fn adjust_pending_approval_after_remove(
    pending_approval: &mut Option<PendingApproval>,
    removed: usize,
) {
    if let Some(pending) = pending_approval {
        if pending.block_index == removed {
            *pending_approval = None;
        } else if pending.block_index > removed {
            pending.block_index -= 1;
        }
    }
}

fn adjust_index_after_remove(index: &mut Option<usize>, removed: usize) {
    if let Some(value) = index {
        if *value == removed {
            *index = None;
        } else if *value > removed {
            *value -= 1;
        }
    }
}

fn mark_active_tool_blocks_cancelled(
    transcript: &mut [TranscriptBlock],
    active_tools: &HashMap<String, usize>,
) {
    for idx in active_tools.values().copied() {
        let Some(block) = transcript.get_mut(idx) else {
            continue;
        };
        block.kind = TranscriptKind::Error;
        block.title = block.title.replace('…', "cancelled");
        if !block.content.ends_with('\n') && !block.content.is_empty() {
            block.content.push('\n');
        }
        block.content.push_str(TOOL_CANCELLED_MESSAGE);
    }
}

fn append_tool_output_chunk(existing: &mut String, stream: &str, chunk: &str) {
    if !existing.contains("streamed output:\n") {
        if !existing.trim().is_empty() {
            existing.push_str("\n\n");
        }
        existing.push_str("streamed output:\n");
    }
    if !existing.ends_with('\n') {
        existing.push('\n');
    }
    existing.push_str(&format!("[{stream}] "));
    existing.push_str(chunk);
    if !chunk.ends_with('\n') {
        existing.push('\n');
    }
}

fn update_bottom_scroll(ctx: &mut AgentEventContext<'_>) -> Result<()> {
    if ctx.stick_to_bottom {
        *ctx.scroll = bottom_scroll(
            ctx.terminal,
            ctx.input,
            ctx.transcript,
            ctx.show_full_tools,
            ctx.show_reasoning,
        )?;
    }
    Ok(())
}

fn ensure_final_assistant_visible(
    transcript: &mut Vec<TranscriptBlock>,
    conversation: &Conversation,
) -> bool {
    let Some(content) = conversation.records.last().and_then(|record| match record {
        conversation::Record::Assistant {
            content,
            tool_calls,
            ..
        } if tool_calls.is_empty() && !content.trim().is_empty() => Some(content),
        _ => None,
    }) else {
        return false;
    };

    if let Some(last) = transcript.last_mut() {
        if matches!(last.kind, TranscriptKind::Assistant) {
            if assistant_content_matches(&last.content, content) {
                return false;
            }
            if content.trim_start().starts_with(last.content.trim_start()) {
                last.content = content.clone();
                return true;
            }
            return false;
        }
    }

    transcript.push(TranscriptBlock {
        kind: TranscriptKind::Assistant,
        title: "response".into(),
        content: content.clone(),
    });
    true
}

fn assistant_content_matches(a: &str, b: &str) -> bool {
    a == b || (!a.trim().is_empty() && a.trim() == b.trim())
}

fn transcript_from_loaded(
    conversation: &Conversation,
    warning: Option<String>,
) -> Vec<TranscriptBlock> {
    let mut blocks = Vec::new();
    if let Some(w) = warning {
        blocks.push(TranscriptBlock {
            kind: TranscriptKind::Error,
            title: "warning".into(),
            content: w,
        });
    }
    blocks.extend(blocks_from_conversation(conversation));
    blocks
}

fn summarize_tool_arguments(name: &str, args: &serde_json::Value) -> String {
    match name {
        "read" => summarize_read_args(args),
        "write" => summarize_write_args(args),
        "edit" => summarize_edit_args(args),
        "shell" => summarize_shell_args(args),
        "grep" => summarize_grep_args(args),
        "ls" => summarize_ls_args(args),
        _ => pretty_json(args),
    }
}

fn summarize_read_args(args: &serde_json::Value) -> String {
    let Some(files) = args.get("files").and_then(|value| value.as_array()) else {
        return pretty_json(args);
    };
    if files.len() == 1 {
        let Some(file) = files.first() else {
            return pretty_json(args);
        };
        let Some(path) = file.get("path").and_then(|value| value.as_str()) else {
            return pretty_json(args);
        };
        let mut lines = vec![format!("file: {path}")];
        if let Some(range) = file.get("lines").and_then(|value| value.as_str()) {
            lines.push(format!("lines: {}", range.replace('-', "–")));
        }
        return lines.join("\n");
    }
    let mut lines = vec![format!("files: {}", files.len())];
    for file in files.iter().take(4) {
        if let Some(path) = file.get("path").and_then(|value| value.as_str()) {
            lines.push(format!("- {path}"));
        }
    }
    if files.len() > 4 {
        lines.push(format!("… {} more", files.len() - 4));
    }
    lines.join("\n")
}

fn summarize_write_args(args: &serde_json::Value) -> String {
    let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
        return pretty_json(args);
    };
    let mut lines = vec![format!("file: {path}")];
    if let Some(content) = args.get("content").and_then(|value| value.as_str()) {
        lines.push(format!("bytes: {}", human_bytes(content.len())));
    }
    lines.join("\n")
}

fn summarize_edit_args(args: &serde_json::Value) -> String {
    let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
        return pretty_json(args);
    };
    let Some(edits) = args.get("edits").and_then(|value| value.as_array()) else {
        return pretty_json(args);
    };
    format!("file: {path}\nedits: {}", edits.len())
}

fn summarize_shell_args(args: &serde_json::Value) -> String {
    let Some(command) = args.get("command").and_then(|value| value.as_str()) else {
        return pretty_json(args);
    };
    format!("command: {command}")
}

fn summarize_grep_args(args: &serde_json::Value) -> String {
    let Some(query) = args.get("query").and_then(|value| value.as_str()) else {
        return pretty_json(args);
    };
    let mut lines = vec![format!("query: {query}")];
    if let Some(paths) = args.get("paths").and_then(|value| value.as_array()) {
        if paths.len() == 1 {
            if let Some(path) = paths.first().and_then(|value| value.as_str()) {
                lines.push(format!("path: {path}"));
            }
        } else if !paths.is_empty() {
            lines.push(format!("paths: {}", paths.len()));
        }
    }
    if args.get("regex").and_then(|value| value.as_bool()) == Some(true) {
        lines.push("regex: true".into());
    }
    lines.join("\n")
}

fn summarize_ls_args(args: &serde_json::Value) -> String {
    let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
        return pretty_json(args);
    };
    format!("path: {path}")
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn human_bytes(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let bytes_f = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes_f < MB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{:.1} MB", bytes_f / MB)
    }
}

fn short_call_id(id: &str) -> String {
    if id.len() <= 12 {
        id.to_string()
    } else {
        format!("{}…{}", &id[..6], &id[id.len() - 4..])
    }
}

fn terminal_area(terminal: &terminal::CassTerminal) -> Result<ratatui::layout::Rect> {
    let size = terminal.size()?;
    Ok(ratatui::layout::Rect::new(0, 0, size.width, size.height))
}

fn bottom_scroll(
    terminal: &terminal::CassTerminal,
    input: &str,
    transcript: &[TranscriptBlock],
    show_full_tools: bool,
    show_reasoning: bool,
) -> Result<u16> {
    let area = render::transcript_area(terminal_area(terminal)?, input);
    Ok(render::max_transcript_scroll(
        transcript,
        show_full_tools,
        show_reasoning,
        area,
    ))
}

fn clamp_scroll(
    terminal: &terminal::CassTerminal,
    input: &str,
    transcript: &[TranscriptBlock],
    show_full_tools: bool,
    show_reasoning: bool,
    scroll: u16,
) -> Result<u16> {
    Ok(scroll.min(bottom_scroll(
        terminal,
        input,
        transcript,
        show_full_tools,
        show_reasoning,
    )?))
}

fn blocks_from_conversation(conversation: &Conversation) -> Vec<TranscriptBlock> {
    let completed_tool_calls: HashSet<&str> = conversation
        .records
        .iter()
        .filter_map(|record| match record {
            conversation::Record::Tool { tool_call_id, .. } => Some(tool_call_id.as_str()),
            _ => None,
        })
        .collect();
    let mut blocks = Vec::new();
    for r in &conversation.records {
        match r {
            conversation::Record::User { content, .. } => blocks.push(TranscriptBlock {
                kind: TranscriptKind::User,
                title: "message".into(),
                content: content.clone(),
            }),
            conversation::Record::Assistant {
                content,
                reasoning,
                tool_calls,
                ..
            } => {
                if !reasoning.trim().is_empty() {
                    blocks.push(TranscriptBlock {
                        kind: TranscriptKind::Reasoning,
                        title: "reasoning".into(),
                        content: reasoning.clone(),
                    });
                }
                if !content.trim().is_empty() {
                    blocks.push(TranscriptBlock {
                        kind: TranscriptKind::Assistant,
                        title: "response".into(),
                        content: content.clone(),
                    });
                }
                for call in tool_calls {
                    if completed_tool_calls.contains(call.id.as_str()) {
                        continue;
                    }
                    blocks.push(TranscriptBlock {
                        kind: TranscriptKind::Tool,
                        title: format!("{} … ({})", call.name, short_call_id(&call.id)),
                        content: summarize_tool_arguments(&call.name, &call.arguments),
                    });
                }
            }
            conversation::Record::Tool {
                tool_call_id,
                name,
                ok,
                content,
                ..
            } => blocks.push(TranscriptBlock {
                kind: if *ok {
                    TranscriptKind::Tool
                } else {
                    TranscriptKind::Error
                },
                title: format!(
                    "{name} {} ({})",
                    if *ok { "✓" } else { "✗" },
                    short_call_id(tool_call_id)
                ),
                content: content.clone(),
            }),
            _ => {}
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn summarize_edit_args_uses_edits() {
        let summary = summarize_tool_arguments(
            "edit",
            &serde_json::json!({"path":"src/ui/render.rs","edits":[{"old_text":"a","new_text":"b"},{"old_text":"c","new_text":"d"}]}),
        );

        assert_eq!(summary, "file: src/ui/render.rs\nedits: 2");
        assert!(!summary.contains("replacements"));
    }

    #[test]
    fn summarize_shell_args_uses_command() {
        let summary =
            summarize_tool_arguments("shell", &serde_json::json!({"command":"cargo test"}));

        assert_eq!(summary, "command: cargo test");
    }

    #[test]
    fn summarize_read_args_uses_file_and_lines() {
        let summary = summarize_tool_arguments(
            "read",
            &serde_json::json!({"files":[{"path":"src/app.rs","lines":"1-20"}]}),
        );

        assert_eq!(summary, "file: src/app.rs\nlines: 1–20");
    }

    #[test]
    fn summarize_unknown_tool_falls_back_to_json() {
        let summary = summarize_tool_arguments("custom", &serde_json::json!({"alpha":1}));

        assert!(summary.contains("\"alpha\": 1"));
    }

    #[test]
    fn loaded_transcript_hides_completed_tool_call_invocations() {
        let root = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        let config = Config {
            root: root.path().to_path_buf(),
            model: "test-model".into(),
            ..Config::default()
        };
        let mut conversation = Conversation::create(
            &config.conversations_dir(),
            &config.model,
            cwd.path(),
            "base prompt".into(),
        )
        .unwrap();
        conversation
            .append(Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![conversation::StoredToolCall {
                    id: "call_done".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"files":[{"path":"src/app.rs"}]}),
                }],
                ts: conversation::now_ts(),
            })
            .unwrap();
        conversation
            .append(Record::Tool {
                tool_call_id: "call_done".into(),
                name: "read".into(),
                ok: true,
                content: "ok".into(),
                ts: conversation::now_ts(),
            })
            .unwrap();

        let blocks = blocks_from_conversation(&conversation);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].title, "read ✓ (call_done)");
    }

    #[test]
    fn loaded_transcript_keeps_pending_tool_call_invocations() {
        let root = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        let config = Config {
            root: root.path().to_path_buf(),
            model: "test-model".into(),
            ..Config::default()
        };
        let mut conversation = Conversation::create(
            &config.conversations_dir(),
            &config.model,
            cwd.path(),
            "base prompt".into(),
        )
        .unwrap();
        conversation
            .append(Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![conversation::StoredToolCall {
                    id: "call_pending".into(),
                    name: "shell".into(),
                    arguments: serde_json::json!({"command":"sleep 60"}),
                }],
                ts: conversation::now_ts(),
            })
            .unwrap();

        let blocks = blocks_from_conversation(&conversation);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].title, "shell … (call_pending)");
    }

    #[test]
    fn cancelled_turn_repairs_missing_tool_results() {
        let root = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        let config = Config {
            root: root.path().to_path_buf(),
            model: "test-model".into(),
            ..Config::default()
        };
        let mut conversation = Conversation::create(
            &config.conversations_dir(),
            &config.model,
            cwd.path(),
            "base prompt".into(),
        )
        .unwrap();
        let chat_id = conversation.id.clone();
        let start_len = conversation.records.len();
        conversation
            .append(Record::User {
                content: "run tools".into(),
                ts: conversation::now_ts(),
            })
            .unwrap();
        conversation
            .append(Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![
                    conversation::StoredToolCall {
                        id: "call_done".into(),
                        name: "read".into(),
                        arguments: serde_json::json!({"path":"a"}),
                    },
                    conversation::StoredToolCall {
                        id: "call_pending".into(),
                        name: "shell".into(),
                        arguments: serde_json::json!({"command":"sleep 60"}),
                    },
                ],
                ts: conversation::now_ts(),
            })
            .unwrap();
        conversation
            .append(Record::Tool {
                tool_call_id: "call_done".into(),
                name: "read".into(),
                ok: true,
                content: "ok".into(),
                ts: conversation::now_ts(),
            })
            .unwrap();

        let updated =
            finalize_cancelled_turn(&config, &chat_id, Some(start_len), Some("run tools")).unwrap();

        assert!(matches!(
            updated.records.get(updated.records.len() - 2),
            Some(Record::Tool { tool_call_id, name, ok, content, .. })
                if tool_call_id == "call_pending"
                    && name == "shell"
                    && !ok
                    && content == TOOL_CANCELLED_MESSAGE
        ));
        assert!(matches!(
            updated.records.last(),
            Some(Record::Assistant { content, tool_calls, .. })
                if content == TURN_CANCELLED_MESSAGE && tool_calls.is_empty()
        ));
    }

    #[test]
    fn ensure_final_assistant_visible_appends_missing_final() {
        let conversation = Conversation {
            id: "chat".into(),
            path: PathBuf::new(),
            records: vec![conversation::Record::Assistant {
                content: "Done.".into(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: Vec::new(),
                ts: "now".into(),
            }],
        };
        let mut transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Tool,
            title: "result: read ✓ (call_1)".into(),
            content: "ok".into(),
        }];

        assert!(ensure_final_assistant_visible(
            &mut transcript,
            &conversation
        ));
        assert!(matches!(
            transcript.last(),
            Some(TranscriptBlock {
                kind: TranscriptKind::Assistant,
                content,
                ..
            }) if content == "Done."
        ));
    }

    #[test]
    fn ensure_final_assistant_visible_does_not_duplicate_streamed_final() {
        let conversation = Conversation {
            id: "chat".into(),
            path: PathBuf::new(),
            records: vec![conversation::Record::Assistant {
                content: "\nDone.".into(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: Vec::new(),
                ts: "now".into(),
            }],
        };
        let mut transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Assistant,
            title: "response".into(),
            content: "Done.".into(),
        }];

        assert!(!ensure_final_assistant_visible(
            &mut transcript,
            &conversation
        ));
        assert_eq!(transcript.len(), 1);
    }
}
