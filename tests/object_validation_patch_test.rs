use praxium::{CollectionName, Error, ObjectRevision, ObjectStore, SqliteStorage};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_object_validation_and_patch_lifecycle() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("praxium.db");
    let storage = SqliteStorage::open(&db_path).expect("open");
    let store = ObjectStore::new(storage);

    let items = CollectionName::new("items").unwrap();
    let nonexistent = CollectionName::new("nonexistent").unwrap();

    // 1. Validation on non-existent collection returns NotFound
    let err = store
        .objects()
        .validate(&nonexistent, &json!({ "name": "item" }))
        .unwrap_err();
    match err {
        Error::NotFound {
            operation,
            entity,
            id,
        } => {
            assert_eq!(operation, "validate_object");
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // 2. Validation with non-object root returns InvalidInput
    store
        .collections()
        .create(&items)
        .expect("create collection");
    let err = store
        .objects()
        .validate(&items, &json!("string value"))
        .unwrap_err();
    match err {
        Error::InvalidInput {
            operation, field, ..
        } => {
            assert_eq!(operation, "validate_object");
            assert_eq!(field, "value");
        }
        other => panic!("expected InvalidInput, got {:?}", other),
    }

    // 3. Validation on collection without schema is always valid
    let report = store
        .objects()
        .validate(&items, &json!({ "any": "thing" }))
        .expect("validate without schema");
    assert!(report.is_valid());
    assert!(report.issues().is_empty());

    // 4. Attach an active schema requiring "sku" (string), "price" (number >= 0), "tags" (array of strings)
    let schema_v1 = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["sku", "price"],
        "properties": {
            "sku": { "type": "string", "minLength": 3 },
            "price": { "type": "number", "minimum": 0 },
            "tags": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "additionalProperties": false
    });
    let manifest = praxium::InitializationManifest::builder()
        .add_collection(praxium::CollectionDefinition::new(items.clone()).with_schema(schema_v1))
        .build();
    store
        .initialize(&manifest)
        .expect("initialize manifest with schema");

    // 5. Validate-without-write against active schema
    // 5a. Valid candidate
    let valid_candidate = json!({
        "sku": "ITEM-1",
        "price": 19.95,
        "tags": ["alpha", "beta"]
    });
    let report = store
        .objects()
        .validate(&items, &valid_candidate)
        .expect("validate valid");
    assert!(report.is_valid());
    assert!(report.issues().is_empty());

    // 5b. Invalid candidate returning deterministically sorted issues
    let invalid_candidate = json!({
        "sku": "A",               // too short (< minLength 3)
        "price": -10,             // negative (< minimum 0)
        "extra_field": true       // additionalProperties forbidden
    });
    let report = store
        .objects()
        .validate(&items, &invalid_candidate)
        .expect("validate invalid");
    assert!(!report.is_valid());
    assert_eq!(report.issues().len(), 3);
    // Verify deterministic ordering: instance_pointer, then schema_pointer, then keyword
    for window in report.issues().windows(2) {
        let a = (
            &window[0].instance_pointer,
            &window[0].schema_pointer,
            &window[0].keyword,
        );
        let b = (
            &window[1].instance_pointer,
            &window[1].schema_pointer,
            &window[1].keyword,
        );
        assert!(
            a <= b,
            "issues must be deterministically sorted: {:?} vs {:?}",
            a,
            b
        );
    }

    // Verify full detail on returned validation issues
    let issue0 = &report.issues()[0];
    assert_eq!(issue0.instance_pointer, "");
    assert_eq!(issue0.schema_pointer, "/additionalProperties");
    assert_eq!(issue0.keyword, "additionalProperties");
    assert!(issue0.message.is_some());

    let issue1 = &report.issues()[1];
    assert_eq!(issue1.instance_pointer, "/price");
    assert_eq!(issue1.schema_pointer, "/properties/price/minimum");
    assert_eq!(issue1.keyword, "minimum");
    assert!(issue1.message.is_some());

    let issue2 = &report.issues()[2];
    assert_eq!(issue2.instance_pointer, "/sku");
    assert_eq!(issue2.schema_pointer, "/properties/sku/minLength");
    assert_eq!(issue2.keyword, "minLength");
    assert!(issue2.message.is_some());

    // 6. Create rejecting invalid candidate with Error::Validation
    let err = store
        .objects()
        .create(&items, invalid_candidate.clone())
        .unwrap_err();
    match err {
        Error::Validation { operation, report } => {
            assert_eq!(operation, "create_object");
            assert!(!report.is_valid());
            assert_eq!(report.issues().len(), 3);
        }
        other => panic!("expected Validation error, got {:?}", other),
    }

    // 7. Create valid object
    let created = store
        .objects()
        .create(&items, valid_candidate.clone())
        .expect("create valid");
    assert_eq!(created.revision.get(), 1);

    // 8. Replace rejecting invalid candidate and preserving original state (atomicity)
    let err = store
        .objects()
        .replace(&items, &created.id, created.revision, invalid_candidate)
        .unwrap_err();
    match err {
        Error::Validation { operation, .. } => assert_eq!(operation, "replace_object"),
        other => panic!("expected Validation error, got {:?}", other),
    }

    // Verify storage is untouched after failed replace
    let unchanged = store
        .objects()
        .get(&items, &created.id)
        .expect("get")
        .expect("found");
    assert_eq!(unchanged.revision.get(), 1);
    assert_eq!(unchanged.value, valid_candidate);

    // 9. Patch operations (RFC 6902)
    // 9a. Happy path: add a tag and update price
    let patch_ops = json!([
        { "op": "replace", "path": "/price", "value": 29.95 },
        { "op": "add", "path": "/tags/2", "value": "gamma" }
    ]);
    let patched = store
        .objects()
        .patch(&items, &created.id, created.revision, patch_ops)
        .expect("patch object");
    assert_eq!(patched.revision.get(), 2);
    assert_eq!(patched.value["price"], 29.95);
    assert_eq!(patched.value["tags"], json!(["alpha", "beta", "gamma"]));
    assert_eq!(patched.created_at, created.created_at);
    assert!(patched.updated_at >= created.updated_at);

    // Verify in storage
    let fetched_patched = store
        .objects()
        .get(&items, &created.id)
        .expect("get")
        .expect("found");
    assert_eq!(fetched_patched, patched);

    // 9b. Patch with stale revision returns Conflict
    let stale_err = store
        .objects()
        .patch(
            &items,
            &created.id,
            ObjectRevision::INITIAL, // stale revision 1
            json!([{ "op": "replace", "path": "/price", "value": 39.95 }]),
        )
        .unwrap_err();
    match stale_err {
        Error::Conflict {
            operation,
            entity,
            expected,
            current,
            ..
        } => {
            assert_eq!(operation, "patch_object");
            assert_eq!(entity, "object");
            assert_eq!(expected.as_deref(), Some("1"));
            assert_eq!(current.as_deref(), Some("2"));
        }
        other => panic!("expected Conflict, got {:?}", other),
    }

    // 9c. Patch malformed syntax returns InvalidInput
    let malformed_err = store
        .objects()
        .patch(
            &items,
            &created.id,
            patched.revision,
            json!({ "not": "an array" }),
        )
        .unwrap_err();
    match malformed_err {
        Error::InvalidInput {
            operation, field, ..
        } => {
            assert_eq!(operation, "patch_object");
            assert_eq!(field, "patch");
        }
        other => panic!("expected InvalidInput, got {:?}", other),
    }

    // 9d. Patch operation failure (e.g. test op mismatch) returns InvalidInput
    let test_fail_patch = json!([
        { "op": "test", "path": "/price", "value": 999.99 },
        { "op": "replace", "path": "/price", "value": 50.0 }
    ]);
    let test_fail_err = store
        .objects()
        .patch(&items, &created.id, patched.revision, test_fail_patch)
        .unwrap_err();
    match test_fail_err {
        Error::InvalidInput {
            operation, field, ..
        } => {
            assert_eq!(operation, "patch_object");
            assert_eq!(field, "patch");
        }
        other => panic!("expected InvalidInput, got {:?}", other),
    }

    // 9e. Patch violating active schema returns Error::Validation and preserves storage (atomicity)
    let schema_violation_patch = json!([
        { "op": "replace", "path": "/price", "value": -50.0 } // invalid negative price
    ]);
    let validation_err = store
        .objects()
        .patch(
            &items,
            &created.id,
            patched.revision,
            schema_violation_patch,
        )
        .unwrap_err();
    match validation_err {
        Error::Validation { operation, report } => {
            assert_eq!(operation, "patch_object");
            assert!(!report.is_valid());
        }
        other => panic!("expected Validation error, got {:?}", other),
    }

    // Verify storage remained at revision 2 after failed patch
    let untouched_after_patch = store
        .objects()
        .get(&items, &created.id)
        .expect("get")
        .expect("found");
    assert_eq!(untouched_after_patch.revision.get(), 2);
    assert_eq!(untouched_after_patch.value["price"], 29.95);

    // 9f. Patch on non-existent object returns NotFound
    let dummy_id = praxium::ObjectId::new_v7();
    let not_found_err = store
        .objects()
        .patch(&items, &dummy_id, ObjectRevision::INITIAL, json!([]))
        .unwrap_err();
    match not_found_err {
        Error::NotFound { entity, id, .. } => {
            assert_eq!(entity, "object");
            assert_eq!(id.as_deref(), Some(dummy_id.to_string().as_str()));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // 9g. Patch on non-existent collection returns NotFound
    let col_not_found_err = store
        .objects()
        .patch(&nonexistent, &created.id, patched.revision, json!([]))
        .unwrap_err();
    match col_not_found_err {
        Error::NotFound { entity, id, .. } => {
            assert_eq!(entity, "collection");
            assert_eq!(id.as_deref(), Some("nonexistent"));
        }
        other => panic!("expected NotFound, got {:?}", other),
    }
}
