//! 批准额度内使用供应商留抵的请求合同。
use erp_core::ids::WorkItemId;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseSupplierCreditRequest {
    pub work_item_id: WorkItemId,
    pub task_version: String,
    pub expected_version: u64,
    pub credit_id: String,
    pub payable_account_id: String,
    pub payable_entry_id: String,
    pub amount: Amount,
    pub idempotency_key: String,
}

impl UseSupplierCreditRequest {
    /// 验证身份、金额及版本的输入边界。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整且金额为正时成功。
    /// # 错误
    /// 空身份、非法版本和非正金额时拒绝。
    pub fn validate(&self) -> Result<()> {
        if [&self.credit_id, &self.payable_account_id, &self.payable_entry_id, &self.idempotency_key]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 128)
            || self.work_item_id.as_ref().trim().is_empty()
            || self.expected_version == 0
            || self.task_version.parse::<u64>().ok().is_none_or(|v| v == 0)
            || self.amount <= Amount::zero()
        {
            return Err(Error::from("请核对抵扣来源、金额及当前任务版本后提交"));
        }
        Ok(())
    }
}
