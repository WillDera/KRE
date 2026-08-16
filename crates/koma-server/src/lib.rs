//! Koma network service.
//!
//! Language-independent HTTP integration API (AGENTS.md: Integration API,
//! Phase 8). External applications (Python, Go, JavaScript, C#, games, AI
//! assistants) can compile, render, and query scenes without depending on
//! Koma internals.
//!
//! Endpoints:
//! - `POST /compile/book` — EPUB or KIR bytes -> `.koma` package bytes
//! - `POST /render/document` — `.koma` bytes + options -> PNG frame bytes
//! - `POST /session/open` — `.koma` bytes -> `{ session_id, ... }`
//! - `GET /scene/state?session=<id>&chapter=<id>` — scene JSON
//!
//! Sessions are lightweight in-memory handles to uploaded packages. The
//! service is stateless otherwise; rendering is GPU-accelerated when the
//! `gpu` backend is requested and falls back to software.

pub mod error;
pub mod handlers;
pub mod state;

pub use error::ApiError;
pub use handlers::{compile_book, open_session, render_document, scene_state};
pub use state::{ChapterInfo, ServerState, Session};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

/// Build the Koma network service router with the given server state.
pub fn router(state: ServerState) -> Router {
    Router::new()
        .route("/compile/book", post(handlers::compile_book))
        .route("/render/document", post(handlers::render_document))
        .route("/session/open", post(handlers::open_session))
        .route("/scene/state", get(handlers::scene_state))
        .with_state(state)
        .layer(DefaultBodyLimit::disable())
}

/// Run the service until shutdown. Binds `addr`, then serves requests.
pub async fn serve(addr: impl tokio::net::ToSocketAddrs) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let app = router(ServerState::new());
    axum::serve(listener, app).await?;
    Ok(())
}
