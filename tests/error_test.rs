use std::error::Error as StdError;
use praxium::error::{Error, ValidationIssue, ValidationReport};

#[test]
fn test_validation_report_deterministic_ordering() {
    let issue_c = ValidationIssue::new("/users/0/name", "/properties/name/type", "type", Some("expected string".into()));
    let issue_a = ValidationIssue::new("/age", "/properties/age/minimum", "minimum", Some("must be >= 0".into()));
    let issue_b = ValidationIssue::new("/age", "/properties/age/maximum", "maximum", Some("must be <= 120".into()));
    let issue_d = ValidationIssue::new("/users/0/name", "/properties/name/minLength", "minLength", None);

    // Pass in random order
    let report = ValidationReport::new(vec![issue_c.clone(), issue_a.clone(), issue_b.clone(), issue_d.clone()]);
    let issues = report.issues();

    assert_eq!(issues.len(), 4);
    // /age comes before /users/0/name
    assert_eq!(issues[0].instance_pointer, "/age");
    assert_eq!(issues[0].keyword, "maximum"); // /properties/age/maximum < /properties/age/minimum
    assert_eq!(issues[1].instance_pointer, "/age");
    assert_eq!(issues[1].keyword, "minimum");
    assert_eq!(issues[2].instance_pointer, "/users/0/name");
    assert_eq!(issues[2].keyword, "minLength");
    assert_eq!(issues[3].instance_pointer, "/users/0/name");
    assert_eq!(issues[3].keyword, "type");
}

#[test]
fn test_error_structured_fields_and_source() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "disk read only");
    let storage_err = Error::storage("Database::open", "failed to open database file", Some(Box::new(io_err)));

    match &storage_err {
        Error::Storage { operation, message, source } => {
            assert_eq!(*operation, "Database::open");
            assert_eq!(message, "failed to open database file");
            assert!(source.is_some());
            let src = storage_err.source().expect("has source");
            assert!(src.to_string().contains("disk read only"));
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn test_conflict_structured_fields() {
    let conflict = Error::Conflict {
        operation: "Database::replace",
        entity: "object",
        expected: Some("2".into()),
        current: Some("3".into()),
        message: "revision mismatch".into(),
    };

    match conflict {
        Error::Conflict { operation, entity, expected, current, message } => {
            assert_eq!(operation, "Database::replace");
            assert_eq!(entity, "object");
            assert_eq!(expected, Some("2".into()));
            assert_eq!(current, Some("3".into()));
            assert_eq!(message, "revision mismatch");
        }
        _ => panic!("unexpected error variant"),
    }
}

#[test]
fn test_unsupported_version_fields() {
    let err = Error::UnsupportedVersion {
        found_version: 5,
        supported_version: 1,
    };
    match err {
        Error::UnsupportedVersion { found_version, supported_version } => {
            assert_eq!(found_version, 5);
            assert_eq!(supported_version, 1);
        }
        _ => panic!("unexpected error variant"),
    }
}
