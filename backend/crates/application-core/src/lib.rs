//! 应用合同：分页、命令身份/回执、错误分类与调用人数据。

pub mod command;
pub mod context;
mod error;
pub mod owned_task;
pub mod page;
pub mod query;
mod query_ids;
pub use command::{
    CommandFingerprint, CommandIdentity, CommandReceipt, CommandReceiptFact, CommandReceiptMatch,
};
pub use context::AuditActor;
pub use error::{Error, ErrorClass, Result};
pub use owned_task::await_owned;
pub use page::Page;
pub use query::{
    DEFAULT_PAGE, DEFAULT_PAGE_SIZE, DEFAULT_SORT_DIR, DEFAULT_SORT_FIELD, MAX_PAGE_SIZE, PageView, SortDir,
    non_blank, normalize_sort, normalized_text, page_or_default, page_size_or_default,
};
pub use query_ids::{FilterOption, FilteredPage, QueryIds};
