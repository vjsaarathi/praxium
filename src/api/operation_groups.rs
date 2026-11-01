//! Focused operation groups exposed by the ObjectStore handle.

use std::sync::Arc;
use crate::domain::types::CollectionName;
use crate::error::Error;
use crate::storage::StorageEngine;

/// Operations for managing collections.
#[derive(Clone, Debug)]
pub struct CollectionsOp {
    pub(crate) storage: Arc<dyn StorageEngine>,
}

impl CollectionsOp {
    /// Lists all existing collection names in ascending order.
    pub fn list(&self) -> Result<Vec<CollectionName>, Error> {
        let mut collections = Vec::new();
        self.storage.run_transaction(&mut |tx| {
            collections = tx.list_collections()?;
            Ok(())
        })?;
        Ok(collections)
    }
}

/// Operations for inspecting and managing versioned collection schemas.
#[derive(Clone, Debug)]
pub struct SchemasOp {
    #[allow(dead_code)]
    pub(crate) storage: Arc<dyn StorageEngine>,
}

impl SchemasOp {
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

        let mut runner = |_tx: &mut dyn crate::storage::StorageTransaction| {
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
