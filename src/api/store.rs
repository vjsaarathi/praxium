//! Main ObjectStore entry point and handle.

use std::sync::Arc;

use crate::api::operation_groups::{
    CollectionsOp, HistoryOp, ObjectsOp, QueryOp, SchemasOp, TransactionsOp,
};
use crate::domain::manifest::{
    validate_draft_2020_12_schema, InitializationManifest, InitializationReport,
};
use crate::error::Error;
use crate::storage::StorageEngine;

/// Helper trait to accept any concrete or shared [`StorageEngine`] into [`ObjectStore::new`].
pub trait IntoStorageEngine {
    /// Converts `self` into a shared [`StorageEngine`] trait object.
    fn into_storage_engine(self) -> Arc<dyn StorageEngine>;
}

impl<S: StorageEngine + 'static> IntoStorageEngine for S {
    fn into_storage_engine(self) -> Arc<dyn StorageEngine> {
        Arc::new(self)
    }
}

impl IntoStorageEngine for Arc<dyn StorageEngine> {
    fn into_storage_engine(self) -> Arc<dyn StorageEngine> {
        self
    }
}

/// Main entry point to a Praxium schema-governed JSON object store.
///
/// An `ObjectStore` instance is synchronous, cloneable, and safe to share across threads.
/// It wraps an abstract [`StorageEngine`] port, completely decoupling the public store
/// handle and operation groups from any single storage backend.
#[derive(Clone, Debug)]
pub struct ObjectStore {
    storage: Arc<dyn StorageEngine>,
}

/// Convenient type alias for [`ObjectStore`].
pub type Store = ObjectStore;

impl ObjectStore {
    /// Constructs an [`ObjectStore`] backed by any storage backend chosen by the caller.
    ///
    /// Accepts either a concrete [`StorageEngine`] implementation or a pre-shared
    /// `Arc<dyn StorageEngine>`.
    ///
    /// # Examples
    /// ```no_run
    /// use praxium::{ObjectStore, SqliteStorage};
    ///
    /// let storage = SqliteStorage::open("praxium.db").unwrap();
    /// let store = ObjectStore::new(storage);
    /// ```
    pub fn new(backend: impl IntoStorageEngine) -> Self {
        Self {
            storage: backend.into_storage_engine(),
        }
    }

    /// Atomically seeds missing collections, schemas, and indexes from the provided manifest.
    ///
    /// Merely delegates the seeding operation to the underlying [`StorageEngine`].
    /// Preserves any definitions that already exist in the store without modifying them.
    ///
    /// # Errors
    /// - Returns [`Error::InvalidInput`] if a schema has an unsupported dialect or is malformed.
    /// - Returns [`Error::Storage`] if an underlying storage error occurs.
    ///
    /// In the event of any error, the initialization is rolled back completely (all-or-nothing).
    pub fn initialize(&self, manifest: &InitializationManifest) -> Result<InitializationReport, Error> {
        // 1. Domain pre-validation (Draft 2020-12 schema validation)
        for col in manifest.collections() {
            if let Some(schema) = col.schema() {
                validate_draft_2020_12_schema(schema)?;
            }
        }

        // 2. Delegate seed operation directly through the StorageEngine trait
        self.storage.seed_manifest(manifest)
    }

    /// Access the operations group for collection definitions.
    pub fn collections(&self) -> CollectionsOp {
        CollectionsOp {
            storage: Arc::clone(&self.storage),
        }
    }

    /// Access the operations group for schemas.
    pub fn schemas(&self) -> SchemasOp {
        SchemasOp {
            storage: Arc::clone(&self.storage),
        }
    }

    /// Access the operations group for stored objects.
    pub fn objects(&self) -> ObjectsOp {
        ObjectsOp {
            storage: Arc::clone(&self.storage),
        }
    }

    /// Access the operations group for queries.
    pub fn query(&self) -> QueryOp {
        QueryOp {
            storage: Arc::clone(&self.storage),
        }
    }

    /// Access the operations group for change history.
    pub fn history(&self) -> HistoryOp {
        HistoryOp {
            storage: Arc::clone(&self.storage),
        }
    }

    /// Access the operations group for multi-operation transactions.
    pub fn transactions(&self) -> TransactionsOp {
        TransactionsOp {
            storage: Arc::clone(&self.storage),
        }
    }

    /// Internal access to storage engine adapter.
    #[allow(dead_code)]
    pub(crate) fn storage(&self) -> &Arc<dyn StorageEngine> {
        &self.storage
    }
}
