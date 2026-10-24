//! 应用合同：分页、命令身份/回执、错误分类与调用人数据。

pub mod command;
pub mod context;
mod error;
mod error_macros;
pub mod owned_task;
pub mod page;
pub mod query;
mod query_ids;
pub mod scope_version;
mod structured_receipt;
pub use command::{CommandFingerprint, CommandIdentity, CommandReceipt};
pub use context::AuditActor;
pub use error::{Error, ErrorClass, Result};
pub use owned_task::await_owned;
pub use page::Page;
pub use query::{
    DEFAULT_PAGE, DEFAULT_PAGE_SIZE, DEFAULT_SORT_DIR, DEFAULT_SORT_FIELD, MAX_PAGE_SIZE, PageParams,
    PageView, SortDir, non_blank, normalize_paging, normalize_sort, normalized_text, page_or_default,
    page_size_or_default,
};
pub use query_ids::{FilterOption, OwnershipPage, QueryIds};
pub use scope_version::{EmptyScopeVersion, ScopeVersion, ScopeVersionError};
pub use structured_receipt::{StructuredCommandReceipt, StructuredReceiptMatch};

pub mod directory;

mod validation;
pub use validation::ValidationIssues;
