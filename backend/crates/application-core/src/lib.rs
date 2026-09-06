//! 应用合同：分页、命令身份/回执、错误分类与调用人数据。

pub mod command;
pub mod context;
mod error;
pub mod owned_task;
pub mod page;
pub mod query;

pub use command::{
    CommandFingerprint, CommandIdentity, CommandReceipt, CommandReceiptFact, CommandReceiptMatch,
};
pub use context::AuditActor;
pub use error::{Error, ErrorClass, Result};
pub use owned_task::await_owned;
pub use page::Page;
pub use query::{
    non_blank, normalize_sort, normalized_text, page_or_default, page_size_or_default, PageView, SortDir,
};
