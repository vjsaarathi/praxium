use praxium::domain::types::{CollectionName, JsonPointer};
use praxium::storage::sqlite::SqliteStorage;
use praxium::{Error, Filter, ObjectStore, SortDirection, SortKey};
use serde_json::json;
use tempfile::tempdir;

// Keep existing filter-focused tests concise while exercising the public paginated API.
trait ExecuteAllForTest {
    fn execute(
        &self,
        collection: &CollectionName,
        filter: &Filter,
    ) -> Result<Vec<praxium::StoredObject>, Error>;
}

impl ExecuteAllForTest for praxium::api::operation_groups::QueryOp {
    fn execute(
        &self,
        collection: &CollectionName,
        filter: &Filter,
    ) -> Result<Vec<praxium::StoredObject>, Error> {
        Ok(self
            .execute_page(collection, filter, &[], usize::MAX, 0)?
            .objects)
    }
}

fn setup_test_store() -> (ObjectStore, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let storage = SqliteStorage::open(&db_path).expect("open sqlite");
    let store = ObjectStore::new(storage);
    (store, dir)
}

#[test]
fn query_page_sorts_missing_and_values_and_reports_total() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("page").unwrap();
    store.collections().create(&col).unwrap();
    let missing = store.objects().create(&col, json!({"x": 0})).unwrap();
    let null = store.objects().create(&col, json!({"v": null})).unwrap();
    let _boolean = store.objects().create(&col, json!({"v": false})).unwrap();
    let key = SortKey::ascending(JsonPointer::new("/v").unwrap());
    let page = store
        .query()
        .execute_page(&col, &Filter::and([]), &[key], 2, 0)
        .unwrap();
    assert_eq!(page.total_count, 3);
    assert_eq!(
        page.objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        vec![missing.id, null.id]
    );
    let next = store
        .query()
        .execute_page(&col, &Filter::and([]), &[], 2, 1)
        .unwrap();
    assert_eq!(next.objects.len(), 2);
    assert_eq!(next.total_count, 3);
    assert!(matches!(
        store
            .query()
            .execute_page(&col, &Filter::and([]), &[], 0, 0),
        Err(Error::InvalidInput {
            operation: "execute_query",
            field: "limit",
            ..
        })
    ));
}

#[test]
fn query_sort_orders_every_json_type_and_descending_reverses_missing_placement() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("all-json-types").unwrap();
    store.collections().create(&col).unwrap();
    let values = [
        json!({}),
        json!({"v": null}),
        json!({"v": false}),
        json!({"v": true}),
        json!({"v": 2}),
        json!({"v": "a"}),
        json!({"v": [1]}),
        json!({"v": {"a": 1}}),
    ];
    let ids: Vec<_> = values
        .iter()
        .map(|value| store.objects().create(&col, value.clone()).unwrap().id)
        .collect();
    let pointer = JsonPointer::new("/v").unwrap();
    let ascending = store
        .query()
        .execute_page(
            &col,
            &Filter::and([]),
            &[SortKey::ascending(pointer.clone())],
            20,
            0,
        )
        .unwrap();
    assert_eq!(
        ascending.objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        ids
    );
    let descending = store
        .query()
        .execute_page(
            &col,
            &Filter::and([]),
            &[SortKey::descending(pointer)],
            20,
            0,
        )
        .unwrap();
    assert_eq!(
        descending.objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        vec![
            ids[7], ids[6], ids[5], ids[4], ids[3], ids[2], ids[1], ids[0]
        ]
    );
}

#[test]
fn query_sort_uses_priority_per_key_direction_and_ascending_uuid_ties() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("multi-sort").unwrap();
    store.collections().create(&col).unwrap();
    let first = store
        .objects()
        .create(&col, json!({"group": 1, "rank": 1}))
        .unwrap();
    let second = store
        .objects()
        .create(&col, json!({"group": 1, "rank": 1}))
        .unwrap();
    let third = store
        .objects()
        .create(&col, json!({"group": 1, "rank": 2}))
        .unwrap();
    let fourth = store
        .objects()
        .create(&col, json!({"group": 2, "rank": 9}))
        .unwrap();
    let keys = [
        SortKey::ascending(JsonPointer::new("/group").unwrap()),
        SortKey {
            pointer: JsonPointer::new("/rank").unwrap(),
            direction: SortDirection::Descending,
        },
    ];
    let page = store
        .query()
        .execute_page(&col, &Filter::and([]), &keys, 10, 0)
        .unwrap();
    assert_eq!(
        page.objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        vec![third.id, first.id, second.id, fourth.id]
    );
}

