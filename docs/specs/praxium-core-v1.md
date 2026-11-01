# Praxium Core v1 Specification

## Problem Statement

Applications that manage structured JSON objects repeatedly need persistence, strict schema validation, portable querying, revisions, history, migrations, and transactions. Building these capabilities independently duplicates low-level behavior and makes it difficult to share a reliable contract between clients. Praxium must provide these generic capabilities without taking ownership of any application's domain or interpreting object fields.

## Solution

Build Praxium as a stable Rust library API for named Collections of schema-governed JSON Stored Objects. Core owns SQLite persistence, JSON Schema Draft 2020-12 validation, portable JSON Pointer queries, Collection Schemas and indexes, object identity and revisions, Change History, explicit schema migrations, and atomic transactions.

The current Stored Object is authoritative; append-only Change Entries preserve snapshots for history and restore without making the system event-sourced. Clients may seed definitions through a Core Initialization Manifest and may host their own plugins, workflows, integrations, and field interpretation through the public API. Core has no task-specific behavior, UI, plugin runtime, or language bridge in v1. SQLite is the only v1 backend; other backends may later be offered behind opt-in Cargo features while preserving the public contract.

## User Stories

1. As a Rust application developer, I want Praxium as a library API, so that I can add reusable JSON capabilities without a separate service.
2. As an application developer, I want Core to assign no meaning to JSON fields, so that unrelated applications can use the same library.
3. As an application developer, I want named Collections, so that objects can be organized into separate groups.
4. As an application developer, I want each Collection governed by an editable JSON Schema, so that valid object shape is application- or user-defined.
5. As an application developer, I want Draft 2020-12 as the sole supported schema dialect in v1 and unsupported dialects rejected, so that validation semantics are predictable.
6. As a database owner, I want immutable prior Schema Versions retained, so that schema evolution is inspectable.
7. As an application developer, I want Stored Objects validated against their active Collection Schema on create and update, so that invalid data is never committed.
8. As an application developer, I want Core-generated UUIDv7 IDs plus Core-owned revision and timestamps outside the JSON value, so that identity and concurrency metadata do not constrain my data model.
9. As an application developer, I want to read an object with its current revision, so that I can safely make later changes.
10. As an application developer, I want full replacement and JSON Patch operations guarded by the expected revision, so that stale updates fail without changing state.
11. As an application developer, I want every state-changing object operation guarded by the current revision, so that concurrent clients cannot silently overwrite each other.
12. As an application developer, I want patches and replacements validated against the active schema, so that updates cannot persist invalid data.
13. As an application developer, I want Soft Delete to hide an object while retaining it, so that it can later be recovered.
14. As an application developer, I want normal queries to omit Soft Deleted objects and explicit reads to include them when requested, so that deleted data does not appear accidentally.
15. As an application developer, I want revision-checked restore of Soft Deleted objects, so that recovery is concurrency-safe.
16. As an application developer, I want revision-checked Purge to erase an object and all its Change History with no per-object marker, so that permanent removal is complete and cannot target a newer state accidentally.
17. As an application developer, I want ordered Change History entries containing operation, resulting revision, timestamp, source, transaction ID, and full after-change snapshot when one exists, so that clients can inspect changes and offer recovery.
18. As a client developer, I want to restore an object to a prior revision as a new state-changing revision, so that undo-like behavior preserves history.
19. As a client developer, I want transaction groups restorable atomically, so that a multi-object user action can be undone consistently.
20. As an application developer, I want revision restore rejected when its snapshot fails the active schema, so that the current valid object stays unchanged.
21. As an application developer, I want multi-object writes and their Change Entries to commit or roll back atomically, so that partial actions cannot leak into storage.
22. As an application developer, I want one transaction ID shared by its Change Entries, so that related changes can be grouped for history and restore.
23. As an application developer, I want explicit schema migrations to receive client/plugin/user-supplied transforms, so that domain-specific transformation logic stays outside Core.
24. As an application developer, I want Core to validate proposed schemas and report incompatibilities, so that migration issues are visible before activation.
25. As an application developer, I want schema migrations to transform every stored object, including Soft Deleted objects, and validate every result, so that no data remains under an obsolete schema.
26. As an application developer, I want schema activation and all transformed objects to commit atomically only if every result validates, so that a failed migration leaves all state unchanged.
27. As an application developer, I want migration changes represented by ordinary Change Entries sharing one transaction ID, so that migration effects remain auditable.
28. As an application developer, I want a Core Initialization Manifest to seed missing Collections, schemas, and indexes, so that a client can provide useful defaults for new databases.
29. As a database owner, I want normal initialization to preserve all existing user-managed definitions, so that app startup never overwrites my changes.
30. As a database owner, I want changed client/plugin defaults proposed as explicit versioned upgrades with expected schema/index versions, so that updates cannot silently replace existing definitions.
31. As an application developer, I want filters to use portable JSON Pointer paths and boolean composition (`and`, `or`, `not`), so that queries are not coupled to SQLite syntax.
32. As an application developer, I want `exists(path)` to distinguish a missing path from explicit null, so that absence and null have separate query semantics.
33. As an application developer, I want recursive JSON equality/inequality (object key order ignored, array order significant), so that composite values can be compared structurally.
34. As an application developer, I want `lt`, `lte`, `gt`, and `gte` for number-to-number or string-to-string comparisons, so that range queries are portable and well-defined.
35. As an application developer, I want incompatible value/operator types to return a query error, so that malformed queries are never silently misinterpreted.
36. As an application developer, I want case-sensitive exact-Unicode `contains`, `prefix`, and `suffix` string filters plus Rust-syntax regex, so that string matching is consistent across clients.
37. As an application developer, I want invalid regex patterns rejected before evaluating results, so that query errors are deterministic.
38. As an application developer, I want exact JSON-value array membership predicates with any/all forms and object-key-existence predicates, so that generic structures remain queryable.
39. As an application developer, I want missing paths not to match value comparisons, including `ne`, and explicit null to match `eq null`, so that missing and null behavior is unambiguous.
40. As an application developer, I want a total sort order of missing, null, booleans, numbers, strings, arrays, and objects, with recursive array/object ordering, so that mixed values sort deterministically.
41. As an application developer, I want equal sort keys broken by UUID and limit/offset pagination, so that page ordering is repeatable.
42. As an application developer, I want non-unique indexes on single JSON Pointer paths to accelerate scalar values, while arrays/objects and unindexed paths remain queryable, so that indexes remain optional optimizations.
43. As an application developer, I want index changes guarded by expected versions, so that stale definition updates are rejected.
44. As a client author, I want client-hosted extensions to contribute workflows, integrations, and default Collection definitions through the public API, so that extension behavior does not require a Core plugin runtime.
45. As a future maintainer, I want SQLite internals hidden behind the stable Rust API, so that data-preserving storage migrations and future feature-gated backends do not change client contracts.

