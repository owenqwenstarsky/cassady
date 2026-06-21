use crate::agent::{self, AgentEvent, AgentSettings};
use crate::cli;
use crate::config::Config;
use crate::conversation::{self, Conversation};
use crate::prompt;
use crate::ui::events::poll_event;
use crate::ui::render::{self, TranscriptBlock, TranscriptKind};
use crate::ui::terminal;
use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub async fn run() -> Result<()> {
    let cli = cli::parse();
    let config = Config::load(&cli)?;
    let cwd = resolve_cwd(cli.cwd.clone())?;

    if matches!(cli.resume, Some(None)) {
        list_chats(&config, &cwd)?;
        return Ok(());
    }

    let (conversation, warning) = if let Some(Some(id)) = cli.resume.clone() {
        Conversation::load(&config.conversations_dir(), &id)?
    } else {
        let global = fs::read_to_string(config.global_path()).ok();
        let base = prompt::build_base_system_prompt(global.as_deref());
        (
            Conversation::create(&config.conversations_dir(), &config.model, &cwd, base)?,
            None,
        )
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
    config: Config,
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
    let mut scroll: u16 = 0;
    let mut last_ctrl_c: Option<Instant> = None;
    let mut handle: Option<JoinHandle<Result<Conversation>>> = None;
    let mut active_assistant: Option<usize> = None;
    let mut stick_to_bottom = true;
    let chat_id = conversation.id.clone();

    loop {
        scroll = clamp_scroll(&terminal, &input, &transcript, show_full_tools, scroll)?;

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
                    scroll,
                },
            )
        })?;

        while let Ok(event) = rx.try_recv() {
            match event {
                AgentEvent::AssistantChunk(s) => {
                    if active_assistant.is_none() && s.trim().is_empty() {
                        continue;
                    }
                    let idx = match active_assistant {
                        Some(i) => i,
                        None => {
                            transcript.push(TranscriptBlock {
                                kind: TranscriptKind::Assistant,
                                title: "response".into(),
                                content: String::new(),
                            });
                            let i = transcript.len() - 1;
                            active_assistant = Some(i);
                            i
                        }
                    };
                    transcript[idx].content.push_str(&s);
                    if stick_to_bottom {
                        scroll = bottom_scroll(&terminal, &input, &transcript, show_full_tools)?;
                    }
                }
                AgentEvent::ToolCallStarted {
                    id,
                    name,
                    arguments,
                } => {
                    active_assistant = None;
                    transcript.push(TranscriptBlock {
                        kind: TranscriptKind::Tool,
                        title: format!("call: {name} ({})", short_call_id(&id)),
                        content: serde_json::to_string_pretty(&arguments)
                            .unwrap_or_else(|_| arguments.to_string()),
                    });
                    if stick_to_bottom {
                        scroll = bottom_scroll(&terminal, &input, &transcript, show_full_tools)?;
                    }
                }
                AgentEvent::ToolResult {
                    id,
                    name,
                    ok,
                    content,
                } => {
                    transcript.push(TranscriptBlock {
                        kind: if ok {
                            TranscriptKind::Tool
                        } else {
                            TranscriptKind::Error
                        },
                        title: format!(
                            "result: {name} {} ({})",
                            if ok { "✓" } else { "✗" },
                            short_call_id(&id)
                        ),
                        content,
                    });
                    if stick_to_bottom {
                        scroll = bottom_scroll(&terminal, &input, &transcript, show_full_tools)?;
                    }
                }
                AgentEvent::Status(s) => {
                    transcript.push(TranscriptBlock {
                        kind: TranscriptKind::Status,
                        title: "status".into(),
                        content: s,
                    });
                    if stick_to_bottom {
                        scroll = bottom_scroll(&terminal, &input, &transcript, show_full_tools)?;
                    }
                }
                AgentEvent::TurnFinished => {
                    active_assistant = None;
                    status = "turn finished".into();
                }
            }
        }

        if handle.as_ref().map(|h| h.is_finished()).unwrap_or(false) {
            let h = handle.take().unwrap();
            match h.await {
                Ok(Ok(updated)) => conversation = updated,
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
            status = "idle".into();
        }

        if let Some(event) = poll_event(Duration::from_millis(40))? {
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let busy = handle.is_some();
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                            let now = Instant::now();
                            input.clear();
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
                        (KeyCode::Char('o'), m) if m.contains(KeyModifiers::CONTROL) => {
                            show_full_tools = !show_full_tools;
                            scroll = if stick_to_bottom {
                                bottom_scroll(&terminal, &input, &transcript, show_full_tools)?
                            } else {
                                clamp_scroll(
                                    &terminal,
                                    &input,
                                    &transcript,
                                    show_full_tools,
                                    scroll,
                                )?
                            };
                            status = if show_full_tools {
                                "showing full tool output"
                            } else {
                                "showing truncated tool output"
                            }
                            .into();
                        }
                        (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => {
                            input.push('\n')
                        }
                        (KeyCode::Enter, m) if m.contains(KeyModifiers::CONTROL) => {
                            input.push('\n')
                        }
                        (KeyCode::Enter, _) => {
                            if busy {
                                status = "agent is still running".into();
                            } else if input.trim().is_empty() {
                                input.clear();
                            } else {
                                let msg = input.trim_end().to_string();
                                input.clear();
                                transcript.push(TranscriptBlock {
                                    kind: TranscriptKind::User,
                                    title: "message".into(),
                                    content: msg.clone(),
                                });
                                active_assistant = None;
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
                                scroll =
                                    bottom_scroll(&terminal, &input, &transcript, show_full_tools)?;
                            }
                        }
                        (KeyCode::Backspace, _) => {
                            input.pop();
                        }
                        (KeyCode::PageUp, _) => {
                            scroll = scroll.saturating_sub(10);
                            stick_to_bottom = false;
                        }
                        (KeyCode::PageDown, _) => {
                            let max =
                                bottom_scroll(&terminal, &input, &transcript, show_full_tools)?;
                            scroll = scroll.saturating_add(10).min(max);
                            stick_to_bottom = scroll >= max;
                        }
                        (KeyCode::Char(ch), m)
                            if !m.contains(KeyModifiers::CONTROL)
                                && !m.contains(KeyModifiers::ALT) =>
                        {
                            input.push(ch)
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    let transcript_area =
                        render::transcript_area(terminal_area(&terminal)?, &input);
                    if rect_contains(transcript_area, mouse.column, mouse.row) {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => {
                                scroll = scroll.saturating_sub(5);
                                stick_to_bottom = false;
                            }
                            MouseEventKind::ScrollDown => {
                                let max =
                                    bottom_scroll(&terminal, &input, &transcript, show_full_tools)?;
                                scroll = scroll.saturating_add(5).min(max);
                                stick_to_bottom = scroll >= max;
                            }
                            _ => {}
                        }
                    }
                }
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
) -> Result<u16> {
    let area = render::transcript_area(terminal_area(terminal)?, input);
    Ok(render::max_transcript_scroll(
        transcript,
        show_full_tools,
        area,
    ))
}

fn clamp_scroll(
    terminal: &terminal::CassTerminal,
    input: &str,
    transcript: &[TranscriptBlock],
    show_full_tools: bool,
    scroll: u16,
) -> Result<u16> {
    Ok(scroll.min(bottom_scroll(terminal, input, transcript, show_full_tools)?))
}

fn rect_contains(area: ratatui::layout::Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
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
                tool_calls,
                ..
            } => {
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
                        title: format!("call: {} ({})", call.name, short_call_id(&call.id)),
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
                    "result: {name} {} ({})",
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
