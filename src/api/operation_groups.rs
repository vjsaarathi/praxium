//! Focused operation groups exposed by the ObjectStore handle.

use crate::domain::types::{
    CollectionMetadata, CollectionName, ObjectId, ObjectRevision, SchemaSnapshot, SchemaVersion,
    StoredObject,
};
use crate::error::{Error, ValidationReport};
use crate::storage::StorageEngine;
use std::sync::Arc;

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
    pub fn validate_proposed(
        &self,
        collection: &CollectionName,
        schema: &serde_json::Value,
    ) -> Result<(), Error> {
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
    pub(crate) storage: Arc<dyn StorageEngine>,
}

impl ObjectsOp {
    fn validate_internal(
        &self,
        collection: &CollectionName,
        value: &serde_json::Value,
        operation: &'static str,
    ) -> Result<ValidationReport, Error> {
        if !value.is_object() {
            return Err(Error::invalid_input(
                operation,
                "value",
                "stored object root must be a JSON object",
            ));
        }

        if !self.storage.collection_exists(collection)? {
            return Err(Error::not_found(
                operation,
                "collection",
                Some(collection.as_str().to_string()),
            ));
        }

        if let Some(active) = self.storage.get_active_schema(collection)? {
            crate::domain::validation::validate_value_against_schema(&active.schema, value)
        } else {
            Ok(ValidationReport::default())
        }
    }

    fn apply_update(
        &self,
        existing: StoredObject,
        new_value: serde_json::Value,
        expected_revision: ObjectRevision,
        operation: &'static str,
    ) -> Result<StoredObject, Error> {
        if existing.revision != expected_revision {
            return Err(Error::conflict(
                operation,
                "object",
                Some(expected_revision.to_string()),
                Some(existing.revision.to_string()),
                format!(
                    "stale object update: expected revision {}, current revision is {}",
                    expected_revision, existing.revision
                ),
            ));
        }

        let report = self.validate_internal(&existing.collection, &new_value, operation)?;
        if !report.is_valid() {
            return Err(Error::validation(operation, report));
        }

        let next_rev = existing.revision.next();
        let now = std::time::SystemTime::now();
        let updated = StoredObject {
            collection: existing.collection,
            id: existing.id,
            revision: next_rev,
            value: new_value,
            created_at: existing.created_at,
            updated_at: now,
            deleted_at: existing.deleted_at,
        };

        self.storage
            .update_object(&updated, expected_revision, operation)?;
        Ok(updated)
    }

    /// Validates a candidate object value against the collection's active schema without writing.
    ///
    /// # Errors
    /// - Returns [`Error::InvalidInput`] if `value` is not a JSON object root.
    /// - Returns [`Error::NotFound`] if the collection does not exist.
    pub fn validate(
        &self,
        collection: &CollectionName,
        value: &serde_json::Value,
    ) -> Result<ValidationReport, Error> {
        self.validate_internal(collection, value, "validate_object")
    }

