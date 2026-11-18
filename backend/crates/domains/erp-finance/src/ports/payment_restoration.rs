//! 原付款申请占用与执行责任的恢复能力；应用连接财务编排。

use application_core::AuditActor;
use async_trait::async_trait;
use persistence_core::Executor;

use crate::entity::payable::{PaymentRequestAllocation, SupplierPayment};

/// 以原付款和核销分配恢复同一申请；不得新开事务或重放冲正。
#[async_trait]
pub trait PaymentRequestRestorationPort: Send + Sync {
    /// 消费方持有的错误类型，保留授权、审批码与未知提交的原始分类。
    type Error: std::error::Error + Send + Sync;

    /// 在调用方事务内重验原付款申请，并恢复净付款、占用和任务责任。
    /// 结果：原付款申请及其执行责任恢复成功时返回。
    /// 失败条件：金额、来源、账户、版本或责任资格不一致时返回原财务错误。
    ///
    /// 输入约定：
    /// - `payment`：原事务已经核验的付款事实。
    /// - `allocations`：原付款申请的精确分配。
    /// - `actor`：当前冲正操作人。
    /// - `executor`：冲正命令的同一执行器。
    async fn restore(
        &self,
        payment: &SupplierPayment,
        allocations: &[PaymentRequestAllocation],
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> std::result::Result<(), Self::Error>;
}
