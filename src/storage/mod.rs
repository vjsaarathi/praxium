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
use crate::domain::types::{
    CollectionMetadata, CollectionName, ObjectId, ObjectRevision, SchemaSnapshot, SchemaVersion,
    StoredObject,
};
use crate::error::Error;

/// Storage port defining backend-agnostic persistence capabilities.
///
/// Any storage implementation (SQLite, PostgreSQL, memory, etc.) fulfills this trait.
/// The public [`ObjectStore`](crate::ObjectStore) handle interacts solely through this abstraction,
/// ensuring that Praxium is never coupled to a single backend.
pub trait StorageEngine: Send + Sync + std::fmt::Debug {
    /// Creates a new collection in storage.
    ///
    /// # Errors
    /// Returns [`Error::Conflict`] if a collection with this name already exists.
    fn create_collection(&self, name: &CollectionName) -> Result<(), Error>;

    /// Looks up metadata for an existing named collection.
    /// Returns `Ok(None)` if the collection does not exist.
    fn get_collection(&self, name: &CollectionName) -> Result<Option<CollectionMetadata>, Error>;

    /// Checks whether a collection with the given name exists in storage.
    fn collection_exists(&self, name: &CollectionName) -> Result<bool, Error>;

    /// Lists all existing collection names in ascending order.
    fn list_collections(&self) -> Result<Vec<CollectionName>, Error>;

    /// Retrieves the currently active schema snapshot for the specified collection.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the collection does not exist.
    fn get_active_schema(
        &self,
        collection: &CollectionName,
    ) -> Result<Option<SchemaSnapshot>, Error>;

    /// Retrieves a specific immutable schema version snapshot for the collection.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the collection does not exist.
    fn get_schema_version(
        &self,
        collection: &CollectionName,
        version: SchemaVersion,
    ) -> Result<Option<SchemaSnapshot>, Error>;

    /// Lists all retained schema versions for the collection in ascending order.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the collection does not exist.
    fn list_schema_versions(
        &self,
        collection: &CollectionName,
    ) -> Result<Vec<SchemaVersion>, Error>;

    /// Persists a newly created stored object.
    ///
    /// # Errors
    /// Returns [`Error::Conflict`] if an object with the same ID already exists.
    fn insert_object(&self, object: &StoredObject, operation: &'static str) -> Result<(), Error>;

    /// Retrieves a stored object by its collection and ID.
    /// Returns `Ok(None)` if the object does not exist.
    fn get_object(
        &self,
        collection: &CollectionName,
        id: &ObjectId,
    ) -> Result<Option<StoredObject>, Error>;

    /// Updates an existing stored object guarded by the expected current revision.
    ///
    /// # Errors
    /// - Returns [`Error::NotFound`] if the object does not exist.
    /// - Returns [`Error::Conflict`] if the current revision does not match `expected_revision`.
    fn update_object(
        &self,
        object: &StoredObject,
        expected_revision: ObjectRevision,
        operation: &'static str,
    ) -> Result<(), Error>;

    /// Atomically seeds missing collections, schemas, and indexes from the manifest.
    fn seed_manifest(
        &self,
        manifest: &InitializationManifest,
    ) -> Result<InitializationReport, Error>;

    /// Runs a transactional unit of work atomically.
    ///
    /// Rolls back on [`Err`] and unwinds safely on panic without catching.
    fn run_transaction(&self, f: &mut dyn FnMut() -> Result<(), Error>) -> Result<(), Error>;
}
