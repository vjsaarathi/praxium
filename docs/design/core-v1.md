# Praxium Core v1 Design

## Purpose and boundary

Praxium is a Rust library for storing, validating, querying, and tracking changes to JSON objects. It is intended to be reusable by applications in different domains. It does not define task concepts or attach meaning to object fields. A future task client can provide a `tasks` collection and interpret its objects, but belongs in a separate repository.

The Core owns SQLite persistence, schema validation, generic queries, identity, revisions, change history, and transaction behavior. Clients own application workflows, configuration and wiring. Client-hosted extensions may contribute workflows, integrations, and default collection definitions through the public Rust API. Core does not host a plugin runtime or user interface. A language bridge can be added when a non-Rust client requires one.

The operating model is local-first, single-user, and offline-capable. Synchronization is outside v1. The public Rust API and query/schema behavior must be stable before clients adopt the library. SQLite internals may change through data-preserving migrations.

## Collections, schemas, and objects

A database contains named Collections. Each Collection has one active JSON Schema Draft 2020-12 schema and immutable retained schema versions. Schema documents declare their `$schema`; unsupported dialects are rejected. Schema definitions and index definitions are managed separately.

Stored Objects are JSON objects validated against the active schema. Core does not reserve domain fields. Each object has a Core-generated UUIDv7 ID and a monotonically increasing revision, along with creation/update timestamps, stored separately from the customizable JSON value. Every state-changing object operation supplies the current revision; stale revisions fail without changing state.

Clients may pass a Core Initialization Manifest containing collection, schema, and index defaults. Initialization seeds definitions that do not exist and preserves existing user-managed definitions. New client/plugin defaults do not silently replace existing definitions; changes are proposed as explicit versioned upgrades using expected schema/index versions.

## Public operations

The v1 API covers collection and schema management, object creation and reads, full replacement, JSON Patch, soft delete, restore, hard purge, history reads, revision restore, generic filtering/sorting/pagination, explicit schema migration, indexes, and atomic multi-object transactions. It contains no task-specific operations.

Create, replace, patch, delete, restore, and purge are state changes guarded by the expected object revision. Replace and JSON Patch both require the current revision. Soft-deleted objects are omitted from normal queries; clients may explicitly include them in reads. Restore makes a soft-deleted object visible again. Purge irreversibly erases the object and all its per-object history, leaving no marker.

Transactions group multiple writes atomically. Either all writes and their history entries commit or none do. Each transaction has an ID shared by its Change Entries. Clients manage user-facing undo/redo; Core exposes history and atomic restore to an earlier object revision or transaction group. A restore is itself a new change. If the selected snapshot fails validation under the active schema, Core rejects the restore and leaves the current object unchanged.

## Rust API and error contract

The repository root is one Rust library package with layered public API, domain, and private storage modules. The public API uses `serde_json::Value`, is synchronous, and exposes a cloneable `ObjectStore` (or `Store`) handle that can be shared safely across threads. It is storage-backend agnostic via an abstract `StorageEngine` trait, constructed via `ObjectStore::new(backend)`. Focused operation groups are available from the handle for collections, schemas, objects, queries, history, and transactions. Storage implementations (such as SQLite) remain private behind storage adapters.

Storage connection and initialization are separate. Backend-specific adapters (such as `SqliteStorage::open(path)`) create or open the underlying storage and apply internal data-preserving migrations without seeding client defaults. `initialize(manifest)` is an explicit atomic operation on the store that validates Draft 2020-12 schemas, adds only missing collection/schema/index definitions via the engine's `seed_manifest`, and returns seeded/preserved entries and their versions. Normal initialization never overwrites existing definitions.

Public names, IDs, revisions, schema/index versions, and JSON Pointers use distinct validated newtypes. Collection names are non-empty, reject control characters, and preserve exact text and case without normalization. JSON Pointers strictly follow RFC 6901, including the empty root pointer. Object ID parsing enforces UUIDv7. Stored JSON roots must be objects; object results include the value and Core metadata (collection, ID, revision, creation/update times, optional deletion time). Timestamps use `SystemTime`; object revisions start at 1 and advance once per committed object state change.

