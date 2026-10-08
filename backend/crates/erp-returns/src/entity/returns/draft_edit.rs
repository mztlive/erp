//! 财务纠错原单编辑的经办岗位与草稿状态资格。

use crate::{Error, Result};

/// 财务纠错单据的草稿编辑资格。
pub(crate) struct FinancialDraftEditPolicy;

impl FinancialDraftEditPolicy {
    /// 校验原经办人发起命令的职责，不依赖提交后的单据状态。
    ///
    /// # 参数
    /// * `handled_by` - 原单经办人。
    /// * `reviewed_by` - 原单复核人。
    /// * `actor_id` - 当前提交人。
    ///
    /// # 返回
    /// 原经办人本人提交且与复核人分离时返回成功。
    ///
    /// # 错误
    /// 提交人为空、不是原经办人或兼任复核人时返回 `Forbidden`。
    pub(crate) fn ensure_submitter(handled_by: &str, reviewed_by: &str, actor_id: &str) -> Result<()> {
        if actor_id.is_empty() || handled_by != actor_id || reviewed_by == actor_id {
            return Err(Error::Forbidden("仅原财务经办人可以提交冲正，且不得兼任复核人".into()));
        }
        Ok(())
    }

    /// 校验原经办人、经办复核分离与草稿状态。
    ///
    /// # 参数
    /// * `handled_by` - 原单经办人
    /// * `reviewed_by` - 原单复核人
    /// * `actor_id` - 当前编辑人
    /// * `is_draft` - 原单当前是否为草稿
    ///
    /// # 返回
    /// 经办本人编辑草稿且与复核岗位分离时返回成功。
    ///
    /// # 错误
    /// 非原经办人返回权限错误，非草稿返回状态冲突。
    pub(crate) fn ensure(handled_by: &str, reviewed_by: &str, actor_id: &str, is_draft: bool) -> Result<()> {
        if actor_id.is_empty() || handled_by != actor_id || reviewed_by == actor_id {
            return Err(Error::Forbidden("仅原财务经办人可以修改本单据草稿".into()));
        }
        if !is_draft {
            return Err(Error::ConflictError("请先将驳回单据撤回草稿后再修改".into()));
        }
        Ok(())
    }
}
