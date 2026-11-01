use praxium::Error;
use praxium::domain::types::{
    CollectionName, IndexVersion, JsonPointer, ObjectId, ObjectRevision, SchemaVersion,
};
use std::str::FromStr;

#[test]
fn test_collection_name_valid() {
    let name = CollectionName::new("tasks").expect("valid collection name");
    assert_eq!(name.as_str(), "tasks");
    assert_eq!(name.to_string(), "tasks");

    // Preserves exact case and text without normalization
    let mixed = CollectionName::new("Task_Items-2026").expect("valid mixed name");
    assert_eq!(mixed.as_str(), "Task_Items-2026");
}

#[test]
fn test_collection_name_rejects_empty_and_control_chars() {
    // Empty rejected
    let err = CollectionName::new("").unwrap_err();
    match err {
        Error::InvalidInput { field, .. } => assert_eq!(field, "collection_name"),
        other => panic!("expected InvalidInput, got {:?}", other),
    }

    // Rejects ASCII control characters
    assert!(CollectionName::new("tasks\n").is_err());
    assert!(CollectionName::new("tasks\tname").is_err());
    assert!(CollectionName::new("tasks\0").is_err());
    assert!(CollectionName::new("\u{001F}task").is_err());
}

#[test]
fn test_object_id_uuidv7() {
    let id = ObjectId::new_v7();
    let id_str = id.to_string();
    let parsed = ObjectId::from_str(&id_str).expect("valid uuidv7");
    assert_eq!(id, parsed);

    // Rejects invalid UUID or non-v7 UUID (e.g. nil or v4)
    assert!(ObjectId::from_str("not-a-uuid").is_err());
    assert!(ObjectId::from_str("00000000-0000-0000-0000-000000000000").is_err());
    // v4 UUID (version nibble is 4)
    assert!(ObjectId::from_str("9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d").is_err());
}

#[test]
fn test_revisions_and_versions() {
    // ObjectRevision starts at 1
    let rev = ObjectRevision::new(1).expect("valid revision");
    assert_eq!(rev.get(), 1);
    assert_eq!(rev.next().get(), 2);
    assert!(ObjectRevision::new(0).is_err());

    // SchemaVersion starts at 1
    let s_ver = SchemaVersion::new(1).expect("valid schema version");
    assert_eq!(s_ver.get(), 1);
    assert_eq!(s_ver.next().get(), 2);
    assert!(SchemaVersion::new(0).is_err());

    // IndexVersion starts at 1
    let i_ver = IndexVersion::new(1).expect("valid index version");
    assert_eq!(i_ver.get(), 1);
    assert_eq!(i_ver.next().get(), 2);
    assert!(IndexVersion::new(0).is_err());
}

#[test]
fn test_json_pointer_rfc6901() {
    // Empty pointer addresses root
    let root = JsonPointer::new("").expect("valid root pointer");
    assert_eq!(root.as_str(), "");
    assert!(root.is_root());

    // Valid pointers starting with /
    let ptr = JsonPointer::new("/a/b/c").expect("valid pointer");
    assert_eq!(ptr.as_str(), "/a/b/c");
    assert!(!ptr.is_root());

    // Valid escaped pointers (~0 for ~, ~1 for /)
    let escaped = JsonPointer::new("/m~0n/k~1l").expect("valid escaped pointer");
    assert_eq!(escaped.as_str(), "/m~0n/k~1l");

    // Invalid pointers: non-empty must start with '/'
    assert!(JsonPointer::new("invalid").is_err());
    // Invalid escape sequence (~2 is invalid in RFC 6901)
    assert!(JsonPointer::new("/invalid~2code").is_err());
    // Trailing tilde is invalid escape sequence
    assert!(JsonPointer::new("/invalid~").is_err());
}