## Implementation Decisions

- The repository root is the Praxium Rust library package; this repository contains the generic Core only.
- Core owns embedded SQLite persistence and transactions. SQLite is the sole v1 backend; the public API hides backend details. Future database implementations may be opt-in Cargo features and are not part of v1.
- A Collection contains arbitrary JSON Stored Objects validated by its active Draft 2020-12 Collection Schema. No application fields are reserved. Collection Schemas have immutable retained versions.
- Core generates UUIDv7 IDs and manages monotonically increasing revisions and timestamps outside each object's JSON value. Every object mutation, including Soft Delete, restore, Purge, and revision restore, requires the expected current revision.
- The current object row is authoritative. Change History is append-only snapshots and operation metadata, not an event-sourced canonical log. Purge removes the object and its history.
- Transactions group multiple changes atomically, and all corresponding Change Entries carry a shared transaction ID. Core exposes history and revision/transaction-group restore primitives; clients manage user-facing undo/redo policy and stacks.
- Client/plugin/user code supplies schema migration transforms. Core validates/report incompatibilities, transforms all objects including Soft Deleted objects, validates all outputs, and commits schema plus object updates only when every result validates.
- Initialization may add missing Collections, schemas, and indexes from a Core Initialization Manifest. It never overwrites existing definitions. Updates require explicit versioned upgrades and expected versions.
- Query API uses JSON Pointer. V1 defines boolean composition, existence, structural equality/inequality, ordered numeric/string comparisons, contains/prefix/suffix/regex, exact array membership, object-key existence, explicit type errors, and missing/null behavior as described in the user stories.
- Sorting uses the total order missing, null, boolean, number, string, array, object. Booleans sort false before true; numbers numerically; strings lexicographically by Unicode scalar value; arrays lexicographically by recursively ordered elements; objects lexicographically by sorted key/value pairs. UUID breaks equal sort keys. Pagination is limit/offset.
- Indexes are non-unique, single-path JSON Pointer indexes that accelerate scalar values only. Unindexed values remain queryable. Schema and index changes use expected versions.
- Public API and schema/query behavior must stabilize before any client consumes Praxium. Client-hosted plugins may add application workflows, integrations, and defaults through the API; Core does not load plugins.

## Public Rust API Contract

