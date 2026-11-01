//! Core domain newtypes and validation primitives.
//!
//! Provides strongly-typed validated wrappers for collection names, UUIDv7 object IDs,
//! monotonically increasing revisions/versions, and RFC 6901 JSON Pointers.

use std::fmt;
use std::num::{NonZeroU32, NonZeroU64};
use std::str::FromStr;
use uuid::Uuid;

use crate::error::Error;

/// Validated newtype for a Collection name.
///
/// Collection names must be non-empty, contain no ASCII or Unicode control characters,
/// and preserve exact text and case without implicit normalization or folding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CollectionName(String);

impl CollectionName {
    /// Creates and validates a new [`CollectionName`].
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if the name is empty or contains control characters.
    pub fn new(name: impl AsRef<str>) -> Result<Self, Error> {
        let s = name.as_ref();
        if s.is_empty() {
            return Err(Error::invalid_input(
                "CollectionName::new",
                "collection_name",
                "collection name cannot be empty",
            ));
        }
        if s.chars().any(|c| c.is_control()) {
            return Err(Error::invalid_input(
                "CollectionName::new",
                "collection_name",
                "collection name cannot contain control characters",
            ));
        }
        Ok(Self(s.to_string()))
    }

    /// Returns a string slice of the collection name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CollectionName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for CollectionName {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for CollectionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for CollectionName {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for CollectionName {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CollectionName> for String {
    fn from(name: CollectionName) -> Self {
        name.0
    }
}

/// Validated newtype for an object identifier strictly using UUIDv7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ObjectId(Uuid);

impl ObjectId {
    /// Generates a new monotonically time-ordered UUIDv7 object identifier.
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    /// Wraps an existing [`Uuid`], ensuring it is version 7.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if the UUID is not version 7.
    pub fn from_uuid(uuid: Uuid) -> Result<Self, Error> {
        if uuid.get_version_num() != 7 {
            return Err(Error::invalid_input(
                "ObjectId::from_uuid",
                "object_id",
                format!("expected UUIDv7, found version {:?}", uuid.get_version()),
            ));
        }
        Ok(Self(uuid))
    }

    /// Returns the underlying [`Uuid`].
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ObjectId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parsed = Uuid::parse_str(s).map_err(|e| {
            Error::invalid_input("ObjectId::from_str", "object_id", format!("malformed UUID: {e}"))
        })?;
        Self::from_uuid(parsed)
    }
}

impl TryFrom<String> for ObjectId {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl From<ObjectId> for String {
    fn from(id: ObjectId) -> Self {
        id.0.to_string()
    }
}

/// Monotonically increasing revision of a stored object, starting at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct ObjectRevision(NonZeroU64);

impl ObjectRevision {
    /// Initial revision for freshly created objects.
    pub const INITIAL: Self = Self(match NonZeroU64::new(1) {
        Some(v) => v,
        None => unreachable!(),
    });

    /// Creates a new [`ObjectRevision`].
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if revision is zero.
    pub fn new(val: u64) -> Result<Self, Error> {
        NonZeroU64::new(val)
            .map(Self)
            .ok_or_else(|| Error::invalid_input("ObjectRevision::new", "revision", "revision must be >= 1"))
    }

    /// Returns the numerical revision value.
    pub fn get(&self) -> u64 {
        self.0.get()
    }

    /// Computes the subsequent revision.
    pub fn next(&self) -> Self {
        Self(self.0.checked_add(1).expect("revision overflow"))
    }
}

impl fmt::Display for ObjectRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Monotonically increasing version of a collection schema, starting at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct SchemaVersion(NonZeroU32);

impl SchemaVersion {
    /// Initial schema version.
    pub const INITIAL: Self = Self(match NonZeroU32::new(1) {
        Some(v) => v,
        None => unreachable!(),
    });

    /// Creates a new [`SchemaVersion`].
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if version is zero.
    pub fn new(val: u32) -> Result<Self, Error> {
        NonZeroU32::new(val)
            .map(Self)
            .ok_or_else(|| Error::invalid_input("SchemaVersion::new", "schema_version", "schema version must be >= 1"))
    }

    /// Returns the numerical version value.
    pub fn get(&self) -> u32 {
        self.0.get()
    }

    /// Computes the next version.
    pub fn next(&self) -> Self {
        Self(self.0.checked_add(1).expect("schema version overflow"))
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Monotonically increasing version of a collection index, starting at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct IndexVersion(NonZeroU32);

impl IndexVersion {
    /// Initial index version.
    pub const INITIAL: Self = Self(match NonZeroU32::new(1) {
        Some(v) => v,
        None => unreachable!(),
    });

    /// Creates a new [`IndexVersion`].
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if version is zero.
    pub fn new(val: u32) -> Result<Self, Error> {
        NonZeroU32::new(val)
            .map(Self)
            .ok_or_else(|| Error::invalid_input("IndexVersion::new", "index_version", "index version must be >= 1"))
    }

    /// Returns the numerical version value.
    pub fn get(&self) -> u32 {
        self.0.get()
    }

    /// Computes the next version.
    pub fn next(&self) -> Self {
        Self(self.0.checked_add(1).expect("index version overflow"))
    }
}

impl fmt::Display for IndexVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Validated RFC 6901 JSON Pointer.
///
/// An empty string `""` strictly addresses the root value.
/// Non-empty pointers must begin with a forward slash (`/`).
/// Escape sequences must strictly be `~0` (representing `~`) or `~1` (representing `/`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JsonPointer(String);

impl JsonPointer {
    /// Root JSON pointer (empty string `""`).
    pub const ROOT: Self = Self(String::new());

    /// Creates and validates a new [`JsonPointer`] according to RFC 6901.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if the pointer does not conform to RFC 6901.
    pub fn new(pointer: impl AsRef<str>) -> Result<Self, Error> {
        let s = pointer.as_ref();
        if s.is_empty() {
            return Ok(Self(String::new()));
        }

        if !s.starts_with('/') {
            return Err(Error::invalid_input(
                "JsonPointer::new",
                "json_pointer",
                "non-empty JSON Pointer must start with '/'",
            ));
        }

        // Validate escape sequences: ~ must only be followed by '0' or '1'
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '~' {
                match chars.next() {
                    Some('0') | Some('1') => (),
                    Some(other) => {
                        return Err(Error::invalid_input(
                            "JsonPointer::new",
                            "json_pointer",
                            format!("invalid RFC 6901 escape sequence: '~{other}'"),
                        ));
                    }
                    None => {
                        return Err(Error::invalid_input(
                            "JsonPointer::new",
                            "json_pointer",
                            "invalid trailing '~' in JSON Pointer",
                        ));
                    }
                }
            }
        }

        Ok(Self(s.to_string()))
    }

    /// Returns `true` if this pointer addresses the document root (empty string).
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a string slice of the JSON Pointer.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for JsonPointer {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JsonPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for JsonPointer {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for JsonPointer {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<JsonPointer> for String {
    fn from(ptr: JsonPointer) -> Self {
        ptr.0
    }
}