#[test]
fn query_sort_recurses_through_arrays_and_compares_objects_by_sorted_keys() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("recursive-sort").unwrap();
    store.collections().create(&col).unwrap();
    let later_array = store
        .objects()
        .create(&col, json!({"v": [1, [false]]}))
        .unwrap();
    let earlier_array = store
        .objects()
        .create(&col, json!({"v": [1, [null]]}))
        .unwrap();
    let later_object = store
        .objects()
        .create(&col, json!({"v": {"b": 0, "a": 2}}))
        .unwrap();
    let earlier_object = store
        .objects()
        .create(&col, json!({"v": {"a": 1, "z": 0}}))
        .unwrap();
    let page = store
        .query()
        .execute_page(
            &col,
            &Filter::and([]),
            &[SortKey::ascending(JsonPointer::new("/v").unwrap())],
            10,
            0,
        )
        .unwrap();
    assert_eq!(
        page.objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        vec![
            earlier_array.id,
            later_array.id,
            earlier_object.id,
            later_object.id
        ]
    );
}

#[test]
fn query_without_sort_uses_ascending_uuid_order() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("uuid-default-sort").unwrap();
    store.collections().create(&col).unwrap();
    let inserted = [
        store.objects().create(&col, json!({"n": 3})).unwrap(),
        store.objects().create(&col, json!({"n": 1})).unwrap(),
        store.objects().create(&col, json!({"n": 2})).unwrap(),
    ];
    let page = store
        .query()
        .execute_page(&col, &Filter::and([]), &[], 10, 0)
        .unwrap();
    let mut expected: Vec<_> = inserted.iter().map(|o| o.id).collect();
    expected.sort();
    assert_eq!(
        page.objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn test_query_nonexistent_collection_returns_not_found() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("nonexistent").unwrap();
    let filter = Filter::exists(JsonPointer::new("/field").unwrap());

    let res = store.query().execute(&col, &filter);
    match res {
        Err(Error::NotFound { entity, id, .. }) => {
            assert_eq!(entity, "collection");
            assert_eq!(id, Some("nonexistent".to_string()));
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn test_query_pre_evaluation_regex_error() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("users").unwrap();
    store.collections().create(&col).unwrap();

    // Invalid regex pattern "[a-z" (unclosed bracket)
    let filter = Filter::regex(JsonPointer::new("/email").unwrap(), "[a-z");

    let res = store.query().execute(&col, &filter);
    match res {
        Err(Error::Query { details, .. }) => {
            assert_eq!(details.collection, Some("users".to_string()));
            assert_eq!(details.operator, Some("regex".to_string()));
            assert_eq!(details.pointer, Some("/email".to_string()));
            assert!(details.message.contains("regex"));
        }
        other => panic!("expected Query error with regex details, got {other:?}"),
    }
}

#[test]
fn test_query_pre_evaluation_ordered_operand_error() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("users").unwrap();
    store.collections().create(&col).unwrap();

    // Invalid operand for ordered comparison (array operand is not number or string)
    let filter = Filter::gt(JsonPointer::new("/score").unwrap(), json!([1, 2]));

    let res = store.query().execute(&col, &filter);
    match res {
        Err(Error::Query { details, .. }) => {
            assert_eq!(details.collection, Some("users".to_string()));
            assert_eq!(details.operator, Some("gt".to_string()));
            assert_eq!(details.pointer, Some("/score".to_string()));
            assert!(
                details
                    .message
                    .contains("ordered comparisons allow number and string only")
            );
        }
        other => panic!("expected Query error with ordered operand details, got {other:?}"),
    }
}

#[test]
fn test_query_exists_distinguishes_null_from_missing() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("users").unwrap();
    store.collections().create(&col).unwrap();

    // Insert obj1 with explicit null, obj2 with value, obj3 with missing field
    let obj1 = store
        .objects()
        .create(&col, json!({"name": null, "tag": "null_field"}))
        .unwrap();
    let obj2 = store
        .objects()
        .create(&col, json!({"name": "Alice", "tag": "present_field"}))
        .unwrap();
    let _obj3 = store
        .objects()
        .create(&col, json!({"tag": "missing_field"}))
        .unwrap();

    let filter = Filter::exists(JsonPointer::new("/name").unwrap());
    let results = store.query().execute(&col, &filter).unwrap();

    // Should match obj1 (explicit null is present) and obj2 (string is present), but not obj3
    assert_eq!(results.len(), 2);
    let ids: Vec<_> = results.into_iter().map(|o| o.id).collect();
    assert!(ids.contains(&obj1.id));
    assert!(ids.contains(&obj2.id));
}

