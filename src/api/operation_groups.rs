//! Focused operation groups exposed by the ObjectStore handle.

use std::sync::Arc;
use crate::domain::types::{CollectionMetadata, CollectionName, SchemaSnapshot, SchemaVersion};
use crate::error::Error;
use crate::storage::StorageEngine;

/// Operations for managing collections.
#[derive(Clone, Debug)]
pub struct CollectionsOp {
    pub(crate) storage: Arc<dyn StorageEngine>,
}

impl CollectionsOp {
    /// Creates a new named collection.
    ///
    /// # Errors
    /// Returns [`Error::Conflict`] if a collection with this name already exists.
    pub fn create(&self, name: &CollectionName) -> Result<(), Error> {
        self.storage.create_collection(name)
    }

    /// Looks up metadata for an existing named collection.
    /// Returns `Ok(None)` if no collection with the specified name exists.
    pub fn get(&self, name: &CollectionName) -> Result<Option<CollectionMetadata>, Error> {
        self.storage.get_collection(name)
    }

    /// Checks if a collection with the given name exists.
    pub fn exists(&self, name: &CollectionName) -> Result<bool, Error> {
        self.storage.collection_exists(name)
    }

    /// Lists all existing collection names in ascending order.
    pub fn list(&self) -> Result<Vec<CollectionName>, Error> {
        self.storage.list_collections()
    }
}

/// Operations for inspecting and managing versioned collection schemas.
#[derive(Clone, Debug)]
pub struct SchemasOp {
    #[allow(dead_code)]
    pub(crate) storage: Arc<dyn StorageEngine>,
}

impl SchemasOp {
    /// Retrieves the active schema for the specified collection.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the collection does not exist.
    pub fn get_active(&self, collection: &CollectionName) -> Result<Option<SchemaSnapshot>, Error> {
        self.storage.get_active_schema(collection)
    }

    /// Retrieves a specific retained schema version for the specified collection.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the collection does not exist.
    pub fn get_version(
        &self,
        collection: &CollectionName,
        version: SchemaVersion,
    ) -> Result<Option<SchemaSnapshot>, Error> {
        self.storage.get_schema_version(collection, version)
    }

    /// Lists all retained schema versions for the specified collection in ascending order.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the collection does not exist.
    pub fn list_versions(&self, collection: &CollectionName) -> Result<Vec<SchemaVersion>, Error> {
        self.storage.list_schema_versions(collection)
    }

    /// Validates a proposed collection schema against the Draft 2020-12 specification
    /// and checks that the targeted collection exists.
    ///
    /// # Errors
    /// - Returns [`Error::NotFound`] if the collection does not exist.
    /// - Returns [`Error::InvalidInput`] if the schema dialect is unsupported or invalid.
    pub fn validate_proposed(&self, collection: &CollectionName, schema: &serde_json::Value) -> Result<(), Error> {
        if !self.storage.collection_exists(collection)? {
            return Err(Error::not_found(
                "validate_proposed",
                "collection",
                Some(collection.as_str().to_string()),
            ));
        }
        self.validate(schema)
    }

    /// Validates a proposed collection schema against the Draft 2020-12 specification
    /// without writing it to storage.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if the schema dialect is unsupported or invalid.
    pub fn validate(&self, schema: &serde_json::Value) -> Result<(), Error> {
        crate::domain::manifest::validate_draft_2020_12_schema(schema)
    }
}

/// Operations for reading, creating, and updating stored objects.
#[derive(Clone, Debug)]
pub struct ObjectsOp {
    #[allow(dead_code)]
    pub(crate) storage: Arc<dyn StorageEngine>,
}

/// Operations for generic filtering, sorting, and pagination.
#[derive(Clone, Debug)]
pub struct QueryOp {
    #[allow(dead_code)]
    pub(crate) storage: Arc<dyn StorageEngine>,
}

/// Operations for reading change history entries and revisions.
#[derive(Clone, Debug)]
pub struct HistoryOp {
    #[allow(dead_code)]
    pub(crate) storage: Arc<dyn StorageEngine>,
}

/// Operations for executing multi-operation atomic transactions.
#[derive(Clone, Debug)]
pub struct TransactionsOp {
    pub(crate) storage: Arc<dyn StorageEngine>,
}

impl TransactionsOp {
    /// Runs a transactional unit of work atomically.
    ///
    /// If the closure returns `Err(error)`, all changes made within the transaction
    /// are rolled back and the error is returned.
    /// If the closure panics, the transaction rolls back during stack unwinding
    /// and the panic propagates unchanged.
    pub fn run<T, F>(&self, f: F) -> Result<T, Error>
    where
        F: FnOnce() -> Result<T, Error>,
    {
        let mut opt_f = Some(f);
        let mut result = None;

        let mut runner = || {
            let func = opt_f.take().expect("transaction closure invoked once");
            match func() {
                Ok(val) => {
                    result = Some(val);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        };

        self.storage.run_transaction(&mut runner)?;
        Ok(result.expect("transaction committed but returned no value"))
    }
}
