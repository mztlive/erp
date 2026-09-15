//! MongoDB 连接、执行器、事务与通用仓储机械能力。

mod connection;
mod errors;
mod executor;
pub mod mongo_ops;
pub mod repository;
mod transaction;

pub use connection::{connect, ensure_transaction_support};
pub use errors::{Error, Result};
pub use executor::{Executor, NoTransaction};
pub use repository::{PageResult, Pagination, QueryFilter, Repository, insert_literal_regex_filter};
pub use transaction::Transactional;
