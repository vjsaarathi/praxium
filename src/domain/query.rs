//! Portable query filter AST and evaluator over RFC 6901 JSON Pointers.

use crate::domain::types::{CollectionName, JsonPointer, StoredObject};
use crate::error::Error;
use std::cmp::Ordering;

/// Direction used to order a JSON pointer's value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One priority-ordered JSON pointer sort key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SortKey {
    pub pointer: JsonPointer,
    pub direction: SortDirection,
}

impl SortKey {
    pub fn ascending(pointer: JsonPointer) -> Self {
        Self {
            pointer,
            direction: SortDirection::Ascending,
        }
    }
    pub fn descending(pointer: JsonPointer) -> Self {
        Self {
            pointer,
            direction: SortDirection::Descending,
        }
    }
}

/// A query page and the total number of matching objects before pagination.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryResult {
    pub objects: Vec<StoredObject>,
    pub total_count: usize,
}

/// Compares JSON values according to the portable total order (missing is handled by caller).
pub fn json_sort_cmp(a: &serde_json::Value, b: &serde_json::Value) -> Ordering {
    use serde_json::Value::*;
    let rank = |v: &serde_json::Value| match v {
        Null => 0,
        Bool(_) => 1,
        Number(_) => 2,
        String(_) => 3,
        Array(_) => 4,
        Object(_) => 5,
    };
    let types = rank(a).cmp(&rank(b));
    if types != Ordering::Equal {
        return types;
    }
    match (a, b) {
        (Null, Null) => Ordering::Equal,
        (Bool(a), Bool(b)) => a.cmp(b),
        (Number(a), Number(b)) => number_cmp(a, b),
        (String(a), String(b)) => a.chars().cmp(b.chars()),
        (Array(a), Array(b)) => a
            .iter()
            .map(|x| x)
            .zip(b.iter())
            .map(|(x, y)| json_sort_cmp(x, y))
            .find(|o| *o != Ordering::Equal)
            .unwrap_or_else(|| a.len().cmp(&b.len())),
        (Object(a), Object(b)) => {
            let mut a_entries: Vec<_> = a.iter().collect();
            let mut b_entries: Vec<_> = b.iter().collect();
            a_entries.sort_unstable_by(|(ka, _), (kb, _)| ka.chars().cmp(kb.chars()));
            b_entries.sort_unstable_by(|(ka, _), (kb, _)| ka.chars().cmp(kb.chars()));
            a_entries
                .iter()
                .zip(b_entries.iter())
                .map(|((ka, va), (kb, vb))| {
                    ka.chars()
                        .cmp(kb.chars())
                        .then_with(|| json_sort_cmp(va, vb))
                })
                .find(|o| *o != Ordering::Equal)
                .unwrap_or_else(|| a_entries.len().cmp(&b_entries.len()))
        }
        _ => Ordering::Equal,
    }
}

/// A portable, typed query filter AST.
#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    /// Logical AND of multiple filters. All must match.
    And(Vec<Filter>),
    /// Logical OR of multiple filters. At least one must match.
    Or(Vec<Filter>),
    /// Logical NOT of a filter. Negates the inner filter.
    Not(Box<Filter>),
    /// A single predicate expression.
    Predicate(Predicate),
}

/// Individual predicate expressions operating over JSON Pointers.
#[derive(Clone, Debug, PartialEq)]
pub enum Predicate {
    /// Tests whether a path exists in the JSON document (distinguishes missing from null).
    Exists(JsonPointer),
    /// Recursive structural and mathematical equality.
    Eq(JsonPointer, serde_json::Value),
    /// Recursive structural and mathematical inequality.
    Ne(JsonPointer, serde_json::Value),
    /// Ordered comparison: less than (number/number or string/string only).
    Lt(JsonPointer, serde_json::Value),
    /// Ordered comparison: less than or equal (number/number or string/string only).
    Lte(JsonPointer, serde_json::Value),
    /// Ordered comparison: greater than (number/number or string/string only).
    Gt(JsonPointer, serde_json::Value),
    /// Ordered comparison: greater than or equal (number/number or string/string only).
    Gte(JsonPointer, serde_json::Value),
    /// Case-sensitive exact Unicode substring containment on string paths.
    Contains(JsonPointer, String),
    /// Case-sensitive exact Unicode prefix match on string paths.
    Prefix(JsonPointer, String),
    /// Case-sensitive exact Unicode suffix match on string paths.
    Suffix(JsonPointer, String),
    /// Rust-syntax regex match on string paths.
    Regex(JsonPointer, String),
    /// Exact JSON array membership: true if any candidate matches an array element. Empty candidates set is false.
    Any(JsonPointer, Vec<serde_json::Value>),
    /// Exact JSON array membership: true if all candidates match an array element. Empty candidates set is true.
    All(JsonPointer, Vec<serde_json::Value>),
    /// Object key existence check on object paths.
    HasKey(JsonPointer, String),
}

