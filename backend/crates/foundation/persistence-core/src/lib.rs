//! MongoDB 连接、执行器、事务与通用仓储机械能力。

pub mod aggregate;
pub mod change_feed;
mod connection;
pub mod dirty_marks;
mod error_macros;
mod errors;
mod executor;
pub mod indexes;
pub mod mongo_ops;
pub mod repository;
mod reset;
#[cfg(feature = "test-support")]
pub mod testing;
mod transaction;
pub use connection::{connect, ensure_transaction_support};
pub use errors::{Error, Result};
pub use executor::{ExecutionBoundary, Executor, NoTransaction, ReadExecutor, SessionAccess};
pub use repository::{
    PageResult, Pagination, QueryFilter, Repository, insert_literal_regex_filter, persisted_document,
};
pub use reset::{DatabaseReset, ResetRetention};
pub use transaction::Transactional;

pub mod rotation;