#[test]
fn test_query_equality_and_inequality_contract() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("items").unwrap();
    store.collections().create(&col).unwrap();

    // 1. Mathematical numeric equality (1.0 == 1)
    let o1 = store
        .objects()
        .create(&col, json!({"val": 1.0, "status": "active"}))
        .unwrap();
    let o2 = store
        .objects()
        .create(&col, json!({"val": 2, "status": null}))
        .unwrap();
    let _o3 = store.objects().create(&col, json!({"val": 3})).unwrap(); // status is missing

    // eq("/val", 1) matches o1 where val is 1.0
    let res = store
        .query()
        .execute(&col, &Filter::eq(JsonPointer::new("/val").unwrap(), 1))
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, o1.id);

    // Structural equality: ignores key order, preserves array order
    let o4 = store
        .objects()
        .create(
            &col,
            json!({
                "nested": {"b": 2, "a": 1},
                "arr": [1, 2]
            }),
        )
        .unwrap();

    let res = store
        .query()
        .execute(
            &col,
            &Filter::eq(
                JsonPointer::new("/nested").unwrap(),
                json!({"a": 1, "b": 2}),
            ),
        )
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, o4.id);

    // Array order mismatch: [2, 1] does not equal [1, 2]
    let res = store
        .query()
        .execute(
            &col,
            &Filter::eq(JsonPointer::new("/arr").unwrap(), json!([2, 1])),
        )
        .unwrap();
    assert_eq!(res.len(), 0);

    // Missing paths do not match value comparisons, including ne!
    // o3 has no "status" field. Neither eq nor ne should match o3!
    let eq_res = store
        .query()
        .execute(
            &col,
            &Filter::eq(JsonPointer::new("/status").unwrap(), "active"),
        )
        .unwrap();
    assert_eq!(eq_res.len(), 1);
    assert_eq!(eq_res[0].id, o1.id);

    let ne_res = store
        .query()
        .execute(
            &col,
            &Filter::ne(JsonPointer::new("/status").unwrap(), "active"),
        )
        .unwrap();
    // Only o2 has status present and != "active" (status is null)
    // o3 MUST NOT match because /status is missing
    assert_eq!(ne_res.len(), 1);
    assert_eq!(ne_res[0].id, o2.id);

    // Explicit null matches eq null
    let null_res = store
        .query()
        .execute(
            &col,
            &Filter::eq(
                JsonPointer::new("/status").unwrap(),
                serde_json::Value::Null,
            ),
        )
        .unwrap();
    assert_eq!(null_res.len(), 1);
    assert_eq!(null_res[0].id, o2.id);
}

#[test]
fn test_query_ordered_comparisons_and_incompatible_types() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("metrics").unwrap();
    store.collections().create(&col).unwrap();

    let o1 = store
        .objects()
        .create(&col, json!({"score": 10.5, "title": "Alpha"}))
        .unwrap();
    let _o2 = store
        .objects()
        .create(&col, json!({"score": 20, "title": "Beta"}))
        .unwrap();

    // Mathematical range comparisons (lt, lte, gt, gte)
    let res = store
        .query()
        .execute(&col, &Filter::lt(JsonPointer::new("/score").unwrap(), 15))
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, o1.id);

    let res_lte = store
        .query()
        .execute(
            &col,
            &Filter::lte(JsonPointer::new("/score").unwrap(), 10.5),
        )
        .unwrap();
    assert_eq!(res_lte.len(), 1);
    assert_eq!(res_lte[0].id, o1.id);

    // Case-sensitive exact Unicode string comparisons including non-ASCII
    let _o_unicode1 = store
        .objects()
        .create(&col, json!({"score": 30, "title": "café"}))
        .unwrap();
    let _o_unicode2 = store
        .objects()
        .create(&col, json!({"score": 40, "title": "cafz"}))
        .unwrap();

    let res = store
        .query()
        .execute(
            &col,
            &Filter::gte(JsonPointer::new("/title").unwrap(), "Beta"),
        )
        .unwrap();
    assert_eq!(res.len(), 3); // Beta, café, cafz

    let res_unicode = store
        .query()
        .execute(
            &col,
            &Filter::gt(JsonPointer::new("/title").unwrap(), "cafz"),
        )
        .unwrap();
    // In exact Unicode scalar value order, 'é' (U+00E9 = 233) > 'z' (U+007A = 122)
    assert_eq!(res_unicode.len(), 1);
    assert_eq!(res_unicode[0].id, _o_unicode1.id);

    // Boolean NOT in SQLite API query
    let res_not = store
        .query()
        .execute(
            &col,
            &Filter::not(Filter::gte(JsonPointer::new("/score").unwrap(), 20)),
        )
        .unwrap();
    assert_eq!(res_not.len(), 1);
    assert_eq!(res_not[0].id, o1.id);

    // Incompatible stored type fails with structured Query error
    let bad_obj = store
        .objects()
        .create(&col, json!({"score": "not_a_number"}))
        .unwrap();
    let err = store
        .query()
        .execute(&col, &Filter::gt(JsonPointer::new("/score").unwrap(), 5))
        .unwrap_err();

    match err {
        Error::Query { details, .. } => {
            assert_eq!(details.collection, Some("metrics".to_string()));
            assert_eq!(details.object_id, Some(bad_obj.id.to_string()));
            assert_eq!(details.pointer, Some("/score".to_string()));
            assert_eq!(details.operator, Some("gt".to_string()));
        }
        other => panic!("expected Query error, got {other:?}"),
    }
}

