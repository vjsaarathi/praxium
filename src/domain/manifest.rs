//! Core initialization manifest definitions and reports.
//!
//! Clients can pass an [`InitializationManifest`] to atomically seed default
//! collections, schemas, and indexes. Initialization is idempotent and strictly
//! preserves any existing definitions.

use serde_json::Value;

use crate::domain::types::{CollectionName, IndexVersion, JsonPointer, SchemaVersion};
use crate::error::Error;

/// Draft 2020-12 canonical schema identifier.
pub const DRAFT_2020_12_SCHEMA_URI: &str = "https://json-schema.org/draft/2020-12/schema";

/// Client-supplied defaults for initializing collections, schemas, and indexes.
#[derive(Debug, Clone, Default)]
pub struct InitializationManifest {
    pub(crate) collections: Vec<CollectionDefinition>,
}

impl InitializationManifest {
    /// Creates a builder for constructing an [`InitializationManifest`].
    pub fn builder() -> InitializationManifestBuilder {
        InitializationManifestBuilder::default()
    }

    /// Returns a slice of the collection definitions in the manifest.
    pub fn collections(&self) -> &[CollectionDefinition] {
        &self.collections
    }
}

/// Builder for [`InitializationManifest`].
#[derive(Debug, Default)]
pub struct InitializationManifestBuilder {
    collections: Vec<CollectionDefinition>,
}

impl InitializationManifestBuilder {
    /// Adds a collection definition to the manifest.
    pub fn add_collection(mut self, def: CollectionDefinition) -> Self {
        self.collections.push(def);
        self
    }

    /// Finalizes and returns the [`InitializationManifest`].
    pub fn build(self) -> InitializationManifest {
        InitializationManifest {
            collections: self.collections,
        }
    }
}

/// Specification for a collection to seed if missing.
#[derive(Debug, Clone)]
pub struct CollectionDefinition {
    pub(crate) name: CollectionName,
    pub(crate) schema: Option<Value>,
    pub(crate) indexes: Vec<IndexDefinition>,
}

impl CollectionDefinition {
    /// Creates a new collection definition with the given name.
    pub fn new(name: CollectionName) -> Self {
        Self {
            name,
            schema: None,
            indexes: Vec::new(),
        }
    }

    /// Sets the JSON Schema Draft 2020-12 document for the collection.
    pub fn with_schema(mut self, schema: Value) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Adds a single scalar-path index definition to the collection.
    pub fn add_index(mut self, index: IndexDefinition) -> Self {
        self.indexes.push(index);
        self
    }

    /// Collection name.
    pub fn name(&self) -> &CollectionName {
        &self.name
    }

    /// Active schema JSON, if provided.
    pub fn schema(&self) -> Option<&Value> {
        self.schema.as_ref()
    }

    /// Indexes to seed on this collection.
    pub fn indexes(&self) -> &[IndexDefinition] {
        &self.indexes
    }
}

/// Declaration for an index on a JSON Pointer path.
#[derive(Debug, Clone)]
pub struct IndexDefinition {
    pub(crate) pointer: JsonPointer,
}

impl IndexDefinition {
    /// Creates a new index definition targeting the specified JSON Pointer.
    pub fn new(pointer: JsonPointer) -> Self {
        Self { pointer }
    }

    /// Target path pointer for this index.
    pub fn pointer(&self) -> &JsonPointer {
        &self.pointer
    }
}

/// Report returned by [`crate::ObjectStore::initialize`] detailing all seeded and preserved items.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InitializationReport {
    /// Collections that were newly created and seeded.
    pub seeded_collections: Vec<CollectionName>,
    /// Collections that already existed and were preserved intact.
    pub preserved_collections: Vec<CollectionName>,
    /// Schemas that were newly seeded with their initial [`SchemaVersion`].
    pub seeded_schemas: Vec<(CollectionName, SchemaVersion)>,
    /// Schemas that already existed and were preserved with their current [`SchemaVersion`].
    pub preserved_schemas: Vec<(CollectionName, SchemaVersion)>,
    /// Indexes that were newly seeded with their initial [`IndexVersion`].
    pub seeded_indexes: Vec<(CollectionName, JsonPointer, IndexVersion)>,
    /// Indexes that already existed and were preserved with their current [`IndexVersion`].
    pub preserved_indexes: Vec<(CollectionName, JsonPointer, IndexVersion)>,
}

/// Validates that a schema document is a valid Draft 2020-12 JSON Schema.
pub fn validate_draft_2020_12_schema(schema: &Value) -> Result<(), Error> {
    // Stored JSON schema must be an object (or boolean schema, but v1 specifies object schema declarations)
    let schema_obj = schema.as_object().ok_or_else(|| {
        Error::invalid_input("validate_schema", "schema", "schema document must be a JSON object")
    })?;

    // Draft 2020-12 requires declaring $schema matching Draft 2020-12
    let schema_uri = schema_obj.get("$schema").and_then(Value::as_str);
    match schema_uri {
        Some(uri) if uri == DRAFT_2020_12_SCHEMA_URI || uri == format!("{DRAFT_2020_12_SCHEMA_URI}#") => {
            // Valid Draft 2020-12 dialect URI
        }
        Some(other) => {
            return Err(Error::invalid_input(
                "validate_schema",
                "schema",
                format!("unsupported schema dialect '{other}'; only JSON Schema Draft 2020-12 is supported"),
            ));
        }
        None => {
            return Err(Error::invalid_input(
                "validate_schema",
                "schema",
                "schema must declare '$schema': 'https://json-schema.org/draft/2020-12/schema'",
            ));
        }
    }

    // Verify it compiles with jsonschema Draft 2020-12 compiler
    jsonschema::validator_for(schema).map_err(|e| {
        Error::invalid_input("validate_schema", "schema", format!("malformed JSON Schema: {e}"))
    })?;

    Ok(())
}
