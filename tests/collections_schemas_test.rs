use praxium::{CollectionName, Error, ObjectStore, SqliteStorage};
use tempfile::tempdir;

#[test]
fn test_collection_create_get_exists_and_list() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let users = CollectionName::new("users").unwrap();
    let documents = CollectionName::new("documents").unwrap();

    // Initially neither exists, get returns None
    assert!(!store.collections().exists(&users).expect("exists"));
    assert!(!store.collections().exists(&documents).expect("exists"));
    assert!(store.collections().get(&users).expect("get").is_none());
    assert!(store.collections().list().expect("list").is_empty());

    // Create users collection
    store.collections().create(&users).expect("create users");
    assert!(store.collections().exists(&users).expect("exists"));
    assert!(!store.collections().exists(&documents).expect("exists"));

    // Lookup users collection metadata
    let users_meta = store.collections().get(&users).expect("get users").expect("found");
    assert_eq!(users_meta.name, users);
    assert!(users_meta.created_at <= std::time::SystemTime::now());

    // Create documents collection
    store.collections().create(&documents).expect("create documents");
    assert!(store.collections().exists(&documents).expect("exists"));

    // List collections should be sorted ascending: ["documents", "users"]
    let list = store.collections().list().expect("list");
    assert_eq!(list, vec![documents.clone(), users.clone()]);

    // Duplicate create returns Conflict error
    let err = store.collections().create(&users).unwrap_err();
    match err {
        Error::Conflict { operation, expected, current, .. } => {
            assert_eq!(operation, "create_collection");
            assert_eq!(expected.as_deref(), Some("unique collection"));
            assert_eq!(current.as_deref(), Some("users"));
        }
        other => panic!("expected Conflict error, got {:?}", other),
    }
}

#[test]
fn test_schema_inspection_active_and_retained_versions() {
    use praxium::{CollectionDefinition, InitializationManifest, SchemaVersion};
    use serde_json::json;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let documents_col = CollectionName::new("documents").unwrap();
    let schema_v1 = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "title": { "type": "string" }
        },
        "required": ["title"]
    });

    let manifest = InitializationManifest::builder()
        .add_collection(
            CollectionDefinition::new(documents_col.clone()).with_schema(schema_v1.clone())
        )
        .build();

    store.initialize(&manifest).expect("initialize");

    // 1. Inspect active schema
    let active = store.schemas().get_active(&documents_col).expect("get_active");
    assert!(active.is_some());
    let active_snapshot = active.unwrap();
    assert_eq!(active_snapshot.version, SchemaVersion::INITIAL);
    assert_eq!(active_snapshot.schema, schema_v1);

    // 2. Inspect retained version 1
    let v1 = store.schemas().get_version(&documents_col, SchemaVersion::new(1).unwrap()).expect("get_version");
    assert!(v1.is_some());
    let v1_snapshot = v1.unwrap();
    assert_eq!(v1_snapshot.version, SchemaVersion::new(1).unwrap());
    assert_eq!(v1_snapshot.schema, schema_v1);

    // 3. Inspect nonexistent version 2
    let v2 = store.schemas().get_version(&documents_col, SchemaVersion::new(2).unwrap()).expect("get_version 2");
    assert!(v2.is_none());

    // 4. List versions
    let versions = store.schemas().list_versions(&documents_col).expect("list_versions");
    assert_eq!(versions, vec![SchemaVersion::new(1).unwrap()]);

    // 5. Inspect collection created without schema
    let items_col = CollectionName::new("items").unwrap();
    store.collections().create(&items_col).expect("create items");
    let items_active = store.schemas().get_active(&items_col).expect("items get_active");
    assert!(items_active.is_none());
    let items_versions = store.schemas().list_versions(&items_col).expect("items list_versions");
    assert!(items_versions.is_empty());
}

#[test]
fn test_schema_operations_on_nonexistent_collection_return_not_found() {
    use praxium::SchemaVersion;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let nonexistent = CollectionName::new("nonexistent").unwrap();

    let err1 = store.schemas().get_active(&nonexistent).unwrap_err();
    match err1 {
        Error::NotFound { operation, entity, id } => {
            assert_eq!(operation, "get_active_schema");
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    let err2 = store.schemas().get_version(&nonexistent, SchemaVersion::INITIAL).unwrap_err();
    match err2 {
        Error::NotFound { operation, entity, id } => {
            assert_eq!(operation, "get_schema_version");
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    let err3 = store.schemas().list_versions(&nonexistent).unwrap_err();
    match err3 {
        Error::NotFound { operation, entity, id } => {
            assert_eq!(operation, "list_schema_versions");
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }
}

#[test]
fn test_validate_proposed_schema_lifecycle() {
    use serde_json::json;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let items_col = CollectionName::new("items").unwrap();
    let nonexistent = CollectionName::new("nonexistent").unwrap();

    let valid_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "sku": { "type": "string" },
            "price": { "type": "number", "minimum": 0 }
        },
        "required": ["sku"]
    });

    let draft7_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object"
    });

    // 1. Validating proposed schema on nonexistent collection fails with NotFound
    let not_found_err = store.schemas().validate_proposed(&nonexistent, &valid_schema).unwrap_err();
    match not_found_err {
        Error::NotFound { operation, entity, id } => {
            assert_eq!(operation, "validate_proposed");
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // Create items collection
    store.collections().create(&items_col).expect("create items");

    // 2. Validating draft 7 on existing collection fails with InvalidInput
    let invalid_dialect_err = store.schemas().validate_proposed(&items_col, &draft7_schema).unwrap_err();
    match invalid_dialect_err {
        Error::InvalidInput { field, message, .. } => {
            assert_eq!(field, "schema");
            assert!(message.contains("unsupported schema dialect"));
        }
        other => panic!("expected InvalidInput, got {:?}", other),
    }

    // 3. Validating valid Draft 2020-12 schema succeeds
    assert!(store.schemas().validate_proposed(&items_col, &valid_schema).is_ok());

    // 4. Validating malformed schema (e.g. non-object or bad type) fails
    let malformed = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": 12345
    });
    let malformed_err = store.schemas().validate_proposed(&items_col, &malformed).unwrap_err();
    match malformed_err {
        Error::InvalidInput { field, message, .. } => {
            assert_eq!(field, "schema");
            assert!(message.to_lowercase().contains("malformed"));
        }
        other => panic!("expected InvalidInput, got {:?}", other),
    }
}