#[test]
fn test_query_string_predicates() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("docs").unwrap();
    store.collections().create(&col).unwrap();

    let o1 = store
        .objects()
        .create(
            &col,
            json!({
                "slug": "praxium-core-v1",
                "email": "user@example.com",
                "file": "main.rs"
            }),
        )
        .unwrap();

    // contains
    let res = store
        .query()
        .execute(
            &col,
            &Filter::contains(JsonPointer::new("/slug").unwrap(), "core"),
        )
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, o1.id);

    // prefix
    let res = store
        .query()
        .execute(
            &col,
            &Filter::prefix(JsonPointer::new("/slug").unwrap(), "praxium-"),
        )
        .unwrap();
    assert_eq!(res.len(), 1);

    // suffix
    let res = store
        .query()
        .execute(
            &col,
            &Filter::suffix(JsonPointer::new("/file").unwrap(), ".rs"),
        )
        .unwrap();
    assert_eq!(res.len(), 1);

    // regex
    let res = store
        .query()
        .execute(
            &col,
            &Filter::regex(
                JsonPointer::new("/email").unwrap(),
                r"^[\w.-]+@example\.com$",
            ),
        )
        .unwrap();
    assert_eq!(res.len(), 1);

    // Incompatible type error: contains on integer field
    let bad_obj = store
        .objects()
        .create(&col, json!({"slug": 12345}))
        .unwrap();
    let err = store
        .query()
        .execute(
            &col,
            &Filter::contains(JsonPointer::new("/slug").unwrap(), "123"),
        )
        .unwrap_err();
    match err {
        Error::Query { details, .. } => {
            assert_eq!(details.object_id, Some(bad_obj.id.to_string()));
            assert_eq!(details.operator, Some("contains".to_string()));
        }
        other => panic!("expected Query error, got {other:?}"),
    }
}

#[test]
fn test_query_array_membership_any_and_all() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("projects").unwrap();
    store.collections().create(&col).unwrap();

    let o_empty = store.objects().create(&col, json!({"tags": []})).unwrap();
    let o_multi = store
        .objects()
        .create(&col, json!({"tags": ["rust", "wasm"]}))
        .unwrap();
    let o_single = store
        .objects()
        .create(&col, json!({"tags": ["rust"]}))
        .unwrap();

    // any on empty candidate set is false
    let res = store
        .query()
        .execute(
            &col,
            &Filter::any(JsonPointer::new("/tags").unwrap(), Vec::<String>::new()),
        )
        .unwrap();
    assert_eq!(res.len(), 0);

    // any matching at least one candidate
    let res = store
        .query()
        .execute(
            &col,
            &Filter::any(JsonPointer::new("/tags").unwrap(), ["rust", "nomatch"]),
        )
        .unwrap();
    assert_eq!(res.len(), 2);
    let ids: Vec<_> = res.into_iter().map(|o| o.id).collect();
    assert!(!ids.contains(&o_empty.id));
    assert!(ids.contains(&o_multi.id));
    assert!(ids.contains(&o_single.id));

    // all on empty candidate set is true
    let res = store
        .query()
        .execute(
            &col,
            &Filter::all(JsonPointer::new("/tags").unwrap(), Vec::<String>::new()),
        )
        .unwrap();
    // empty candidate set matches any object having an array
    assert_eq!(res.len(), 3);

    // all matching all candidates
    let res = store
        .query()
        .execute(
            &col,
            &Filter::all(JsonPointer::new("/tags").unwrap(), ["rust", "wasm"]),
        )
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, o_multi.id);

    // Incompatible type on array membership
    let bad_obj = store
        .objects()
        .create(&col, json!({"tags": "not_an_array"}))
        .unwrap();
    let err = store
        .query()
        .execute(
            &col,
            &Filter::any(JsonPointer::new("/tags").unwrap(), ["rust"]),
        )
        .unwrap_err();
    match err {
        Error::Query { details, .. } => {
            assert_eq!(details.object_id, Some(bad_obj.id.to_string()));
            assert_eq!(details.operator, Some("any".to_string()));
        }
        other => panic!("expected Query error, got {other:?}"),
    }
}