impl Filter {
    /// Constructs a logical AND filter.
    pub fn and(filters: impl IntoIterator<Item = Filter>) -> Self {
        Self::And(filters.into_iter().collect())
    }

    /// Constructs a logical OR filter.
    pub fn or(filters: impl IntoIterator<Item = Filter>) -> Self {
        Self::Or(filters.into_iter().collect())
    }

    /// Constructs a logical NOT filter.
    #[allow(clippy::should_implement_trait)]
    pub fn not(filter: Filter) -> Self {
        Self::Not(Box::new(filter))
    }

    /// Constructs an `exists` predicate filter.
    pub fn exists(pointer: JsonPointer) -> Self {
        Self::Predicate(Predicate::Exists(pointer))
    }

    /// Constructs an `eq` predicate filter.
    pub fn eq(pointer: JsonPointer, value: impl Into<serde_json::Value>) -> Self {
        Self::Predicate(Predicate::Eq(pointer, value.into()))
    }

    /// Constructs a `ne` predicate filter.
    pub fn ne(pointer: JsonPointer, value: impl Into<serde_json::Value>) -> Self {
        Self::Predicate(Predicate::Ne(pointer, value.into()))
    }

    /// Constructs a `lt` predicate filter.
    pub fn lt(pointer: JsonPointer, value: impl Into<serde_json::Value>) -> Self {
        Self::Predicate(Predicate::Lt(pointer, value.into()))
    }

    /// Constructs a `lte` predicate filter.
    pub fn lte(pointer: JsonPointer, value: impl Into<serde_json::Value>) -> Self {
        Self::Predicate(Predicate::Lte(pointer, value.into()))
    }

    /// Constructs a `gt` predicate filter.
    pub fn gt(pointer: JsonPointer, value: impl Into<serde_json::Value>) -> Self {
        Self::Predicate(Predicate::Gt(pointer, value.into()))
    }

    /// Constructs a `gte` predicate filter.
    pub fn gte(pointer: JsonPointer, value: impl Into<serde_json::Value>) -> Self {
        Self::Predicate(Predicate::Gte(pointer, value.into()))
    }

    /// Constructs a `contains` predicate filter.
    pub fn contains(pointer: JsonPointer, substr: impl Into<String>) -> Self {
        Self::Predicate(Predicate::Contains(pointer, substr.into()))
    }

    /// Constructs a `prefix` predicate filter.
    pub fn prefix(pointer: JsonPointer, prefix: impl Into<String>) -> Self {
        Self::Predicate(Predicate::Prefix(pointer, prefix.into()))
    }

    /// Constructs a `suffix` predicate filter.
    pub fn suffix(pointer: JsonPointer, suffix: impl Into<String>) -> Self {
        Self::Predicate(Predicate::Suffix(pointer, suffix.into()))
    }

    /// Constructs a `regex` predicate filter.
    pub fn regex(pointer: JsonPointer, pattern: impl Into<String>) -> Self {
        Self::Predicate(Predicate::Regex(pointer, pattern.into()))
    }

    /// Constructs an `any` array membership predicate filter over candidate values.
    pub fn any(
        pointer: JsonPointer,
        candidates: impl IntoIterator<Item = impl Into<serde_json::Value>>,
    ) -> Self {
        Self::Predicate(Predicate::Any(
            pointer,
            candidates.into_iter().map(Into::into).collect(),
        ))
    }

    /// Constructs an `all` array membership predicate filter over candidate values.
    pub fn all(
        pointer: JsonPointer,
        candidates: impl IntoIterator<Item = impl Into<serde_json::Value>>,
    ) -> Self {
        Self::Predicate(Predicate::All(
            pointer,
            candidates.into_iter().map(Into::into).collect(),
        ))
    }

    /// Constructs a `has_key` object key existence predicate filter.
    pub fn has_key(pointer: JsonPointer, key: impl Into<String>) -> Self {
        Self::Predicate(Predicate::HasKey(pointer, key.into()))
    }

