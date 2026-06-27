use cassady::embedding::Session;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct DesktopState {
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub turns: Arc<Mutex<HashMap<String, TurnEntry>>>,
}

pub struct TurnEntry {
    pub approval_tx: mpsc::UnboundedSender<ApprovalDecision>,
    pub cancel_tx: tokio::sync::oneshot::Sender<()>,
    pub chat_id: String,
}

#[derive(Debug)]
pub struct ApprovalDecision {
    pub request_id: String,
    pub approved: bool,
}

impl DesktopState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            turns: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for DesktopState {
    fn default() -> Self {
        Self::new()
    }
}
