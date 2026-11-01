#![forbid(unsafe_code)]
//! # Praxium
//!
//! A synchronous, schema-governed JSON object store with generic query,
//! revision history, and atomic transaction capabilities.
//!
//! Praxium Core owns persistence, validation, querying, revisions, and transactions,
//! leaving domain interpretations and UI to clients.

pub mod api;
pub mod domain;
pub mod error;
pub mod storage;

pub use api::store::IntoStorageEngine;
pub use api::{ObjectStore, Store};
pub use domain::manifest::{
    CollectionDefinition, IndexDefinition, InitializationManifest, InitializationReport,
};
pub use domain::types::{
    CollectionMetadata, CollectionName, IndexVersion, JsonPointer, ObjectId, ObjectRevision,
    SchemaSnapshot, SchemaVersion, StoredObject,
};
pub use error::{Error, ValidationIssue, ValidationReport};
pub use storage::{SqliteStorage, StorageEngine};

/// Praxium Result type alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
