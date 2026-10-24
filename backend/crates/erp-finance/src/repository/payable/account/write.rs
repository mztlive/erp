//! 应付进度条件更新使用财务仓储共用实现。

pub(crate) use crate::repository::progress::AccountProgressWrite as PayableAccountWriteExt;
pub(in crate::repository::payable) use crate::repository::progress::{amount_bson, progress_pipeline};
