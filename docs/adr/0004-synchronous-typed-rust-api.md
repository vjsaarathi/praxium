# Synchronous typed Rust API with private storage

Praxium exposes a synchronous, cloneable, thread-safe `Database` handle with typed operation groups and a non-exhaustive structured `Result` error contract; expected failures do not panic, while SQLite remains behind a private storage adapter. This keeps v1 direct for its local SQLite workload and gives Rust clients a stable machine contract without committing to an async runtime, SQLite types, or a serialized query wire format; transaction callbacks roll back on errors or unwinding panics, with panics propagated unchanged.
