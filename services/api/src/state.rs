use std::sync::Arc;
use std::time::Instant;

use crate::aggregate::Registry;
use crate::store::MemoryStore;

pub struct AppState {
    pub store: MemoryStore,
    pub env: String,
    pub started: Instant,
    pub providers: Registry,
}

pub type SharedState = Arc<AppState>;
