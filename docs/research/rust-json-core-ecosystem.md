# Rust JSON core ecosystem comparison

**Reviewed:** 2026-11-01  
**Compared against:** [Praxium Core v1](../design/core-v1.md)  
**Scope:** DocLite and XDB, using project-owned documentation, published crate documentation, and repository manifests. This is a fit assessment, not an implementation or security audit.

## Executive summary

There are existing Rust projects that cover meaningful slices of Praxium's proposed capabilities. **DocLite is the closest match for embedded document storage with indexing, optimistic concurrency, history, and atomic batches. XDB is the closest match for JSON-valued named collections in SQLite with local-first operation and transactions.** Neither documented API matches the full Praxium contract.

DocLite's central model is a Rust struct that supplies the record schema, index declarations, and generated query surface. It exposes a lower-level engine API, but that API stores opaque bytes and index rows rather than runtime-defined JSON objects. XDB stores JSON values in SQLite collections and supports transactions and soft-delete tombstones, but its documented Rust API has no schema validation, per-object revision history, or portable server-side JSON query contract. Its app-facing `useFind` API filters and sorts locally.

**Practical implication:** Praxium is not reinventing all underlying storage primitives. Before implementation, it would be useful to decide whether to reuse or adapt DocLite's low-level storage ideas, XDB's SQLite patterns, or simply use SQLite directly. Replacing Praxium wholesale with either project would require changing or adding several defining v1 capabilities.

## Fit against Praxium v1

| Requirement | DocLite | XDB |
|---|---|---|
| Runtime-defined, user-editable JSON Schema Draft 2020-12 | **No documented support.** A Rust struct is the schema; derive macros use it to define indexes and typed queries. JSON is an optional storage encoding for that typed record. | **No documented support.** The Rust crate stores JSON records; a `.xdb` bundle file is described as a JSON schema/seed document, not as a runtime JSON Schema validator or versioned collection schema API. |
| SQLite owned by the library | **Partial.** `doclite-sqlite` is an optional durable engine, alongside a memory engine; `doclite-core` is storage-engine independent. | **Yes.** The Rust library persists named JSON collections in SQLite. |
| Generic JSON Pointer filters, ordering, pagination | **No.** The typed layer generates queries for declared Rust fields and indexes, with equality and range operations. The low-level core scans index rows; it is not a JSON query engine. | **Partial.** The React `useFind` hook documents local filters, sort, and pagination. The listed Rust database methods expose collection reads but no portable JSON path query language. |
| Declared single-path scalar indexes | **Partial.** Indexes are declared on typed fields at compile time; compound indexes and unique constraints are also supported. This does not match runtime JSON Pointer index declarations. | **Not documented.** The project documentation does not describe user-declared JSON-path indexes. |
| Stale-write protection with monotonically increasing object revision | **Yes, in a different model.** Updating creates a superseding revision and stale records are rejected. IDs and revision representation differ from Praxium's UUIDv7 plus externally managed integer revision contract. | **No documented equivalent.** Records have UUID IDs and timestamps; no per-object revision precondition appears in the documented CRUD API. The project does reject certain stale peer updates using synchronization epochs, which is a different concern. |
| Full after-change history; revision/group restore | **Partial.** Revisions link to predecessors and history can walk them. No documented API for restoring an object or a transaction group to prior state. | **No.** The docs describe current records and CRDT state, not per-object CRUD change history or revision restore. |
| Atomic transactions / batches | **Yes.** A transaction value groups inserts and updates and commits all-or-none. | **Yes.** `with_transaction`, imports, and multi-record updates are documented as transactional/all-or-nothing. |
| Soft delete and hard purge semantics | **Not documented as supported.** The documented typed operations emphasize insert/update/history. | **Partial.** `delete_record` creates a retained tombstone hidden from collection reads; collection clear is a local hard reset. There is no documented per-object restore-plus-history purge contract. |
| Versioned collection schemas and atomic object migrations | **No documented support.** Rust types act as schemas; no runtime schema-version or per-object transform API is documented. | **No documented support.** No retained JSON Schema versions or atomic schema migration operation is listed. |
| Rust library suitable for embedding | **Yes.** Layered crates expose both a typed document API and a low-level engine API. | **Yes, with host-specific context.** The crate is used by Softn's Rust server and desktop runtime; it can be built without Tauri features. |

