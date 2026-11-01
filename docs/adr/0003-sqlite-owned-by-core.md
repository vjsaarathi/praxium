# SQLite persistence owned by Core

Core owns an embedded SQLite database for JSON objects, schemas, indexes, revisions, and history so transactions and query behavior stay consistent across clients. SQLite remains an internal detail behind the Rust API, with data-preserving migrations allowed as the implementation evolves.
