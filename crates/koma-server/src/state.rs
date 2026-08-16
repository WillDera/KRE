//! Server-side session state.
//!
//! A session is a lightweight in-memory handle to an uploaded `.koma` package.
//! The package bytes are kept (a `KomaPackage` is not `Clone` and holds a zip
//! handle, so the service re-opens from bytes per request). Sessions enable
//! `GET /scene/state` and future runtime features (reading position, sync).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

/// Chapter descriptor returned when opening a session.
#[derive(Debug, Clone, Serialize)]
pub struct ChapterInfo {
    pub id: String,
    pub title: Option<String>,
}

/// An open session: uploaded package bytes plus its manifest summary.
#[derive(Debug, Clone)]
pub struct Session {
    pub bytes: Vec<u8>,
    pub title: Option<String>,
    pub chapters: Vec<ChapterInfo>,
}

/// Shared server state: session store + id counter.
#[derive(Clone, Default)]
pub struct ServerState {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    next_id: Arc<AtomicU64>,
}

impl ServerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a session, returning its id.
    pub fn insert(&self, session: Session) -> String {
        let id = format!("koma-{:08x}", self.next_id.fetch_add(1, Ordering::Relaxed));
        self.sessions
            .lock()
            .expect("session lock")
            .insert(id.clone(), session);
        id
    }

    /// Fetch a session's bytes by id.
    pub fn get(&self, id: &str) -> Option<Vec<u8>> {
        self.sessions
            .lock()
            .expect("session lock")
            .get(id)
            .map(|s| s.bytes.clone())
    }
}
