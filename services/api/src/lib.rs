//! Ultimate Finance API library: domain models, store facade, auth, aggregation,
//! routes. Exposed as a lib so integration tests drive the exact production router.

pub mod aggregate;
pub mod auth;
pub mod error;
pub mod models;
pub mod routes;
pub mod state;
pub mod store;
