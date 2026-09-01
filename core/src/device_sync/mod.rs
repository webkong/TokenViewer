//! Encrypted, local-first synchronization of portable Skills state.
//!
//! Phase 1 deliberately exposes only the protocol foundation and a local
//! object store. Cloud transports and the production Settings surface remain
//! gated until their respective phase gates pass.

pub mod archive;
pub mod config;
pub mod crypto;
pub mod engine;
pub mod models;
pub mod skill_env;
pub mod snapshot;
pub mod store;

pub use crypto::create_vault_metadata;
pub use engine::DeviceSyncEngine;
pub use models::{
    DeviceSyncConfig, DeviceSyncError, DeviceSyncErrorCode, DeviceSyncState, PROTOCOL_VERSION,
};
