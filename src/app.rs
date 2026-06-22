use crate::agent::{self, AgentEvent, AgentSettings};
use crate::cli::{self, Command};
use crate::config::{Config, ModelDefinition};
use crate::conversation::{self, Conversation};
use crate::prompt;
use crate::ui::autofill::{AutoFillItem, AutoFillMenu};
use crate::ui::events::poll_event;
use crate::ui::render::{self, TranscriptBlock, TranscriptKind};
use crate::ui::terminal;
use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub async fn run() -> Result<()> {
    let cli = cli::parse();
    if matches!(cli.command, Some(Command::Check)) {
        let report = crate::check::run(&cli)?;
        print!("{}", report.render());
        if report.has_errors() {
            std::process::exit(1);
        }
        return Ok(());
    }

    let config = Config::load(&cli)?;
    let cwd = resolve_cwd(cli.cwd.clone())?;

    if matches!(cli.resume, Some(None)) {
        list_chats(&config, &cwd)?;
        return Ok(());
    }

    let (conversation, warning) = if let Some(Some(id)) = cli.resume.clone() {
        Conversation::load(&config.conversations_dir(), &id)?
    } else {
        (create_new_conversation(&config, &cwd)?, None)
    };

    run_tui(config, cwd, conversation, warning).await
}

fn resolve_cwd(cwd: Option<PathBuf>) -> Result<PathBuf> {
    let cwd = cwd.unwrap_or(std::env::current_dir()?);
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("resolving cwd {}", cwd.display()))?;
    std::env::set_current_dir(&cwd)?;
    Ok(cwd)
}

