//! 应收进度条件更新使用财务仓储共用实现。

pub(crate) use crate::repository::progress::AccountProgressWrite as ReceivableAccountWriteExt;
pub(in crate::repository::receivable) use crate::repository::progress::{amount_bson, progress_pipeline};
