//! koma-runtime: the runtime host layer.
//!
//! Responsibilities (AGENTS.md System Boundaries):
//! - open `.koma` packages
//! - manage reading sessions
//! - lazy chapter windows (current + nearby)
//! - user state persistence (separate from packages)
//!
//! No platform UI. Applications talk to the runtime through its API (and later
//! through NRP / IPC).

pub mod error;
pub mod session;
pub mod user_state;
pub mod window;

pub use error::RuntimeError;
pub use session::Session;
pub use user_state::{Bookmark, ReadingPosition, StoredMark, UserState};
pub use window::ChapterWindow;
