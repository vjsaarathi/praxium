# Backend-agnostic ObjectStore and pluggable StorageEngine

The primary store interface is named `ObjectStore` (with `Store` as a type alias) rather than `Database`. `ObjectStore` is completely decoupled from any specific storage backend and is instantiated via `ObjectStore::new(backend)` over an abstract `StorageEngine` trait.

Connection, file path, and storage lifecycle management (such as `SqliteStorage::open(path)` or in-memory setups) belong strictly to concrete storage adapters. This prevents backend-specific parameters (such as file paths, connection strings, or pools) from leaking into the generic store interface, while keeping SQLite as the private default implementation for v1.