- The repository root is one Rust library package. Source is layered as public API, domain rules, and a private storage adapter. Storage engines implement an abstract `StorageEngine` trait; SQLite is private to the adapter; no SQL or SQLite types appear in public signatures.
- A synchronous, cloneable `ObjectStore` (with `Store` alias) handle is safe to share across threads and backend-agnostic, accepting any `impl StorageEngine` via `ObjectStore::new(backend)`. Focused operation groups are exposed from the handle (collections, schemas, objects, queries, history, and transactions). A scoped transaction handle exposes the same operations, sees its own writes, and rejects nested transactions.
- Storage connection and initialization are separate. Backend-specific adapters (such as `SqliteStorage::open(path)`) create or open the storage and apply data-preserving internal storage migrations, but do not seed client defaults. `initialize(manifest)` applies missing collection/schema/index defaults atomically via `StorageEngine::seed_manifest` and returns an `InitializationReport` identifying seeded and preserved definitions, including their versions where applicable. Existing definitions are never replaced by normal initialization; later defaults require explicit versioned upgrades.
- Public JSON values use `serde_json::Value`. Query filters, sort keys, and page options are owned typed Rust values; v1 promises no serialized query wire format. Collection names, UUIDv7 object IDs, object revisions, schema/index versions, and JSON Pointers are distinct validated newtypes. Collection names are non-empty, contain no control characters, preserve exact text/case, and are not normalized. JSON Pointers follow RFC 6901 strictly; the empty pointer addresses the root. Parsed object IDs must be UUIDv7.
- Stored Object roots are JSON objects. Results return the JSON value with Core-owned collection, UUIDv7 ID, revision, creation time, update time, and optional deletion time. Timestamps use `SystemTime`. Object revisions start at 1 and increment once for every committed object state change.
- Query methods require explicit positive limit and offset and return the page plus the total match count from the same consistent database snapshot. An omitted sort still means deterministic ascending UUID order. Explicit sort keys are evaluated in priority order; descending reverses the full value order (including missing/null placement), while the final UUID tie-break is always ascending.
- Numeric comparisons use mathematical numeric equality and ordering, so equivalent integer/decimal values compare equal. For `any` and `all` membership over an array, empty candidate sets produce false and true respectively. Any encountered type mismatch fails the whole query, even if another boolean branch could determine the result. Query errors identify collection, object ID, JSON Pointer, and operator where available.
- Every public operation returns `Result`; expected invalid input and other expected failures never panic. The non-exhaustive typed error variants and structured fields are the machine contract; `Display` text may change. Categories cover invalid input, not found, revision/version conflict, validation, query, storage, transaction, and migration failures. Conflicts include expected and current values when available. Errors carry useful operation/context and standard source chains; SQLite-specific types stay private. Opening a database created by a newer unsupported Praxium version returns a distinct error and does not modify it.
- Validation without writing returns a report, including for invalid candidates. Writes return the same issues in a typed validation error. All issues are returned in stable order by instance pointer, then schema pointer, then keyword; each exposes machine-readable pointers/keyword and a human-readable message where available.
- A transaction callback returns the same public `Result<T, Error>` as other operations. Returning an error rolls back and returns that error. A panic rolls back during unwinding and propagates unchanged. Other concurrent calls see committed state before or after the transaction, never partial writes. Migration transforms are synchronous per-object callbacks, may return client-defined errors preserved as opaque sources, and are documented as deterministic and side-effect-free; external side effects cannot be rolled back.
- Schema and index definition versions start at 1 and increment on explicit versioned updates. Every schema change, including one compatible with all current objects, goes through the explicit migration flow. Index changes use an expected version, rebuild/update atomically as needed, and each collection may have at most one index per JSON Pointer path.

## Testing Decisions

- Main seam: public Rust API contract tests against temporary SQLite databases. Tests assert observable results and errors, not private module structure or SQL internals.
- Cover initialization and preservation of user definitions; Draft 2020-12 support/rejection; create/read/replace/patch validation; UUID/revision behavior and stale writes; Soft Delete/include-deleted/restore/Purge; history and revision restore; transaction commit and rollback; schema migration including deleted objects and all-or-nothing behavior; all query operators and type errors; deterministic sorting and pagination; and index lifecycle/versioning.
- Transaction tests verify both current object state and matching Change Entries after commit, and verify no partial object/history state remains after rollback.
- Query tests include structural equality, missing versus null, invalid regex, exact membership, incompatible types, mixed JSON types, UUID tie-breaks, and pagination edges.
- Migration tests include at least one invalid transform result and verify no schema, object, or history changes survive; successful migration tests verify all objects and shared transaction ID.
- No Cargo manifest, implementation, or automated test suite exists yet, so there is no local prior art. Establish the public API contract suite before adding lower-level tests.

## Out of Scope

- Task-management semantics, task fields/workflows, and the future task client.
- CLI, TUI, GUI, UI configuration, or rendering.
- Core-hosted plugin loader/runtime/ABI.
- Client-facing undo/redo stack and policy; Core provides history and restore primitives.
- Multi-user operation, synchronization, cloud/service hosting, and non-Rust language bridges.
- Alternate database backend implementations in v1.
- Composite/unique indexes or indexing array/object values.

## Further Notes

- The package name Praxium was checked against crates.io and was not found at the time of checking. Availability is not reserved and should be rechecked before publication.
- The agreed domain vocabulary, design contract, and durable architectural decisions are captured in the repository glossary, core design document, and ADRs.
- This spec records the v1 target. Implementation should begin only after the project owner explicitly starts it.
