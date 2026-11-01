//! Praxium error types and structured diagnostic contracts.
//!
//! All public operations in Praxium return [`Result<T, Error>`]. Expected failures
//! (e.g. invalid arguments, missing entities, validation failures, version conflicts)
//! never panic and are expressed via typed variants with stable machine-readable fields.

use std::fmt;

/// The primary error type for Praxium operations.
///
/// This enum is marked `#[non_exhaustive]` to allow future failure modes
/// while maintaining backward compatibility. Variants and their structured fields
/// form the stable machine contract, whereas the human-readable [`fmt::Display`]
/// representations may evolve over time.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An input parameter failed validation or violated an invariant.
    #[error("invalid input for operation '{operation}' on field '{field}': {message}")]
    InvalidInput {
        /// The operation during which the invalid input was provided.
        operation: &'static str,
        /// The specific field or argument that was invalid.
        field: &'static str,
        /// Human-readable explanation of the validation failure.
        message: String,
    },

    /// The requested entity (collection, schema, object, or index) was not found.
    #[error("entity '{entity}' not found in operation '{operation}' (id: {id:?})")]
    NotFound {
        /// The operation being performed.
        operation: &'static str,
        /// The kind of entity that was missing (e.g., "collection", "object").
        entity: &'static str,
        /// Optional identifier of the missing entity.
        id: Option<String>,
    },

    /// A conflict occurred due to mismatched revisions or versions.
    #[error(
        "concurrency or state conflict on '{entity}' during '{operation}': {message} (expected: {expected:?}, current: {current:?})"
    )]
    Conflict {
        /// The operation being performed.
        operation: &'static str,
        /// The entity on which the conflict arose.
        entity: &'static str,
        /// Expected revision or version value, if applicable.
        expected: Option<String>,
        /// Current observed revision or version value, if known.
        current: Option<String>,
        /// Human-readable description of the conflict.
        message: String,
    },

    /// A JSON value failed validation against the collection schema.
    #[error("validation failed during '{operation}': {report}")]
    Validation {
        /// The operation attempting to validate or persist the value.
        operation: &'static str,
        /// The structured, deterministically ordered validation issues.
        report: ValidationReport,
    },

    /// A query operation failed due to incompatible types, malformed operators, or invalid regex.
    #[error("query error during '{operation}': {details}")]
    Query {
        /// The operation executing the query.
        operation: &'static str,
        /// Detailed query diagnostic information.
        details: Box<QueryErrorDetails>,
    },

    /// An internal storage failure occurred without leaking backend-specific types.
    #[error("storage failure during '{operation}': {message}")]
    Storage {
        /// The operation encountering the storage error.
        operation: &'static str,
        /// Description of the storage failure.
        message: String,
        /// The underlying standard error cause, if any. Backend types stay encapsulated.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// A multi-operation transaction failed and was rolled back.
    #[error("transaction failure during '{operation}': {message}")]
    Transaction {
        /// The operation during which the transaction failed.
        operation: &'static str,
        /// Description of the failure.
        message: String,
        /// The underlying standard error cause, if any.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// An explicit schema or storage migration failed.
    #[error("migration failure during '{operation}': {message}")]
    Migration {
        /// The migration operation that failed.
        operation: &'static str,
        /// Description of the migration failure.
        message: String,
        /// The client-supplied or internal migration error cause.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// The database storage version is newer than the highest version supported by this Praxium release.
    #[error(
        "unsupported database version: found {found_version}, supported up to {supported_version}"
    )]
    UnsupportedVersion {
        /// The storage version found in the database.
        found_version: u32,
        /// The maximum storage version supported by this library.
        supported_version: u32,
    },
}

impl Error {
    /// Helper to construct an [`Error::InvalidInput`].
    pub fn invalid_input(
        operation: &'static str,
        field: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::InvalidInput {
            operation,
            field,
            message: message.into(),
        }
    }

