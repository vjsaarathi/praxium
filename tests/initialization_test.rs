use praxium::domain::manifest::{CollectionDefinition, IndexDefinition, InitializationManifest};
use praxium::domain::types::{CollectionName, IndexVersion, JsonPointer, SchemaVersion};
use praxium::{Error, ObjectStore, SqliteStorage};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_initialization_seeds_missing_definitions_atomically() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let task_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "priority": { "type": "integer" }
        },
        "required": ["title"]
    });

    let manifest = InitializationManifest::builder()
        .add_collection(
            CollectionDefinition::new(CollectionName::new("tasks").unwrap())
                .with_schema(task_schema)
                .add_index(IndexDefinition::new(JsonPointer::new("/priority").unwrap())),
        )
        .add_collection(CollectionDefinition::new(
            CollectionName::new("projects").unwrap(),
        ))
        .build();

    let report = store.initialize(&manifest).expect("initialize succeeds");

    // Check report
    assert_eq!(report.seeded_collections.len(), 2);
    assert!(
        report
            .seeded_collections
            .contains(&CollectionName::new("tasks").unwrap())
    );
    assert!(
        report
            .seeded_collections
            .contains(&CollectionName::new("projects").unwrap())
    );
    assert!(report.preserved_collections.is_empty());

    assert_eq!(report.seeded_schemas.len(), 1);
    assert_eq!(
        report.seeded_schemas[0],
        (
            CollectionName::new("tasks").unwrap(),
            SchemaVersion::INITIAL
        )
    );
    assert!(report.preserved_schemas.is_empty());

    assert_eq!(report.seeded_indexes.len(), 1);
    assert_eq!(
        report.seeded_indexes[0],
        (
            CollectionName::new("tasks").unwrap(),
            JsonPointer::new("/priority").unwrap(),
            IndexVersion::INITIAL
        )
    );
    assert!(report.preserved_indexes.is_empty());

    // Verify in store
    let cols = store.collections().list().expect("list collections");
    assert_eq!(cols.len(), 2);
}

#[test]
fn test_initialization_idempotency_and_preservation() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object"
    });

    let manifest = InitializationManifest::builder()
        .add_collection(
            CollectionDefinition::new(CollectionName::new("items").unwrap())
                .with_schema(schema.clone())
                .add_index(IndexDefinition::new(JsonPointer::new("/tag").unwrap())),
        )
        .build();

    // First initialization
    let report1 = store.initialize(&manifest).expect("first initialize");
    assert_eq!(report1.seeded_collections.len(), 1);
    assert_eq!(report1.seeded_schemas.len(), 1);
    assert_eq!(report1.seeded_indexes.len(), 1);

    // Second initialization with identical manifest: everything preserved, nothing re-seeded
    let report2 = store.initialize(&manifest).expect("second initialize");
    assert!(report2.seeded_collections.is_empty());
    assert_eq!(report2.preserved_collections.len(), 1);
    assert_eq!(
        report2.preserved_collections[0],
        CollectionName::new("items").unwrap()
    );

    assert!(report2.seeded_schemas.is_empty());
    assert_eq!(report2.preserved_schemas.len(), 1);
    assert_eq!(
        report2.preserved_schemas[0],
        (
            CollectionName::new("items").unwrap(),
            SchemaVersion::INITIAL
        )
    );

    assert!(report2.seeded_indexes.is_empty());
    assert_eq!(report2.preserved_indexes.len(), 1);
    assert_eq!(
        report2.preserved_indexes[0],
        (
            CollectionName::new("items").unwrap(),
            JsonPointer::new("/tag").unwrap(),
            IndexVersion::INITIAL
        )
    );
}

#[test]
fn test_initialization_preserves_unmanifested_existing_definitions() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object"
    });

    let manifest1 = InitializationManifest::builder()
        .add_collection(
            CollectionDefinition::new(CollectionName::new("first_col").unwrap())
                .with_schema(schema.clone())
                .add_index(IndexDefinition::new(JsonPointer::new("/field").unwrap())),
        )
        .build();

    store.initialize(&manifest1).expect("manifest1 initialize");

    // manifest2 only defines second_col, omitting first_col
    let manifest2 = InitializationManifest::builder()
        .add_collection(CollectionDefinition::new(
            CollectionName::new("second_col").unwrap(),
        ))
        .build();

    let report2 = store.initialize(&manifest2).expect("manifest2 initialize");

    // second_col is seeded
    assert_eq!(
        report2.seeded_collections,
        vec![CollectionName::new("second_col").unwrap()]
    );
    // first_col is reported in preserved_collections even though omitted from manifest2!
    assert!(
        report2
            .preserved_collections
            .contains(&CollectionName::new("first_col").unwrap())
    );
    // first_col's schema is reported in preserved_schemas!
    assert!(
        report2
            .preserved_schemas
            .iter()
            .any(|(c, _)| c.as_str() == "first_col")
    );
    // first_col's index is reported in preserved_indexes!
    assert!(
        report2
            .preserved_indexes
            .iter()
            .any(|(c, p, _)| c.as_str() == "first_col" && p.as_str() == "/field")
    );
}

#[test]
fn test_initialization_atomic_rollback_on_invalid_schema() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    // Schema with unsupported dialect (Draft 7)
    let bad_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object"
    });

    let manifest = InitializationManifest::builder()
        .add_collection(CollectionDefinition::new(
            CollectionName::new("valid_col").unwrap(),
        ))
        .add_collection(
            CollectionDefinition::new(CollectionName::new("bad_col").unwrap())
                .with_schema(bad_schema),
        )
        .build();

    let err = store
        .initialize(&manifest)
        .expect_err("must fail due to unsupported schema dialect");
    match err {
        Error::InvalidInput { field, .. } => assert_eq!(field, "schema"),
        other => panic!("expected InvalidInput for schema dialect, got {:?}", other),
    }

    // Atomicity check: valid_col was NOT created because transaction rolled back
    let cols = store.collections().list().expect("list collections");
    assert!(
        cols.is_empty(),
        "all-or-nothing: no collections should exist after rollback"
    );
}
