//! 指定记录读取授权；来源终止不可恢复，任务办理资格不由读取授权产生。
pub mod event;
mod source;
pub use source::{GrantSource, GrantTarget, ResourceGrant, ResourceKind, SourceKey, SourceUpdate};

pub(crate) mod rows;
