# Current state with change history

The current object row is authoritative; append-only after-change snapshots provide history and restore points without making the database event-sourced. This keeps normal reads direct while allowing clients to implement undo/redo by issuing new revision-checked restores; purge removes both current state and per-object history.
