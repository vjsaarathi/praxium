//! Public API contracts and operation groups.

pub mod operation_groups;
pub mod store;

pub use operation_groups::{
    CollectionsOp, HistoryOp, ObjectsOp, QueryOp, SchemasOp, TransactionsOp,
};
pub use store::{IntoStorageEngine, ObjectStore, Store};
