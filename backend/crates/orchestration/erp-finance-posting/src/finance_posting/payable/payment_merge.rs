//! 旧应付直付任务不能合并；首批实际付款限定一份已批准申请。
use application_core::AuditActor;
use erp_finance::dto::payment_merge::{PaymentMergeCandidatesParams, PaymentMergeCandidatesView};

use super::PayableProcess;
use crate::{Error, Result};
impl PayableProcess {
    /// 停止旧应付任务合并入口。
    /// # 参数
    /// * `params` - 旧任务筛选。
    /// * `actor` - 当前用户。
    /// # 返回
    /// 不返回可付款候选。
    /// # 错误
    /// 固定拒绝并要求走采购付款申请。
    pub async fn payment_merge_candidates(
        &self,
        _params: PaymentMergeCandidatesParams,
        _actor: &AuditActor,
    ) -> Result<PaymentMergeCandidatesView> {
        Err(Error::ConflictError("旧采购应付合并直付入口已停止，请从已批准付款申请登记付款".into()))
    }
}