    /// Helper to construct an [`Error::Storage`].
    pub fn storage(
        operation: &'static str,
        message: impl Into<String>,
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    ) -> Self {
        Self::Storage {
            operation,
            message: message.into(),
            source,
        }
    }

    /// Helper to construct an [`Error::Transaction`].
    pub fn transaction(
        operation: &'static str,
        message: impl Into<String>,
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    ) -> Self {
        Self::Transaction {
            operation,
            message: message.into(),
            source,
        }
    }

    /// Helper to construct an [`Error::NotFound`].
    pub fn not_found(operation: &'static str, entity: &'static str, id: Option<String>) -> Self {
        Self::NotFound {
            operation,
            entity,
            id,
        }
    }

    /// Helper to construct an [`Error::Conflict`].
    pub fn conflict(
        operation: &'static str,
        entity: &'static str,
        expected: Option<String>,
        current: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Conflict {
            operation,
            entity,
            expected,
            current,
            message: message.into(),
        }
    }

    /// Helper to construct an [`Error::Validation`].
    pub fn validation(operation: &'static str, report: ValidationReport) -> Self {
        Self::Validation { operation, report }
    }

    /// Helper to construct an [`Error::Query`].
    pub fn query(
        operation: &'static str,
        collection: Option<String>,
        object_id: Option<String>,
        pointer: Option<String>,
        operator: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Query {
            operation,
            details: Box::new(QueryErrorDetails {
                collection,
                object_id,
                pointer,
                operator,
                message: message.into(),
            }),
        }
    }
}

/// A structured report of schema validation issues.
///
/// Issues are guaranteed to be sorted deterministically in order of:
/// 1. `instance_pointer`
/// 2. `schema_pointer`
/// 3. `keyword`
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct ValidationReport {
    issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Creates a new validation report from a list of issues, sorting them deterministically.
    pub fn new(mut issues: Vec<ValidationIssue>) -> Self {
        issues.sort();
        Self { issues }
    }

    /// Returns a slice of the sorted validation issues.
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    /// Returns `true` if there are no validation issues.
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.issues.is_empty() {
            write!(f, "valid")
        } else {
            write!(f, "{} validation issue(s)", self.issues.len())
        }
    }
}

/// A single validation issue identifying where and why schema validation failed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ValidationIssue {
    /// RFC 6901 JSON Pointer to the location in the candidate instance that failed validation.
    pub instance_pointer: String,
    /// RFC 6901 JSON Pointer to the schema keyword or definition that rejected the instance.
    pub schema_pointer: String,
    /// The validation keyword that failed (e.g. `type`, `required`, `minimum`).
    pub keyword: String,
    /// Optional human-readable description of the validation issue.
    pub message: Option<String>,
}

impl ValidationIssue {
    /// Creates a new validation issue.
    pub fn new(
        instance_pointer: impl Into<String>,
        schema_pointer: impl Into<String>,
        keyword: impl Into<String>,
        message: Option<String>,
    ) -> Self {
        Self {
            instance_pointer: instance_pointer.into(),
            schema_pointer: schema_pointer.into(),
            keyword: keyword.into(),
            message,
        }
    }
}

/// Structured diagnostic details for a query error.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueryErrorDetails {
    /// The collection being queried, if known.
    pub collection: Option<String>,
    /// The object ID that triggered the error, if applicable.
    pub object_id: Option<String>,
    /// The JSON Pointer path evaluated, if applicable.
    pub pointer: Option<String>,
    /// The query operator involved (e.g. `gt`, `contains`), if applicable.
    pub operator: Option<String>,
    /// Descriptive explanation of the error.
    pub message: String,
}

impl fmt::Display for QueryErrorDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(op) = &self.operator {
            write!(f, " (operator: {op})")?;
        }
        if let Some(ptr) = &self.pointer {
            write!(f, " at path '{ptr}'")?;
        }
        Ok(())
    }
}