Filter, sort, and page inputs are typed owned Rust values with no v1 wire-format promise. Every query specifies a positive limit and offset and returns matching objects plus the total count from the same consistent snapshot. Missing paths sort before null in ascending order, then use the documented total JSON order. Descending reverses the complete value order; the final UUID tie-break remains ascending. Numeric comparisons use mathematical value semantics. Empty membership candidates mean `any = false`, `all = true`. Any encountered incompatible type errors the whole query, even if boolean short-circuiting could otherwise decide that row; the error identifies the collection, object, path, and operator where available.

All public operations use `Result` and expected failures do not panic. A non-exhaustive typed error's variants/fields are the stable machine contract; human-readable `Display` text may change. Errors distinguish invalid input, not found, revision/version conflicts, validation, query, storage, transaction, and migration failures; conflicts report expected/current values where available. They include operation/context and source chains while keeping SQLite details private. Validation returns all issues in deterministic instance-pointer, schema-pointer, keyword order, with machine-readable locations and a human-readable message where available. Opening storage with a newer unsupported storage version returns a distinct unsupported-newer-database error without modifying the database.

Transaction callbacks use the same `Result<T, Error>` convention. Returned errors roll back; panics roll back during unwinding and propagate unchanged. The transaction handle has the same operation groups, reads its own writes, and rejects nesting. Other calls see only committed state. Migration transforms are synchronous callbacks over JSON values; their client-defined error is retained as an opaque source. They must be deterministic and side-effect-free; external side effects are outside database rollback.

## Change history

The current object row is authoritative. Change History is an append-only history of successful object mutations, not an event-sourced source of truth. Each Change Entry stores the operation, resulting revision, timestamp, source, transaction ID, and full after-change JSON snapshot when one exists. Read operations are not logged. Purge erases the object and its history.

## Schema evolution

Clients, plugins, or users supply object transformation logic for an explicit Schema Migration. Core checks the expected schema version, validates the proposed schema, applies the transform to every stored object including soft-deleted objects, and validates every result. The schema activation and all transformed objects commit atomically only if every result validates. Otherwise, nothing changes. Core retains immutable schema versions. Transformed objects receive ordinary Change Entries grouped under the migration transaction ID.

## Portable query contract

Queries address values through JSON Pointer paths. Filters support boolean composition, path existence, recursive JSON equality/inequality, ordered comparisons, string predicates, array membership, and object-key existence. The v1 filter vocabulary includes:

- `exists(path)`;
- recursive JSON `eq` and `ne` (object key order is ignored; array order is significant);
- `lt`, `lte`, `gt`, and `gte` for number-to-number or string-to-string comparisons;
- case-sensitive string `contains`, `prefix`, `suffix`, and regex matching using Rust regex syntax;
- exact JSON-value array membership, including any/all membership predicates;
- object key-existence predicates; and
- boolean `and`, `or`, and `not` composition.

Missing paths do not match value comparisons, including `ne`. An explicit `null` matches `eq null`, and `exists(path)` distinguishes an explicit null from a missing path. A present value with an incompatible type for an operator produces a query error. Invalid regex patterns produce a query error before result evaluation. Strings are matched as exact Unicode text without implicit case folding or normalization.

Numeric equality and ordering use mathematical values (for example, `1` and `1.0` compare equal). Empty membership candidate sets produce false for `any` and true for `all`. A query type mismatch encountered for any evaluated row fails the query even when another boolean branch could otherwise decide that row; query errors include collection, object ID, pointer, and operator when available.

Sorting uses this total order: missing, null, booleans, numbers, strings, arrays, objects. Booleans order false before true; numbers compare numerically; strings compare lexicographically by Unicode scalar value; arrays compare lexicographically by recursively ordered elements; objects compare lexicographically by sorted key/value pairs, with values compared recursively. Equivalent sort keys are broken by object UUID. Pagination uses limit/offset with deterministic ordering.

Multiple sort keys are applied in priority order. Descending reverses the full value order, including missing/null positions, but the final UUID tie-break stays ascending. Queries require a positive limit and explicit offset and return total count from the same consistent snapshot; a query with no sort keys uses ascending UUID order.

## Indexes

Clients declare non-unique indexes on single JSON Pointer paths. Indexes accelerate scalar values only; arrays and objects remain queryable but are not indexed. Queries over unindexed paths remain valid. Index definitions have expected versions for updates and are separate from schema validation. Adding/changing defaults to an existing collection requires an explicit versioned upgrade.

## Out of scope for v1

Task workflows and task-specific field meanings, a CLI/TUI/GUI, plugin loading/runtime, multi-user access, synchronization, and non-Rust language bridges are outside this library's initial scope.