    /// Validates the filter AST before evaluation (e.g. pre-compiling regexes and checking operands).
    pub fn validate(&self, collection: Option<&CollectionName>) -> Result<(), Error> {
        match self {
            Self::And(filters) | Self::Or(filters) => {
                for f in filters {
                    f.validate(collection)?;
                }
                Ok(())
            }
            Self::Not(f) => f.validate(collection),
            Self::Predicate(p) => match p {
                Predicate::Regex(ptr, pattern) => {
                    regex::Regex::new(pattern).map_err(|e| {
                        Error::query(
                            "validate_query",
                            collection.map(|c| c.as_str().to_string()),
                            None,
                            Some(ptr.to_string()),
                            Some("regex".to_string()),
                            format!("invalid regex pattern '{pattern}': {e}"),
                        )
                    })?;
                    Ok(())
                }
                Predicate::Lt(ptr, val) => validate_ordered_operand(collection, ptr, val, "lt"),
                Predicate::Lte(ptr, val) => validate_ordered_operand(collection, ptr, val, "lte"),
                Predicate::Gt(ptr, val) => validate_ordered_operand(collection, ptr, val, "gt"),
                Predicate::Gte(ptr, val) => validate_ordered_operand(collection, ptr, val, "gte"),
                _ => Ok(()),
            },
        }
    }

