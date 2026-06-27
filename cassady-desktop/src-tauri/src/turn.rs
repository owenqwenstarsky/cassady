use crate::state::{ApprovalDecision, DesktopState, TurnEntry};
use crate::types::{ApproveArgs, CancelTurnArgs, ConversationInfoDto, StreamEvent, TurnHandle};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::mpsc;

static TURN_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_turn_id() -> String {
    let n = TURN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("turn-{n}")
}

#[tauri::command]
pub async fn start_turn(
    state: State<'_, DesktopState>,
    chat_id: String,
    message: String,
    on_event: Channel<StreamEvent>,
) -> Result<TurnHandle, String> {
    let session = {
        let mut sessions = state
            .sessions
            .lock()
            .map_err(|e| format!("sessions lock: {e}"))?;
        sessions
            .remove(&chat_id)
            .ok_or_else(|| format!("session {} not found", chat_id))?
    };

    let turn = session
        .start_turn(message)
        .await
        .map_err(|e| e.to_string())?;

    let turn_id = next_turn_id();
    let (approval_tx, approval_rx) = mpsc::unbounded_channel::<ApprovalDecision>();
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    state
        .turns
        .lock()
        .map_err(|e| format!("turns lock: {e}"))?
        .insert(
            turn_id.clone(),
            TurnEntry {
                approval_tx,
                cancel_tx,
                chat_id: chat_id.clone(),
            },
        );

    let state_clone = state.inner().clone();
    spawn_turn_worker(
        state_clone,
        turn_id.clone(),
        chat_id.clone(),
        turn,
        on_event,
        approval_rx,
        cancel_rx,
    );

    Ok(TurnHandle { turn_id, chat_id })
}

#[allow(clippy::too_many_arguments)]
fn spawn_turn_worker(
    state: DesktopState,
    turn_id: String,
    chat_id: String,
    turn: cassady::embedding::Turn,
    on_event: Channel<StreamEvent>,
    mut approval_rx: mpsc::UnboundedReceiver<ApprovalDecision>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    tauri::async_runtime::spawn(async move {
        let mut turn: Option<cassady::embedding::Turn> = Some(turn);

        loop {
            let event_result = tokio::select! {
                biased;
                _ = &mut cancel_rx => {
                    // Cancel during streaming.
                    if let Some(t) = turn.take() {
                        match t.cancel().await {
                            Ok(session) => {
                                let info: ConversationInfoDto = session.info().into();
                                if let Ok(mut sessions) = state.sessions.lock() {
                                    sessions.insert(chat_id.clone(), session);
                                }
                                let _ = on_event.send(StreamEvent::Status {
                                    text: "Turn cancelled.".to_string(),
                                });
                                let _ = info;
                            }
                            Err(e) => {
                                let _ = on_event.send(StreamEvent::Error {
                                    message: format!("cancel: {e}"),
                                });
                            }
                        }
                    }
                    cleanup_turn(&state, &turn_id);
                    return;
                }
                event = async {
                    match turn.as_mut() {
                        Some(t) => t.next_event().await,
                        None => Ok(None),
                    }
                } => event,
            };

            match event_result {
                Ok(Some(event)) => {
                    let mapped = StreamEvent::from_embedding(event);
                    let is_finished = matches!(mapped, StreamEvent::Finished);
                    let is_approval = matches!(mapped, StreamEvent::ApprovalRequested { .. });
                    if on_event.send(mapped).is_err() {
                        break;
                    }
                    if is_finished {
                        break;
                    }
                    if is_approval {
                        // The agent is now blocked waiting for a decision.
                        // Wait for the frontend to approve/deny or cancel.
                        tokio::select! {
                            biased;
                            _ = &mut cancel_rx => {
                                if let Some(t) = turn.take() {
                                    match t.cancel().await {
                                        Ok(session) => {
                                            if let Ok(mut sessions) = state.sessions.lock() {
                                                sessions.insert(chat_id.clone(), session);
                                            }
                                            let _ = on_event.send(StreamEvent::Status {
                                                text: "Turn cancelled.".to_string(),
                                            });
                                        }
                                        Err(e) => {
                                            let _ = on_event.send(StreamEvent::Error {
                                                message: format!("cancel: {e}"),
                                            });
                                        }
                                    }
                                }
                                cleanup_turn(&state, &turn_id);
                                return;
                            }
                            decision = approval_rx.recv() => {
                                if let Some(decision) = decision {
                                    if let Some(t) = turn.as_mut() {
                                        let res = if decision.approved {
                                            t.approve(&decision.request_id)
                                        } else {
                                            t.deny(&decision.request_id)
                                        };
                                        if let Err(e) = res {
                                            let _ = on_event.send(StreamEvent::Error {
                                                message: format!("approval: {e}"),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = on_event.send(StreamEvent::Error {
                        message: e.to_string(),
                    });
                    break;
                }
            }
        }

        // Normal completion: finish the turn and re-insert the session.
        if let Some(t) = turn {
            match t.finish().await {
                Ok(session) => {
                    if let Ok(mut sessions) = state.sessions.lock() {
                        sessions.insert(chat_id.clone(), session);
                    }
                }
                Err(e) => {
                    let _ = on_event.send(StreamEvent::Error {
                        message: format!("finish: {e}"),
                    });
                }
            }
        }

        cleanup_turn(&state, &turn_id);
    });
}

fn cleanup_turn(state: &DesktopState, turn_id: &str) {
    if let Ok(mut turns) = state.turns.lock() {
        turns.remove(turn_id);
    }
}

#[tauri::command]
pub async fn approve(state: State<'_, DesktopState>, args: ApproveArgs) -> Result<(), String> {
    let tx = {
        let turns = state.turns.lock().map_err(|e| format!("turns lock: {e}"))?;
        let entry = turns
            .get(&args.turn_id)
            .ok_or_else(|| format!("turn {} not found", args.turn_id))?;
        entry.approval_tx.clone()
    };
    tx.send(ApprovalDecision {
        request_id: args.request_id,
        approved: true,
    })
    .map_err(|_| format!("turn {} worker gone", args.turn_id))?;
    Ok(())
}

#[tauri::command]
pub async fn deny(state: State<'_, DesktopState>, args: ApproveArgs) -> Result<(), String> {
    let tx = {
        let turns = state.turns.lock().map_err(|e| format!("turns lock: {e}"))?;
        let entry = turns
            .get(&args.turn_id)
            .ok_or_else(|| format!("turn {} not found", args.turn_id))?;
        entry.approval_tx.clone()
    };
    tx.send(ApprovalDecision {
        request_id: args.request_id,
        approved: false,
    })
    .map_err(|_| format!("turn {} worker gone", args.turn_id))?;
    Ok(())
}

#[tauri::command]
pub async fn cancel_turn(
    state: State<'_, DesktopState>,
    args: CancelTurnArgs,
) -> Result<ConversationInfoDto, String> {
    let (cancel_tx, chat_id) = {
        let mut turns = state.turns.lock().map_err(|e| format!("turns lock: {e}"))?;
        let entry = turns
            .remove(&args.turn_id)
            .ok_or_else(|| format!("turn {} not found", args.turn_id))?;
        (entry.cancel_tx, entry.chat_id)
    };
    let _ = cancel_tx.send(());

    // Wait for the worker to re-insert the session after cancel completes.
    let info = loop {
        {
            let sessions = state
                .sessions
                .lock()
                .map_err(|e| format!("sessions lock: {e}"))?;
            if let Some(session) = sessions.get(&chat_id) {
                break session.info();
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    Ok(info.into())
}
