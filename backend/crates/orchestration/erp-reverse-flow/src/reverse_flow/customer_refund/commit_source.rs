use erp_finance::service::receivable::customer_refund::load_customer_refund_source;
use erp_returns::dto::CommitCustomerRefundRequest;
use erp_returns::service::ReturnsService;
use persistence_core::Executor;

use super::*;

impl ReturnsProcess {
    /// 读取精确原资金事实并按已有领域规则准备退款，不写入数据库。
    ///
    /// # 参数
    /// * `req` - 一次创建并提交的请求。
    /// * `actor` - 已通过鉴权的审计操作人。
    ///
    /// # 返回
    /// 返回原回款 ID、读取到的版本，以及尚未写入的退款实体。
    ///
    /// # 错误
    /// 原回款不存在、领域准备失败或仓储读取失败时返回对应错误。
    pub(super) async fn customer_refund_commit_source(
        &self,
        req: &CommitCustomerRefundRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerReceiptId, u64, CustomerRefund)> {
        let receipt = load_customer_refund_source(&self.db, &req.source_fact_id, executor).await?;
        let source_fact_id = CustomerReceiptId::new(receipt.base.id.clone());
        let source_version = receipt.base.version;
        let refund = ReturnsService::prepare_committed_customer_refund(
            req,
            &customer_refund_source_fact(&receipt),
            actor.id(),
        )?;
        Ok((source_fact_id, source_version, refund))
    }
}
