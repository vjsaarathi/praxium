use praxium::{Error, ObjectStore, SqliteStorage, Store};
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

#[test]
fn test_store_open_new_creates_without_seeding_defaults() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");

    assert!(!db_path.exists());
    let storage = SqliteStorage::open(&db_path).expect("open creates store");
    let store = ObjectStore::new(storage);
    assert!(db_path.exists());

    // Verify open alone never seeds defaults
    let collections = store.collections().list().expect("list collections");
    assert!(collections.is_empty(), "open alone must not seed any default collections");
}

#[test]
fn test_store_open_existing_reopens_cleanly() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");

    {
        let storage = SqliteStorage::open(&db_path).expect("initial open");
        let store = Store::new(storage);
        drop(store);
    }

    assert!(db_path.exists());
    let storage2 = SqliteStorage::open(&db_path).expect("reopen");
    let store2 = Store::new(storage2);
    let collections = store2.collections().list().expect("list collections");
    assert!(collections.is_empty());
}

#[test]
fn test_store_open_newer_unsupported_version_returns_distinct_error() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");

    // Create DB with newer version tag in SQLite metadata
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open raw sqlite");
        conn.execute_batch(
            "CREATE TABLE _praxium_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _praxium_metadata (key, value) VALUES ('storage_version', '999');"
        ).expect("write metadata");
    }

    // Now try to open with SqliteStorage::open
    let res = SqliteStorage::open(&db_path);
    match res {
        Err(Error::UnsupportedVersion { found_version, supported_version }) => {
            assert_eq!(found_version, 999);
            assert!(supported_version < 999);
        }
        other => panic!("expected Error::UnsupportedVersion, got {:?}", other),
    }

    // Verify raw db version was left unchanged
    let conn = rusqlite::Connection::open(&db_path).expect("open raw sqlite");
    let val: String = conn.query_row(
        "SELECT value FROM _praxium_metadata WHERE key = 'storage_version'",
        [],
        |r| r.get(0),
    ).expect("query version");
    assert_eq!(val, "999", "unsupported store version must remain unchanged");
}

#[test]
fn test_store_clone_and_thread_safety() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");

    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);
    let store_arc = Arc::new(store);

    let mut handles = vec![];
    for _ in 0..4 {
        let store_clone = Arc::clone(&store_arc);
        handles.push(thread::spawn(move || {
            let cols = store_clone.collections().list().expect("concurrent list");
            assert!(cols.is_empty());
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }
}

#[test]
fn test_store_operation_groups_available() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let _ = store.collections();
    let _ = store.schemas();
    let _ = store.objects();
    let _ = store.query();
    let _ = store.history();
    let _ = store.transactions();
}

#[test]
fn test_transaction_rollback_on_error() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let res: Result<(), Error> = store.transactions().run(|| {
        Err(Error::invalid_input("tx", "test", "simulated failure"))
    });

    assert!(res.is_err());
}

#[test]
fn test_transaction_panic_propagates_unchanged() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = store.transactions().run(|| -> Result<(), Error> {
            panic!("unhandled crash inside transaction");
        });
    }));

    assert!(result.is_err(), "panic must propagate during unwinding");
    let panic_msg = result.unwrap_err();
    let msg = panic_msg.downcast_ref::<&str>().unwrap();
    assert_eq!(*msg, "unhandled crash inside transaction");
}

#[test]
fn test_schema_validate_without_write() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let valid_schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object"
    });
    assert!(store.schemas().validate(&valid_schema).is_ok());

    let invalid_schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object"
    });
    let err = store.schemas().validate(&invalid_schema).unwrap_err();
    match err {
        Error::InvalidInput { field, .. } => assert_eq!(field, "schema"),
        other => panic!("expected InvalidInput, got {:?}", other),
    }
}

#[test]
fn test_store_is_backend_agnostic() {
    use praxium::domain::manifest::{InitializationManifest, InitializationReport};
    use praxium::domain::types::CollectionName;
    use praxium::{StorageEngine, StorageTransaction};

    #[derive(Debug)]
    struct MockStorage;

    struct MockTx;
    impl StorageTransaction for MockTx {
        fn list_collections(&mut self) -> Result<Vec<CollectionName>, Error> {
            Ok(vec![CollectionName::new("mock_collection").unwrap()])
        }
    }

    impl StorageEngine for MockStorage {
        fn seed_manifest(&self, _manifest: &InitializationManifest) -> Result<InitializationReport, Error> {
            Ok(InitializationReport::default())
        }

        fn run_transaction(&self, f: &mut dyn FnMut(&mut dyn StorageTransaction) -> Result<(), Error>) -> Result<(), Error> {
            let mut tx = MockTx;
            f(&mut tx)
        }
    }

    // Pass custom backend directly to ObjectStore::new constructor
    let store = ObjectStore::new(MockStorage);
    let cols = store.collections().list().expect("list collections from mock backend");
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].as_str(), "mock_collection");
}

#[test]
fn test_store_new_with_sqlite_backend() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");

    // Caller chooses SQLite explicitly and passes it to ObjectStore::new
    let storage = SqliteStorage::open(&db_path).expect("open sqlite storage");
    let store = ObjectStore::new(storage);

    let cols = store.collections().list().expect("list collections");
    assert!(cols.is_empty());
}