fn create_new_conversation(config: &Config, cwd: &Path) -> Result<Conversation> {
    let global = fs::read_to_string(config.global_path()).ok();
    let base = prompt::build_base_system_prompt(global.as_deref());
    Conversation::create(&config.conversations_dir(), &config.model, cwd, base)
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

async fn run_tui(
    mut config: Config,
    cwd: PathBuf,
    mut conversation: Conversation,
    warning: Option<String>,
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
    let mut input = String::new();
    let mut mode = config.default_access_mode;
    let mut status = String::new();
    let mut show_full_tools = false;
    let mut show_reasoning = config.show_reasoning;
    let mut scroll: u16 = 0;
    let mut last_ctrl_c: Option<Instant> = None;
    let mut handle: Option<JoinHandle<Result<Conversation>>> = None;
    let mut active_assistant: Option<usize> = None;
    let mut active_reasoning: Option<usize> = None;
    let mut stick_to_bottom = true;
    let mut chat_id = conversation.id.clone();
    let mut autofill_selected = 0usize;

    loop {
        drain_agent_events(
            &mut rx,
            &mut AgentEventContext {
                terminal: &terminal,
                input: &input,
                transcript: &mut transcript,
                active_assistant: &mut active_assistant,
                active_reasoning: &mut active_reasoning,
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
                    status: &mut status,
                    stick_to_bottom,
                    show_full_tools,
                    show_reasoning,
                    scroll: &mut scroll,
                },
            )?;
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
                Err(err) => transcript.push(TranscriptBlock {
                    kind: TranscriptKind::Error,
                    title: "agent task error".into(),
                    content: err.to_string(),
                }),
            }
            active_assistant = None;
            active_reasoning = None;
            status = "idle".into();
        }

        let autofill = build_autofill(&input, autofill_selected, &config, &cwd)?;
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
                    scroll,
                    autofill: autofill.as_ref(),
                },
            )
        })?;

        if let Some(event) = poll_event(Duration::from_millis(40))? {
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let busy = handle.is_some();
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                            let now = Instant::now();
                            input.clear();
                            autofill_selected = 0;
                            if last_ctrl_c
                                .map(|t| now.duration_since(t) <= Duration::from_millis(1500))
                                .unwrap_or(false)
                            {
                                terminal::leave(terminal)?;
                                println!("Resume this chat with: cass --resume {}", chat_id);
                                return Ok(());
                            }
                            last_ctrl_c = Some(now);
                            status = "press Ctrl-C again within 1.5s to exit".into();
                        }
                        (KeyCode::BackTab, _) => {
                            if busy {
                                status = "mode can be changed when idle".into();
                            } else {
                                mode = mode.toggle();
                                status = format!("mode: {mode}");
                            }
                        }
                        (KeyCode::Tab, _) => {
                            if let Some(menu) = &autofill {
                                if let Some(next_input) = menu.apply(&input) {
                                    input = next_input;
                                    autofill_selected = 0;
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
                                match parse_local_command(&input) {
                                    Ok(LocalCommand::Status) => {
                                        let content = chat_status(
                                            &chat_id,
                                            &config.model,
                                            mode,
                                            &cwd,
                                            busy,
                                            &status,
                                            conversation.records.len(),
                                        );
                                        input.clear();
                                        autofill_selected = 0;
                                        transcript.push(TranscriptBlock {
                                            kind: TranscriptKind::Status,
                                            title: "status".into(),
                                            content,
                                        });
                                        status = "status shown".into();
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
                                    Ok(LocalCommand::Model(model)) => {
                                        if busy {
                                            status = "model can be changed when idle".into();
                                        } else {
                                            config.model = model.clone();
                                            input.clear();
                                            autofill_selected = 0;
                                            transcript.push(TranscriptBlock {
                                                kind: TranscriptKind::Status,
                                                title: "model".into(),
                                                content: format!("model changed to {model}"),
                                            });
                                            status = format!("model: {model}");
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
                                    Ok(LocalCommand::New) => {
                                        if busy {
                                            status = "new chat can be created when idle".into();
                                        } else {
                                            match create_new_conversation(&config, &cwd) {
                                                Ok(new_conversation) => {
                                                    conversation = new_conversation;
                                                    chat_id = conversation.id.clone();
                                                    transcript.clear();
                                                    input.clear();
                                                    autofill_selected = 0;
                                                    active_assistant = None;
                                                    active_reasoning = None;
                                                    status = format!("new chat {chat_id}");
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
                                                    status = format!("new chat failed: {err}");
                                                    transcript.push(TranscriptBlock {
                                                        kind: TranscriptKind::Error,
                                                        title: "new".into(),
                                                        content: err.to_string(),
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
                                        }
                                    }
                                    Ok(LocalCommand::Resume(id)) => {
                                        if busy {
                                            status = "chat can be resumed when idle".into();
                                        } else {
                                            match Conversation::load(
                                                &config.conversations_dir(),
                                                &id,
                                            ) {
                                                Ok((loaded, warning)) => {
                                                    conversation = loaded;
                                                    chat_id = conversation.id.clone();
                                                    transcript = transcript_from_loaded(
                                                        &conversation,
                                                        warning,
                                                    );
                                                    input.clear();
                                                    autofill_selected = 0;
                                                    active_assistant = None;
                                                    active_reasoning = None;
                                                    status = format!("resumed chat {chat_id}");
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
                                                    status = format!("resume failed: {err}");
                                                    transcript.push(TranscriptBlock {
                                                        kind: TranscriptKind::Error,
                                                        title: "resume".into(),
                                                        content: err.to_string(),
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
                                        }
                                    }
                                    Err(err) => {
                                        status = err;
                                    }
                                }
                            } else if busy {
                                status = "agent is still running".into();
                            } else {
                                let msg = input.trim_end().to_string();
                                input.clear();
                                autofill_selected = 0;
                                transcript.push(TranscriptBlock {
                                    kind: TranscriptKind::User,
                                    title: "message".into(),
                                    content: msg.clone(),
                                });
                                active_assistant = None;
                                active_reasoning = None;
                                status = "running".into();
                                let settings = AgentSettings {
                                    config: config.clone(),
                                    cwd: cwd.clone(),
                                    mode,
                                };
                                let convo = conversation.clone();
                                let tx2 = tx.clone();
                                handle = Some(tokio::spawn(async move {
                                    agent::run_turn(convo, msg, settings, tx2).await
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
    }
}

struct AgentEventContext<'a> {
    terminal: &'a terminal::CassTerminal,
    input: &'a str,
    transcript: &'a mut Vec<TranscriptBlock>,
    active_assistant: &'a mut Option<usize>,
    active_reasoning: &'a mut Option<usize>,
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
                content: serde_json::to_string_pretty(&arguments)
                    .unwrap_or_else(|_| arguments.to_string()),
            });
            update_bottom_scroll(ctx)?;
        }
        AgentEvent::ToolResult {
            id,
            name,
            ok,
            content,
        } => {
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
            *ctx.status = "turn finished".into();
        }
    }
    Ok(())
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalCommand {
    Model(String),
    New,
    Resume(String),
    Status,
}

struct CommandSpec {
    name: &'static str,
    usage: &'static str,
    description: &'static str,
    takes_value: bool,
}

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "model",
        usage: "/model <model>",
        description: "switch the model for future turns",
        takes_value: true,
    },
    CommandSpec {
        name: "new",
        usage: "/new",
        description: "create a new chat",
        takes_value: false,
    },
    CommandSpec {
        name: "resume",
        usage: "/resume <chat>",
        description: "resume a chat from this directory",
        takes_value: true,
    },
    CommandSpec {
        name: "status",
        usage: "/status",
        description: "show chat status",
        takes_value: false,
    },
];

fn build_autofill(
    input: &str,
    selected: usize,
    config: &Config,
    cwd: &Path,
) -> Result<Option<AutoFillMenu>> {
    if input.contains('\n') || !input.starts_with('/') {
        return Ok(None);
    }

    if let Some(menu) = command_autofill(input, selected) {
        return Ok(Some(menu));
    }

    if let Some(menu) = model_autofill(input, selected, config)? {
        return Ok(Some(menu));
    }

    resume_chat_autofill(input, selected, config, cwd)
}

fn command_autofill(input: &str, selected: usize) -> Option<AutoFillMenu> {
    if !input.starts_with('/') || input[1..].chars().any(char::is_whitespace) {
        return None;
    }
    if COMMANDS
        .iter()
        .any(|spec| !spec.takes_value && input == format!("/{}", spec.name))
    {
        return None;
    }

    let query = input[1..].to_ascii_lowercase();
    let mut items = Vec::new();
    for spec in COMMANDS {
        if spec.name.starts_with(&query) || spec.usage[1..].starts_with(&query) {
            let insert = if spec.takes_value {
                format!("/{} ", spec.name)
            } else {
                format!("/{}", spec.name)
            };
            items.push(
                AutoFillItem::new(spec.usage, insert).with_detail(spec.description.to_string()),
            );
        }
    }

    if items.is_empty() {
        None
    } else {
        Some(AutoFillMenu::new("Commands", 0, input.len(), items).with_selected(selected))
    }
}

fn model_autofill(input: &str, selected: usize, config: &Config) -> Result<Option<AutoFillMenu>> {
    let Some(rest) = input.strip_prefix("/model") else {
        return Ok(None);
    };
    if rest.is_empty() || !rest.chars().next().is_some_and(|c| c.is_whitespace()) {
        return Ok(None);
    }

    let arg = rest.trim_start_matches(char::is_whitespace);
    if arg.chars().any(char::is_whitespace) {
        return Ok(None);
    }
    let replacement_start = input.len() - arg.len();
    let query = arg.to_ascii_lowercase();

    let models = crate::config::load_or_create_default_model_registry(&config.root)?;
    if !arg.is_empty() && models.models.iter().any(|model| model.id == arg) {
        return Ok(None);
    }

    let mut items = Vec::new();
    for model in models.models {
        if model_matches(&model, &query) {
            let id = model.id.clone();
            let detail = model_detail(&model, &config.model);
            items.push(AutoFillItem::new(id.clone(), id).with_detail(detail));
        }
    }

    if items.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            AutoFillMenu::new("Models", replacement_start, input.len(), items)
                .with_selected(selected),
        ))
    }
}

fn model_matches(model: &ModelDefinition, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    model.id.to_ascii_lowercase().contains(query)
        || model.provider.to_ascii_lowercase().contains(query)
        || model
            .display_name
            .as_ref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(query))
}

fn model_detail(model: &ModelDefinition, current_model: &str) -> String {
    let mut parts = Vec::new();
    if model.id == current_model {
        parts.push("current".to_string());
    }
    if let Some(name) = &model.display_name {
        if !name.trim().is_empty() && name != &model.id {
            parts.push(name.clone());
        }
    }
    parts.push(format!("provider {}", model.provider));
    if let Some(context_length) = model.context_length {
        parts.push(format!("ctx {context_length}"));
    }
    if let Some(max_output_tokens) = model.max_output_tokens {
        parts.push(format!("max {max_output_tokens}"));
    }
    if !model.supports_tools {
        parts.push("no tools".to_string());
    }
    if !model.supports_streaming {
        parts.push("no streaming".to_string());
    }
    parts.join(" · ")
}

fn resume_chat_autofill(
    input: &str,
    selected: usize,
    config: &Config,
    cwd: &Path,
) -> Result<Option<AutoFillMenu>> {
    let Some(rest) = input.strip_prefix("/resume") else {
        return Ok(None);
    };
    if rest.is_empty() || !rest.chars().next().is_some_and(|c| c.is_whitespace()) {
        return Ok(None);
    }

    let arg = rest.trim_start_matches(char::is_whitespace);
    if arg.chars().any(char::is_whitespace) {
        return Ok(None);
    }
    let replacement_start = input.len() - arg.len();
    let query = arg.to_ascii_lowercase();

    let chats = conversation::list_chats(&config.conversations_dir(), cwd)?;
    if !arg.is_empty() && chats.iter().any(|chat| chat.id == arg) {
        return Ok(None);
    }

    let mut items = Vec::new();
    for chat in chats {
        if chat_matches(&chat, &query) {
            let detail = chat_detail(&chat);
            items.push(AutoFillItem::new(chat.id.clone(), chat.id).with_detail(detail));
        }
    }

    if items.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            AutoFillMenu::new("Chats", replacement_start, input.len(), items)
                .with_selected(selected),
        ))
    }
}

fn chat_matches(chat: &conversation::ChatSummary, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    chat.id.to_ascii_lowercase().contains(query)
        || chat.created_at.to_ascii_lowercase().contains(query)
        || chat.model.to_ascii_lowercase().contains(query)
        || chat.first_user_preview.to_ascii_lowercase().contains(query)
}

fn chat_detail(chat: &conversation::ChatSummary) -> String {
    let mut parts = vec![chat.created_at.clone(), short_model_name(&chat.model)];
    if !chat.first_user_preview.is_empty() {
        parts.push(chat.first_user_preview.clone());
    }
    parts.join(" · ")
}

fn short_model_name(model: &str) -> String {
    model.rsplit('/').next().unwrap_or(model).to_string()
}

fn parse_local_command(input: &str) -> std::result::Result<LocalCommand, String> {
    let trimmed = input.trim();
    let mut parts = trimmed.split_whitespace();
    let Some(command) = parts.next() else {
        return Err("empty command".into());
    };

    match command {
        "/model" => {
            let Some(model) = parts.next() else {
                return Err("usage: /model <model>".into());
            };
            if parts.next().is_some() {
                return Err("usage: /model <model>".into());
            }
            Ok(LocalCommand::Model(model.to_string()))
        }
        "/resume" => {
            let Some(chat) = parts.next() else {
                return Err("usage: /resume <chat>".into());
            };
            if parts.next().is_some() {
                return Err("usage: /resume <chat>".into());
            }
            Ok(LocalCommand::Resume(chat.to_string()))
        }
        "/new" => {
            if parts.next().is_some() {
                return Err("usage: /new".into());
            }
            Ok(LocalCommand::New)
        }
        "/status" => {
            if parts.next().is_some() {
                return Err("usage: /status".into());
            }
            Ok(LocalCommand::Status)
        }
        other => Err(format!("unknown command: {other}")),
    }
}

fn chat_status(
    chat_id: &str,
    model: &str,
    mode: crate::access::AccessMode,
    cwd: &Path,
    busy: bool,
    status: &str,
    record_count: usize,
) -> String {
    format!(
        "chat: {chat_id}\nstate: {}\nmodel: {model}\nmode: {mode}\ncwd: {}\nrecords: {record_count}\nstatus: {}",
        if busy { "running" } else { "idle" },
        cwd.display(),
        if status.is_empty() { "idle" } else { status }
    )
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
                    blocks.push(TranscriptBlock {
                        kind: TranscriptKind::Tool,
                        title: format!("{} … ({})", call.name, short_call_id(&call.id)),
                        content: serde_json::to_string_pretty(&call.arguments)
                            .unwrap_or_else(|_| call.arguments.to_string()),
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

    fn config_with_models(models_json: &str) -> (tempfile::TempDir, Config) {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join("models.json"), models_json).unwrap();
        let config = Config {
            root: root.path().to_path_buf(),
            model: "alpha-model".to_string(),
            ..Config::default()
        };
        (root, config)
    }

    #[test]
    fn command_autofill_lists_new_command_and_hides_exact_match() {
        let menu = command_autofill("/n", 0).unwrap();

        assert_eq!(menu.items.len(), 1);
        assert_eq!(menu.items[0].label, "/new");
        assert_eq!(menu.items[0].insert, "/new");
        assert_eq!(menu.apply("/n").unwrap(), "/new");
        assert!(command_autofill("/new", 0).is_none());
    }

    #[test]
    fn parse_local_command_accepts_new_without_args() {
        assert_eq!(parse_local_command("/new").unwrap(), LocalCommand::New);
        assert_eq!(parse_local_command("/new extra"), Err("usage: /new".into()));
    }

    #[test]
    fn model_autofill_lists_models_from_models_json() {
        let (_root, config) = config_with_models(
            r#"{
  "models": [
    {
      "id": "alpha-model",
      "provider": "fireworks",
      "display_name": "Alpha Model",
      "context_length": 1000,
      "max_output_tokens": 200
    },
    {
      "id": "beta-model",
      "provider": "other",
      "display_name": "Beta Model"
    }
  ]
}
"#,
        );

        let menu = model_autofill("/model ", 0, &config).unwrap().unwrap();

        assert_eq!(menu.items.len(), 2);
        assert_eq!(menu.items[0].label, "alpha-model");
        assert_eq!(menu.items[0].insert, "alpha-model");
        assert_eq!(menu.apply("/model ").unwrap(), "/model alpha-model");
        let detail = menu.items[0].detail.as_deref().unwrap();
        assert!(detail.contains("current"));
        assert!(detail.contains("Alpha Model"));
        assert!(detail.contains("provider fireworks"));
    }

    #[test]
    fn model_autofill_filters_and_hides_exact_matches() {
        let (_root, config) = config_with_models(
            r#"{
  "models": [
    { "id": "alpha-model", "provider": "fireworks" },
    { "id": "beta-model", "provider": "other", "display_name": "Beta Model" }
  ]
}
"#,
        );

        let menu = model_autofill("/model beta", 0, &config).unwrap().unwrap();
        assert_eq!(menu.items.len(), 1);
        assert_eq!(menu.items[0].insert, "beta-model");
        assert_eq!(menu.apply("/model beta").unwrap(), "/model beta-model");

        assert!(model_autofill("/model alpha-model", 0, &config)
            .unwrap()
            .is_none());
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
