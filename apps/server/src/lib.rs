//! ClipForge cloud sharing and streaming server.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod storage;

use std::sync::Arc;

use axum::Router;

use crate::config::ServerConfig;
use crate::db::Database;
use crate::error::ServerError;
use crate::routes::{AppState, build_router};
use crate::storage::build_storage;

/// Initializes database, object storage, and returns a configured Axum [`Router`].
pub async fn build_app(config: ServerConfig) -> Result<Router, ServerError> {
    let db = Database::open(&config.database_path)?;
    if let Some(ref bootstrap_key) = config.bootstrap_api_key {
        db.ensure_bootstrap_api_key(bootstrap_key).await?;
    }
    let storage = build_storage(&config.storage)?;
    let state = AppState {
        config: Arc::new(config),
        db,
        storage,
    };
    Ok(build_router(state))
}