    /// Creates a new stored object in the specified collection.
    ///
    /// Assigns a new UUIDv7 [`ObjectId`], initial revision 1, and creation/update timestamps.
    ///
    /// # Errors
    /// - Returns [`Error::NotFound`] if the collection does not exist.
    /// - Returns [`Error::InvalidInput`] if the root value is not a JSON object.
    /// - Returns [`Error::Validation`] if the value violates the collection's active schema.
    pub fn create(
        &self,
        collection: &CollectionName,
        value: serde_json::Value,
    ) -> Result<StoredObject, Error> {
        let report = self.validate_internal(collection, &value, "create_object")?;
        if !report.is_valid() {
            return Err(Error::validation("create_object", report));
        }

        let now = std::time::SystemTime::now();
        let object = StoredObject {
            collection: collection.clone(),
            id: ObjectId::new_v7(),
            revision: ObjectRevision::INITIAL,
            value,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        self.storage.insert_object(&object, "create_object")?;
        Ok(object)
    }

    /// Retrieves a stored object by its collection and ID.
    ///
    /// # Errors
    /// - Returns [`Error::NotFound`] if the collection does not exist.
    pub fn get(
        &self,
        collection: &CollectionName,
        id: &ObjectId,
    ) -> Result<Option<StoredObject>, Error> {
        if !self.storage.collection_exists(collection)? {
            return Err(Error::not_found(
                "get_object",
                "collection",
                Some(collection.as_str().to_string()),
            ));
        }

        self.storage.get_object(collection, id)
    }

    /// Fully replaces an existing stored object guarded by the expected current revision.
    ///
    /// Increments the revision by 1 and updates the update timestamp.
    ///
    /// # Errors
    /// - Returns [`Error::NotFound`] if the collection or object does not exist.
    /// - Returns [`Error::InvalidInput`] if the replacement root value is not a JSON object.
    /// - Returns [`Error::Conflict`] if the current revision does not match `expected_revision`.
    /// - Returns [`Error::Validation`] if the new value violates the collection's active schema.
    pub fn replace(
        &self,
        collection: &CollectionName,
        id: &ObjectId,
        expected_revision: ObjectRevision,
        value: serde_json::Value,
    ) -> Result<StoredObject, Error> {
        if !self.storage.collection_exists(collection)? {
            return Err(Error::not_found(
                "replace_object",
                "collection",
                Some(collection.as_str().to_string()),
            ));
        }

        let existing = self
            .storage
            .get_object(collection, id)?
            .ok_or_else(|| Error::not_found("replace_object", "object", Some(id.to_string())))?;

        self.apply_update(existing, value, expected_revision, "replace_object")
    }

    /// Applies an RFC 6902 JSON Patch to an existing stored object guarded by the expected revision.
    ///
    /// Increments the revision by 1 and updates the update timestamp.
    ///
    /// # Errors
    /// - Returns [`Error::NotFound`] if the collection or object does not exist.
    /// - Returns [`Error::Conflict`] if the current revision does not match `expected_revision`.
    /// - Returns [`Error::InvalidInput`] if the patch is malformed, fails to apply, or results in a non-object.
    /// - Returns [`Error::Validation`] if the patched value violates the collection's active schema.
    pub fn patch(
        &self,
        collection: &CollectionName,
        id: &ObjectId,
        expected_revision: ObjectRevision,
        patch: serde_json::Value,
    ) -> Result<StoredObject, Error> {
        if !self.storage.collection_exists(collection)? {
            return Err(Error::not_found(
                "patch_object",
                "collection",
                Some(collection.as_str().to_string()),
            ));
        }

        let existing = self
            .storage
            .get_object(collection, id)?
            .ok_or_else(|| Error::not_found("patch_object", "object", Some(id.to_string())))?;

        let patch_ops: json_patch::Patch = serde_json::from_value(patch).map_err(|e| {
            Error::invalid_input(
                "patch_object",
                "patch",
                format!("malformed RFC 6902 JSON patch: {e}"),
            )
        })?;

        let mut candidate_value = existing.value.clone();
        json_patch::patch(&mut candidate_value, &patch_ops).map_err(|e| {
            Error::invalid_input(
                "patch_object",
                "patch",
                format!("failed to apply JSON patch: {e}"),
            )
        })?;

        self.apply_update(existing, candidate_value, expected_revision, "patch_object")
    }
}

/// Operations for generic filtering, sorting, and pagination.
#[derive(Clone, Debug)]
pub struct QueryOp {
    pub(crate) storage: Arc<dyn StorageEngine>,
}

impl QueryOp {
    /// Executes a deterministically sorted, paginated query. `limit` must be positive.
    pub fn execute_page(
        &self,
        collection: &CollectionName,
        filter: &crate::domain::query::Filter,
        sort: &[crate::domain::query::SortKey],
        limit: usize,
        offset: usize,
    ) -> Result<crate::domain::query::QueryResult, Error> {
        if limit == 0 {
            return Err(Error::invalid_input(
                "execute_query",
                "limit",
                "limit must be positive",
            ));
        }
        filter.validate(Some(collection))?;
        if !self.storage.collection_exists(collection)? {
            return Err(Error::not_found(
                "execute_query",
                "collection",
                Some(collection.as_str().to_string()),
            ));
        }
        let mut matches = self
            .storage
            .get_all_objects(collection)?
            .into_iter()
            .filter_map(|o| match filter.matches(collection, &o) {
                Ok(true) => Some(Ok(o)),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        matches.sort_by(|a, b| {
            for key in sort {
                let av = a.value.pointer(key.pointer.as_str());
                let bv = b.value.pointer(key.pointer.as_str());
                let mut ord = match (av, bv) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => crate::domain::query::json_sort_cmp(a, b),
                };
                if key.direction == crate::domain::query::SortDirection::Descending {
                    ord = ord.reverse();
                }
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            a.id.cmp(&b.id)
        });
        let total_count = matches.len();
        let objects = matches.into_iter().skip(offset).take(limit).collect();
        Ok(crate::domain::query::QueryResult {
            objects,
            total_count,
        })
    }
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
