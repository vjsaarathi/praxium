use praxium::{CollectionName, Error, ObjectStore, SqliteStorage};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_object_create_and_get() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let items = CollectionName::new("items").unwrap();
    let nonexistent = CollectionName::new("nonexistent").unwrap();

    // 1. Create on nonexistent collection fails with NotFound
    let err = store
        .objects()
        .create(&nonexistent, json!({ "name": "widget" }))
        .unwrap_err();
    match err {
        Error::NotFound {
            operation,
            entity,
            id,
        } => {
            assert_eq!(operation, "create_object");
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // Create the collection
    store
        .collections()
        .create(&items)
        .expect("create collection");

    // 2. Create with non-object root fails with InvalidInput
    let err = store
        .objects()
        .create(&items, json!("not an object"))
        .unwrap_err();
    match err {
        Error::InvalidInput { field, .. } => assert_eq!(field, "value"),
        other => panic!("expected InvalidInput, got {:?}", other),
    }

    // 3. Create valid object
    let val = json!({
        "sku": "WDG-001",
        "name": "Super Widget",
        "price": 49.99,
        "tags": ["tools", "gadgets"]
    });
    let obj = store
        .objects()
        .create(&items, val.clone())
        .expect("create object");

    assert_eq!(obj.collection, items);
    assert_eq!(obj.revision.get(), 1);
    assert_eq!(obj.value, val);
    assert!(obj.created_at <= std::time::SystemTime::now());
    assert_eq!(obj.created_at, obj.updated_at);
    assert!(obj.deleted_at.is_none());

    // 4. Get object by ID
    let fetched = store
        .objects()
        .get(&items, &obj.id)
        .expect("get")
        .expect("found");
    assert_eq!(fetched, obj);

    // 5. Get non-existent object ID returns None
    let dummy_id = praxium::ObjectId::new_v7();
    assert!(
        store
            .objects()
            .get(&items, &dummy_id)
            .expect("get")
            .is_none()
    );

    // 6. Get on non-existent collection returns NotFound
    let err = store.objects().get(&nonexistent, &obj.id).unwrap_err();
    match err {
        Error::NotFound {
            operation,
            entity,
            id,
        } => {
            assert_eq!(operation, "get_object");
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // 7. Replace object happy path (revision 1 -> 2)
    let new_val = json!({
        "sku": "WDG-001",
        "name": "Super Widget Pro",
        "price": 59.99,
        "tags": ["tools", "gadgets", "pro"]
    });
    let replaced = store
        .objects()
        .replace(&items, &obj.id, obj.revision, new_val.clone())
        .expect("replace object");
    assert_eq!(replaced.revision.get(), 2);
    assert_eq!(replaced.value, new_val);
    assert_eq!(replaced.created_at, obj.created_at);
    assert!(replaced.updated_at >= obj.updated_at);

    // Verify fetched reflects replacement
    let fetched_replaced = store
        .objects()
        .get(&items, &obj.id)
        .expect("get")
        .expect("found");
    assert_eq!(fetched_replaced, replaced);

    // 8. Replace with stale revision returns Conflict
    let stale_err = store
        .objects()
        .replace(&items, &obj.id, obj.revision, json!({ "name": "Stale" }))
        .unwrap_err();
    match stale_err {
        Error::Conflict {
            operation,
            entity,
            expected,
            current,
            ..
        } => {
            assert_eq!(operation, "replace_object");
            assert_eq!(entity, "object");
            assert_eq!(expected.as_deref(), Some("1"));
            assert_eq!(current.as_deref(), Some("2"));
        }
        other => panic!("expected Conflict, got {:?}", other),
    }

    // 9. Replace non-existent object returns NotFound
    let not_found_err = store
        .objects()
        .replace(
            &items,
            &dummy_id,
            praxium::ObjectRevision::INITIAL,
            json!({ "name": "Missing" }),
        )
        .unwrap_err();
    match not_found_err {
        Error::NotFound { entity, id, .. } => {
            assert_eq!(entity, "object");
            assert_eq!(id.as_deref(), Some(dummy_id.to_string().as_str()));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // 10. Replace on non-existent collection returns NotFound
    let col_not_found_err = store
        .objects()
        .replace(
            &nonexistent,
            &obj.id,
            replaced.revision,
            json!({ "name": "Missing Col" }),
        )
        .unwrap_err();
    match col_not_found_err {
        Error::NotFound { entity, id, .. } => {
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // 11. Replace with non-object root returns InvalidInput
    let invalid_input_err = store
        .objects()
        .replace(
            &items,
            &obj.id,
            replaced.revision,
            json!(["not", "an", "object"]),
        )
        .unwrap_err();
    match invalid_input_err {
        Error::InvalidInput { field, .. } => assert_eq!(field, "value"),
        other => panic!("expected InvalidInput, got {:?}", other),
    }
}

#[test]
fn test_object_persistence_across_reopen() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");

    let items = CollectionName::new("items").unwrap();
    let initial_val = json!({
        "title": "Item One",
        "count": 42,
        "active": true
    });

    let original_id;
    let original_rev;
    let original_created_at;
    let original_updated_at;

    // 1. Open store, create collection and object, then drop store
    {
        let storage = SqliteStorage::open(&db_path).expect("open");
        let store = ObjectStore::new(storage);
        store
            .collections()
            .create(&items)
            .expect("create collection");
        let obj = store
            .objects()
            .create(&items, initial_val.clone())
            .expect("create object");

        original_id = obj.id;
        original_rev = obj.revision;
        original_created_at = obj.created_at;
        original_updated_at = obj.updated_at;
    }

    // 2. Reopen store from same database file
    {
        let storage = SqliteStorage::open(&db_path).expect("reopen");
        let store = ObjectStore::new(storage);

        let fetched = store
            .objects()
            .get(&items, &original_id)
            .expect("get")
            .expect("found");
        assert_eq!(fetched.id, original_id);
        assert_eq!(fetched.revision, original_rev);
        assert_eq!(fetched.revision.get(), 1);
        assert_eq!(fetched.value, initial_val);
        assert_eq!(fetched.created_at, original_created_at);
        assert_eq!(fetched.updated_at, original_updated_at);
        assert!(fetched.deleted_at.is_none());

        // Perform a replacement after reopen
        let updated_val = json!({
            "title": "Item One Updated",
            "count": 43,
            "active": false
        });
        let replaced = store
            .objects()
            .replace(&items, &original_id, fetched.revision, updated_val.clone())
            .expect("replace after reopen");
        assert_eq!(replaced.revision.get(), 2);
        assert_eq!(replaced.value, updated_val);
        assert_eq!(replaced.created_at, original_created_at);
    }

    // 3. Reopen second time to verify replaced object persisted
    {
        let storage = SqliteStorage::open(&db_path).expect("reopen 2");
        let store = ObjectStore::new(storage);

        let fetched = store
            .objects()
            .get(&items, &original_id)
            .expect("get")
            .expect("found");
        assert_eq!(fetched.revision.get(), 2);
        assert_eq!(fetched.value["title"], "Item One Updated");
        assert_eq!(fetched.value["count"], 43);
        assert_eq!(fetched.value["active"], false);
        assert_eq!(fetched.created_at, original_created_at);
    }
}