#[test]
fn test_query_object_has_key() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("servers").unwrap();
    store.collections().create(&col).unwrap();

    let o1 = store
        .objects()
        .create(&col, json!({"config": {"port": 8080, "host": "localhost"}}))
        .unwrap();
    let _o2 = store
        .objects()
        .create(&col, json!({"config": {"host": "localhost"}}))
        .unwrap();

    let res = store
        .query()
        .execute(
            &col,
            &Filter::has_key(JsonPointer::new("/config").unwrap(), "port"),
        )
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, o1.id);

    // Incompatible type: has_key on array
    let bad_obj = store
        .objects()
        .create(&col, json!({"config": [8080]}))
        .unwrap();
    let err = store
        .query()
        .execute(
            &col,
            &Filter::has_key(JsonPointer::new("/config").unwrap(), "port"),
        )
        .unwrap_err();
    match err {
        Error::Query { details, .. } => {
            assert_eq!(details.object_id, Some(bad_obj.id.to_string()));
            assert_eq!(details.operator, Some("has_key".to_string()));
        }
        other => panic!("expected Query error, got {other:?}"),
    }
}

#[test]
fn test_query_boolean_branches_fail_on_incompatible_type_even_if_branch_decides() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("records").unwrap();
    store.collections().create(&col).unwrap();

    // Row has valid branch /a and invalid stored type for branch /b
    let row = store
        .objects()
        .create(
            &col,
            json!({
                "a": true,
                "b": "not_a_number"
            }),
        )
        .unwrap();

    // Filter is OR: (a == true) OR (b > 10).
    // In ordinary boolean logic, (a == true) is true so short-circuiting would return true.
    // CONTRACT REQUIREMENT: "Any encountered incompatible stored value fails the whole query,
    // even if another boolean branch could determine that row's result."
    let filter_or = Filter::or(vec![
        Filter::eq(JsonPointer::new("/a").unwrap(), true),
        Filter::gt(JsonPointer::new("/b").unwrap(), 10),
    ]);

    let err = store.query().execute(&col, &filter_or).unwrap_err();
    match err {
        Error::Query { details, .. } => {
            assert_eq!(details.object_id, Some(row.id.to_string()));
            assert_eq!(details.pointer, Some("/b".to_string()));
            assert_eq!(details.operator, Some("gt".to_string()));
        }
        other => panic!("expected Query error from /b branch, got {other:?}"),
    }

    // Same for AND: (a == false) AND (b > 10)
    // (a == false) is false, but /b has incompatible type -> MUST fail query!
    let filter_and = Filter::and(vec![
        Filter::eq(JsonPointer::new("/a").unwrap(), false),
        Filter::gt(JsonPointer::new("/b").unwrap(), 10),
    ]);

    let err = store.query().execute(&col, &filter_and).unwrap_err();
    match err {
        Error::Query { details, .. } => {
            assert_eq!(details.object_id, Some(row.id.to_string()));
            assert_eq!(details.pointer, Some("/b".to_string()));
            assert_eq!(details.operator, Some("gt".to_string()));
        }
        other => panic!("expected Query error from /b branch, got {other:?}"),
    }
}

#[test]
fn test_query_failure_never_mutates_state() {
    let (store, _dir) = setup_test_store();
    let col = CollectionName::new("safe_col").unwrap();
    store.collections().create(&col).unwrap();

    let initial = store
        .objects()
        .create(&col, json!({"score": "bad"}))
        .unwrap();

    // Query fails due to type mismatch
    let _ = store
        .query()
        .execute(&col, &Filter::gt(JsonPointer::new("/score").unwrap(), 10));

    // Verify object was not modified
    let fetched = store.objects().get(&col, &initial.id).unwrap().unwrap();
    assert_eq!(fetched.revision, initial.revision);
    assert_eq!(fetched.value, initial.value);
    assert_eq!(fetched.updated_at, initial.updated_at);
}
