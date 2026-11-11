//! 财务纠错原单编辑的经办岗位分离与草稿状态。

use crate::{Error, Result};

/// 财务纠错单据的草稿编辑约束。
pub(crate) struct FinancialDraftEditPolicy;

impl FinancialDraftEditPolicy {
    /// 校验经办人与复核人分离，不依赖提交后的单据状态。
    ///
    /// # 参数
    /// * `handled_by` - 原单经办人。
    /// * `reviewed_by` - 原单复核人。
    ///
    /// # 返回
    /// 经办人与复核人不同时返回成功。
    ///
    /// # 错误
    /// 经办人兼任复核人时返回 `Forbidden`。
    pub(crate) fn ensure_separation(handled_by: &str, reviewed_by: &str) -> Result<()> {
        if handled_by == reviewed_by {
            return Err(Error::Forbidden("财务经办人不得兼任复核人".into()));
        }
        Ok(())
    }

    /// 校验经办复核分离与草稿状态。
    ///
    /// # 参数
    /// * `handled_by` - 原单经办人。
    /// * `reviewed_by` - 原单复核人。
    /// * `is_draft` - 原单当前是否为草稿或已驳回。
    ///
    /// # 返回
    /// 经办人与复核岗位分离且状态可编辑时返回成功。
    ///
    /// # 错误
    /// 岗位未分离返回权限错误，非草稿或已驳回返回状态冲突。
    pub(crate) fn ensure(handled_by: &str, reviewed_by: &str, is_draft: bool) -> Result<()> {
        Self::ensure_separation(handled_by, reviewed_by)?;
        if !is_draft {
            return Err(Error::ConflictError("只有草稿或已驳回单据可以修改".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_edit_requires_separation_and_editable_state() {
        assert!(FinancialDraftEditPolicy::ensure("handler", "reviewer", true).is_ok());
        assert!(matches!(
            FinancialDraftEditPolicy::ensure("handler", "reviewer", false),
            Err(Error::ConflictError(_))
        ));
        for is_draft in [true, false] {
            assert!(matches!(
                FinancialDraftEditPolicy::ensure("handler", "handler", is_draft),
                Err(Error::Forbidden(_))
            ));
        }
    }
}