    /// Evaluates whether the stored object matches the filter.
    ///
    /// Any encountered incompatible stored value type fails the whole query,
    /// even if another boolean branch could determine the row's result.
    pub fn matches(
        &self,
        collection: &CollectionName,
        object: &StoredObject,
    ) -> Result<bool, Error> {
        match self {
            Self::And(filters) => {
                let mut all_matched = true;
                let mut first_err = None;
                for f in filters {
                    match f.matches(collection, object) {
                        Ok(matched) => {
                            if !matched {
                                all_matched = false;
                            }
                        }
                        Err(e) => {
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                }
                if let Some(err) = first_err {
                    Err(err)
                } else {
                    Ok(all_matched)
                }
            }
            Self::Or(filters) => {
                let mut any_matched = false;
                let mut first_err = None;
                for f in filters {
                    match f.matches(collection, object) {
                        Ok(matched) => {
                            if matched {
                                any_matched = true;
                            }
                        }
                        Err(e) => {
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                }
                if let Some(err) = first_err {
                    Err(err)
                } else {
                    Ok(any_matched)
                }
            }
            Self::Not(f) => {
                let matched = f.matches(collection, object)?;
                Ok(!matched)
            }
            Self::Predicate(p) => p.eval(collection, object),
        }
    }
}

fn validate_ordered_operand(
    collection: Option<&CollectionName>,
    ptr: &JsonPointer,
    val: &serde_json::Value,
    op: &'static str,
) -> Result<(), Error> {
    if !val.is_number() && !val.is_string() {
        return Err(Error::query(
            "validate_query",
            collection.map(|c| c.as_str().to_string()),
            None,
            Some(ptr.to_string()),
            Some(op.to_string()),
            format!(
                "incompatible operand for operator '{op}': ordered comparisons allow number and string only, found {}",
                json_type_name(val)
            ),
        ));
    }
    Ok(())
}

impl Predicate {
    fn eval(&self, collection: &CollectionName, object: &StoredObject) -> Result<bool, Error> {
        match self {
            Self::Exists(ptr) => {
                let exists = object.value.pointer(ptr.as_str()).is_some();
                Ok(exists)
            }
            Self::Eq(ptr, expected) => match object.value.pointer(ptr.as_str()) {
                None => Ok(false), // missing path never matches eq
                Some(actual) => Ok(json_eq(actual, expected)),
            },
            Self::Ne(ptr, expected) => match object.value.pointer(ptr.as_str()) {
                None => Ok(false), // missing path never matches ne
                Some(actual) => Ok(!json_eq(actual, expected)),
            },
            Self::Lt(ptr, expected) => {
                self.eval_cmp(collection, object, ptr, expected, "lt", |c| {
                    c == Ordering::Less
                })
            }
            Self::Lte(ptr, expected) => {
                self.eval_cmp(collection, object, ptr, expected, "lte", |c| {
                    c != Ordering::Greater
                })
            }
            Self::Gt(ptr, expected) => {
                self.eval_cmp(collection, object, ptr, expected, "gt", |c| {
                    c == Ordering::Greater
                })
            }
            Self::Gte(ptr, expected) => {
                self.eval_cmp(collection, object, ptr, expected, "gte", |c| {
                    c != Ordering::Less
                })
            }
            Self::Contains(ptr, substr) => {
                eval_str_pred(collection, object, ptr, "contains", |s| {
                    Ok(s.contains(substr))
                })
            }
            Self::Prefix(ptr, prefix) => eval_str_pred(collection, object, ptr, "prefix", |s| {
                Ok(s.starts_with(prefix))
            }),
            Self::Suffix(ptr, suffix) => eval_str_pred(collection, object, ptr, "suffix", |s| {
                Ok(s.ends_with(suffix))
            }),
            Self::Regex(ptr, pattern) => eval_str_pred(collection, object, ptr, "regex", |s| {
                let re = regex::Regex::new(pattern).map_err(|e| {
                    Error::query(
                        "evaluate_query",
                        Some(collection.to_string()),
                        Some(object.id.to_string()),
                        Some(ptr.to_string()),
                        Some("regex".to_string()),
                        format!("invalid regex pattern '{pattern}': {e}"),
                    )
                })?;
                Ok(re.is_match(s))
            }),
            Self::Any(ptr, candidates) => match object.value.pointer(ptr.as_str()) {
                None => Ok(false),
                Some(serde_json::Value::Array(items)) => {
                    if candidates.is_empty() {
                        Ok(false) // Empty any candidate set is false
                    } else {
                        Ok(candidates
                            .iter()
                            .any(|c| items.iter().any(|item| json_eq(item, c))))
                    }
                }
                Some(other) => Err(Error::query(
                    "evaluate_query",
                    Some(collection.to_string()),
                    Some(object.id.to_string()),
                    Some(ptr.to_string()),
                    Some("any".to_string()),
                    format!(
                        "incompatible type for operator 'any': expected array, found {}",
                        json_type_name(other)
                    ),
                )),
            },
            Self::All(ptr, candidates) => match object.value.pointer(ptr.as_str()) {
                None => Ok(false),
                Some(serde_json::Value::Array(items)) => {
                    if candidates.is_empty() {
                        Ok(true) // Empty all candidate set is true
                    } else {
                        Ok(candidates
                            .iter()
                            .all(|c| items.iter().any(|item| json_eq(item, c))))
                    }
                }
                Some(other) => Err(Error::query(
                    "evaluate_query",
                    Some(collection.to_string()),
                    Some(object.id.to_string()),
                    Some(ptr.to_string()),
                    Some("all".to_string()),
                    format!(
                        "incompatible type for operator 'all': expected array, found {}",
                        json_type_name(other)
                    ),
                )),
            },
            Self::HasKey(ptr, key) => match object.value.pointer(ptr.as_str()) {
                None => Ok(false),
                Some(serde_json::Value::Object(map)) => Ok(map.contains_key(key)),
                Some(other) => Err(Error::query(
                    "evaluate_query",
                    Some(collection.to_string()),
                    Some(object.id.to_string()),
                    Some(ptr.to_string()),
                    Some("has_key".to_string()),
                    format!(
                        "incompatible type for operator 'has_key': expected object, found {}",
                        json_type_name(other)
                    ),
                )),
            },
        }
    }

    fn eval_cmp(
        &self,
        collection: &CollectionName,
        object: &StoredObject,
        ptr: &JsonPointer,
        expected: &serde_json::Value,
        op_name: &'static str,
        matches_ordering: impl Fn(Ordering) -> bool,
    ) -> Result<bool, Error> {
        let actual = match object.value.pointer(ptr.as_str()) {
            None => return Ok(false),
            Some(v) => v,
        };

        match (actual, expected) {
            (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                let ord = number_cmp(a, b);
                Ok(matches_ordering(ord))
            }
            (serde_json::Value::String(a), serde_json::Value::String(b)) => {
                let ord = a.cmp(b);
                Ok(matches_ordering(ord))
            }
            _ => Err(Error::query(
                "evaluate_query",
                Some(collection.to_string()),
                Some(object.id.to_string()),
                Some(ptr.to_string()),
                Some(op_name.to_string()),
                format!(
                    "incompatible types for operator '{op_name}': cannot compare {} and {}",
                    json_type_name(actual),
                    json_type_name(expected)
                ),
            )),
        }
    }
}

fn eval_str_pred(
    collection: &CollectionName,
    object: &StoredObject,
    ptr: &JsonPointer,
    op: &'static str,
    check: impl FnOnce(&str) -> Result<bool, Error>,
) -> Result<bool, Error> {
    match object.value.pointer(ptr.as_str()) {
        None => Ok(false),
        Some(serde_json::Value::String(s)) => check(s),
        Some(other) => Err(Error::query(
            "evaluate_query",
            Some(collection.to_string()),
            Some(object.id.to_string()),
            Some(ptr.to_string()),
            Some(op.to_string()),
            format!(
                "incompatible type for operator '{op}': expected string, found {}",
                json_type_name(other)
            ),
        )),
    }
}

fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Recursive structural equality ignoring object key order and using mathematical numeric equality.
pub fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Null, serde_json::Value::Null) => true,
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a == b,
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => number_eq(a, b),
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter().zip(b.iter()).all(|(x, y)| json_eq(x, y))
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            if a.len() != b.len() {
                return false;
            }
            for (k, v_a) in a {
                match b.get(k) {
                    Some(v_b) => {
                        if !json_eq(v_a, v_b) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        }
        _ => false,
    }
}

/// Mathematical equality for numbers (e.g. 1 == 1.0).
pub fn number_eq(a: &serde_json::Number, b: &serde_json::Number) -> bool {
    if let (Some(ai), Some(bi)) = (a.as_i64(), b.as_i64()) {
        return ai == bi;
    }
    if let (Some(au), Some(bu)) = (a.as_u64(), b.as_u64()) {
        return au == bu;
    }
    if let (Some(ai), Some(bu)) = (a.as_i64(), b.as_u64()) {
        if ai < 0 {
            return false;
        }
        return (ai as u64) == bu;
    }
    if let (Some(au), Some(bi)) = (a.as_u64(), b.as_i64()) {
        if bi < 0 {
            return false;
        }
        return au == (bi as u64);
    }
    if let (Some(af), Some(bf)) = (a.as_f64(), b.as_f64()) {
        return af == bf;
    }
    false
}

/// Mathematical ordering for numbers.
pub fn number_cmp(a: &serde_json::Number, b: &serde_json::Number) -> Ordering {
    if let (Some(ai), Some(bi)) = (a.as_i64(), b.as_i64()) {
        return ai.cmp(&bi);
    }
    if let (Some(au), Some(bu)) = (a.as_u64(), b.as_u64()) {
        return au.cmp(&bu);
    }
    if let (Some(ai), Some(bu)) = (a.as_i64(), b.as_u64()) {
        if ai < 0 {
            return Ordering::Less;
        }
        return (ai as u64).cmp(&bu);
    }
    if let (Some(au), Some(bi)) = (a.as_u64(), b.as_i64()) {
        if bi < 0 {
            return Ordering::Greater;
        }
        return au.cmp(&(bi as u64));
    }
    if let (Some(af), Some(bf)) = (a.as_f64(), b.as_f64()) {
        return af.partial_cmp(&bf).unwrap_or(Ordering::Equal);
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_number_eq_and_cmp() {
        let n_int1 = serde_json::Number::from(1);
        let n_float1 = serde_json::Number::from_f64(1.0).unwrap();
        let n_int2 = serde_json::Number::from(2);

        assert!(number_eq(&n_int1, &n_float1));
        assert_eq!(number_cmp(&n_int1, &n_float1), Ordering::Equal);

        assert!(!number_eq(&n_int1, &n_int2));
        assert_eq!(number_cmp(&n_int1, &n_int2), Ordering::Less);
        assert_eq!(number_cmp(&n_int2, &n_int1), Ordering::Greater);
    }

    #[test]
    fn test_json_eq_recursive() {
        let a = json!({"a": [1, {"b": 2.0}], "c": "hello"});
        let b = json!({"c": "hello", "a": [1.0, {"b": 2}]});
        assert!(json_eq(&a, &b));

        let c = json!({"c": "hello", "a": [2, {"b": 1}]});
        assert!(!json_eq(&a, &c));
    }

    #[test]
    fn test_filter_not_negation() {
        let col = CollectionName::new("col").unwrap();
        let obj = StoredObject {
            collection: col.clone(),
            id: crate::domain::types::ObjectId::new_v7(),
            revision: crate::domain::types::ObjectRevision::INITIAL,
            value: json!({"num": 5}),
            created_at: std::time::SystemTime::now(),
            updated_at: std::time::SystemTime::now(),
            deleted_at: None,
        };

        let f = Filter::not(Filter::gt(JsonPointer::new("/num").unwrap(), 10));
        assert!(f.matches(&col, &obj).unwrap());

        let f2 = Filter::not(Filter::lt(JsonPointer::new("/num").unwrap(), 10));
        assert!(!f2.matches(&col, &obj).unwrap());
    }
}
