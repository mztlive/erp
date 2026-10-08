use erp_returns::ReturnsCommandReceiptService;

use super::*;

impl ReturnsProcess {
    /// 已完成提交命令回放仍须通过当前来源与发起职责校验。
    ///
    /// # 参数
    /// * `receipt` - 原提交命令收据。
    /// * `actor` - 回放操作人。
    ///
    /// # 返回
    /// 已有同载荷提交且授权通过时返回退款单 ID；无收据时返回 `None`。
    ///
    /// # 错误
    /// 收据读取失败或回放授权失败时返回对应错误。
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
    ///
    /// # 参数
    /// * `req` - 一次创建并提交的请求。
    /// * `actor` - 已通过鉴权的审计操作人。
    ///
    /// # 返回
    /// 返回原付款 ID、读取到的版本，以及尚未写入的退款实体。
    ///
    /// # 错误
    /// 原付款不存在、领域准备失败或仓储读取失败时返回对应错误。
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
