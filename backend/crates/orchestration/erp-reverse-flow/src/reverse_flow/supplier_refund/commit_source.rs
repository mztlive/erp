use erp_returns::dto::CommitSupplierRefundRequest;
use persistence_core::Executor;

use super::*;

impl ReturnsProcess {
    // 读取精确原资金事实并按已有领域规则准备退款，不写入数据库。
    //
    // `req` 为一次创建并提交的请求。返回原付款 ID、读取到的版本，以及尚未写入的退款实体。原付款不存在、领域准备失败或仓储读取失败时返回对应错误。
    pub(super) async fn supplier_refund_commit_source(
        &self,
        req: &CommitSupplierRefundRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierPaymentId, u64, SupplierRefund)> {
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&req.source_fact_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商付款不存在".to_string()))?;
        let source_fact_id = SupplierPaymentId::new(payment.base.id.clone());
        let source_version = payment.base.version;
        let refund = new_supplier_refund_commit(
            req,
            SupplierRefundSourceFact {
                payment_id: source_fact_id.clone(),
                supplier_id: payment.supplier_id.clone(),
                amount: payment.amount,
            },
            actor.id(),
        )?;
        Ok((source_fact_id, source_version, refund))
    }
}
