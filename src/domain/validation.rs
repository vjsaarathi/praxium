//! Schema validation logic against JSON Schema specifications.

use serde_json::Value;

use crate::error::{Error, ValidationIssue, ValidationReport};

/// Draft 2020-12 canonical schema identifier.
pub const DRAFT_2020_12_SCHEMA_URI: &str = "https://json-schema.org/draft/2020-12/schema";

/// Validates that a schema conforms to the JSON Schema Draft 2020-12 specification.
///
/// Ensures `$schema` is set to `"https://json-schema.org/draft/2020-12/schema"`
/// and that the schema compiles successfully with the Draft 2020-12 validator.
pub fn validate_draft_2020_12_schema(schema: &Value) -> Result<(), Error> {
    let schema_obj = schema.as_object().ok_or_else(|| {
        Error::invalid_input(
            "validate_schema",
            "schema",
            "schema document must be a JSON object",
        )
    })?;

    let schema_uri = schema_obj.get("$schema").and_then(Value::as_str);
    match schema_uri {
        Some(uri)
            if uri == DRAFT_2020_12_SCHEMA_URI || uri == format!("{DRAFT_2020_12_SCHEMA_URI}#") => {
        }
        Some(other) => {
            return Err(Error::invalid_input(
                "validate_schema",
                "schema",
                format!(
                    "unsupported schema dialect '{other}'; only JSON Schema Draft 2020-12 is supported"
                ),
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
        Error::invalid_input(
            "validate_schema",
            "schema",
            format!("malformed JSON Schema: {e}"),
        )
    })?;

    Ok(())
}

/// Validates a candidate JSON value against a compiled JSON Schema.
///
/// Returns a [`ValidationReport`] with issues ordered deterministically by
/// `instance_pointer`, then `schema_pointer`, then `keyword`.
pub fn validate_value_against_schema(
    schema: &Value,
    value: &Value,
) -> Result<ValidationReport, Error> {
    let validator = jsonschema::validator_for(schema).map_err(|e| {
        Error::invalid_input(
            "validate_value",
            "schema",
            format!("malformed JSON Schema: {e}"),
        )
    })?;

    let mut issues = Vec::new();
    for error in validator.iter_errors(value) {
        let instance_str = error.instance_path().to_string();
        let instance_pointer = if instance_str.is_empty() {
            "".to_string()
        } else {
            instance_str
        };

        let schema_str = error.schema_path().to_string();
        let schema_pointer = if schema_str.is_empty() {
            "".to_string()
        } else {
            schema_str
        };

        // Extract keyword from the end of schema_path or map kind cleanly
        let keyword = error
            .schema_path()
            .into_iter()
            .last()
            .map(|k| k.to_string())
            .unwrap_or_else(|| match error.kind() {
                jsonschema::error::ValidationErrorKind::AdditionalProperties { .. } => {
                    "additionalProperties".to_string()
                }
                jsonschema::error::ValidationErrorKind::Required { .. } => "required".to_string(),
                jsonschema::error::ValidationErrorKind::Type { .. } => "type".to_string(),
                jsonschema::error::ValidationErrorKind::MinLength { .. } => "minLength".to_string(),
                jsonschema::error::ValidationErrorKind::MaxLength { .. } => "maxLength".to_string(),
                jsonschema::error::ValidationErrorKind::Minimum { .. } => "minimum".to_string(),
                jsonschema::error::ValidationErrorKind::Maximum { .. } => "maximum".to_string(),
                _ => "validation".to_string(),
            });

        let message = Some(error.to_string());
        issues.push(ValidationIssue::new(
            instance_pointer,
            schema_pointer,
            keyword,
            message,
        ));
    }

    Ok(ValidationReport::new(issues))
}