### DocLite details

DocLite describes its typed layer as deriving each record's schema, indexes, query builder, and revision history from a Rust struct. A field can be indexed or unique, and compound indexes are supported. JSON is one storage encoding; the JSON body sits in an envelope that records document ID, revision, timestamp, and predecessor. The documented history operation walks prior revisions, and stale updates fail rather than overwrite a newer revision. See the [published DocLite crate documentation](https://docs.rs/crate/doclite-derive/latest) and [SQLite engine API](https://docs.rs/doclite-sqlite/latest/doclite_sqlite/index.html).

The separate `doclite-core` layer is deliberately generic about storage: operations apply preconditions and mutations to opaque object bytes plus index rows. Its scan contract focuses on declared index ordering. This is a potentially useful design reference for atomic operation batches and storage abstraction, but it does not supply Praxium's runtime JSON Schema, arbitrary JSON Pointer query semantics, schema migration, soft delete, or restore APIs. The same docs describe conformance testing across memory and SQLite engines and CI checks; the published crate is version 0.1.0, dated 2026-09-06, and Apache-2.0 licensed. These indicate a carefully documented early release, not a mature stable substitute.

### XDB details

XDB's Rust crate stores JSON records with `id`, `collection`, `data`, timestamps, and a `deleted` flag. Collections are created when records are written. Updates shallow-merge object fields; deletes write tombstones; normal collection reads hide tombstones. SQLite transactions group record changes, and the project also supports import/export and optional peer sync. The repository says the Rust crate is used by Softn's desktop runtime and Rust server. See the [XDB repository README](https://github.com/f2i-com/xdb.org#readme), [Cargo workspace manifest](https://github.com/f2i-com/xdb.org/blob/main/Cargo.toml), and [MIT license](https://github.com/f2i-com/xdb.org/blob/main/LICENSE).

For query behavior, the README documents React `useFind` filtering, sorting, and pagination, but the Rust API table lists collection-level reads rather than a core query language. The README also calls out imports, multi-record writes, and nested savepoint behavior as transactional. It does not document runtime JSON Schema validation, index declarations, object revisions with stale-write checks, CRUD history, per-object restore, or schema migrations. The workspace manifest selects UUID v4, while Praxium requires UUIDv7. The repository is a live dependency of its author's products, but its GitHub page reports 16 commits, one star, and no published GitHub releases; the Rust dependency example is a local path dependency. Those are modest public maturity signals, not a measure of correctness.

## Decision notes

- **Do not treat either as a drop-in Praxium implementation** based on the public contracts reviewed here.
- **DocLite is the more relevant design/code reference** for atomic batches, preconditions, storage abstraction, and append-by-supersession history. Its field/type-driven public API conflicts with Praxium's runtime schema and generic JSON query boundary.
- **XDB is the more relevant reference** for local SQLite storage, named collections, JSON record envelopes, tombstones, and transactional batching. Its product scope also includes sync and host integrations, while it lacks the documented schema/query/history contract Praxium needs.
- **A narrower reuse spike could still be worthwhile** if implementation later shows that either project's storage engine can be consumed without importing its incompatible domain model. This report did not build either crate or inspect runtime behavior.
- **Nitrite was not included in the detailed comparison** because the first-party project description identifies Fjall rather than SQLite as its persistent backend, making it less relevant to this storage decision.

## Sources

- [Praxium Core v1 contract](../design/core-v1.md) (repository source of requirements).
- [DocLite derive crate documentation](https://docs.rs/crate/doclite-derive/latest) (typed schema/index/query model, revision/history, batch writes, engine overview, release and license details).
- [DocLite SQLite engine API](https://docs.rs/doclite-sqlite/latest/doclite_sqlite/index.html) (SQLite engine crate).
- [DocLite record module](https://docs.rs/doclite/latest/doclite/record/index.html) (typed record/revision API).
- [XDB repository README](https://github.com/f2i-com/xdb.org#readme) (data model, Rust API, query hooks, transactions, integrations, current usage, repository metadata).
- [XDB Cargo workspace manifest](https://github.com/f2i-com/xdb.org/blob/main/Cargo.toml) (version, MIT license, SQLite and UUID dependencies).
- [XDB license](https://github.com/f2i-com/xdb.org/blob/main/LICENSE).

