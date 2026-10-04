use erp_returns::ReturnsCommandReceiptService;

use super::*;

impl ReturnsProcess {
    /// 已完成提交命令回放仍须通过当前来源与发起职责校验。
    pub(super) async fn supplier_refund_commit_replay(
        &self,
        receipt: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<String>> {
        let Some(id) = ReturnsCommandReceiptService::new(self.db.clone())
            .committed_resource_id(receipt, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        self.authorize_refund_replay(DocumentType::SupplierRefund, "supplier_refund:submit", &id, actor)
            .await?;
        Ok(Some(id))
    }

    /// 读取精确原资金事实并按已有领域规则准备退款，不写入数据库。
    pub(super) async fn supplier_refund_commit_source(
        &self,
        req: &CommitSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<(SupplierPaymentId, u64, SupplierRefund)> {
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
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
