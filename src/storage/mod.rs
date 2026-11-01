//! Storage adapter abstractions and implementations.
//!
//! Internal storage details, schema migrations, and SQL queries remain strictly
//! private to backend adapters and are never leaked across the public API seam.
//!
//! The [`StorageEngine`] trait acts as the storage port, completely decoupling the
//! [`ObjectStore`](crate::ObjectStore) handle from any single concrete database backend.

pub mod sqlite;

pub use sqlite::SqliteStorage;

use crate::domain::manifest::{InitializationManifest, InitializationReport};
use crate::domain::types::CollectionName;
use crate::error::Error;

/// Transactional storage operations handle passed to transactional callbacks.
pub trait StorageTransaction {
    /// Lists all existing collection names in ascending order.
    fn list_collections(&mut self) -> Result<Vec<CollectionName>, Error>;
}

/// Storage port defining backend-agnostic persistence capabilities.
///
/// Any storage implementation (SQLite, PostgreSQL, memory, etc.) fulfills this trait.
/// The public [`ObjectStore`](crate::ObjectStore) handle interacts solely through this abstraction,
/// ensuring that Praxium is never coupled to a single backend.
pub trait StorageEngine: Send + Sync + std::fmt::Debug {
    /// Atomically seeds missing collections, schemas, and indexes from the manifest.
    fn seed_manifest(&self, manifest: &InitializationManifest) -> Result<InitializationReport, Error>;

    /// Runs a transactional unit of work atomically.
    ///
    /// The closure receives a [`StorageTransaction`] handle to execute operations
    /// within the atomic transaction.
    /// Rolls back on [`Err`] and unwinds safely on panic without catching.
    fn run_transaction(&self, f: &mut dyn FnMut(&mut dyn StorageTransaction) -> Result<(), Error>) -> Result<(), Error>;
}
