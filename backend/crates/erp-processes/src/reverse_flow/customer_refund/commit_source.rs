use erp_audit::CommandReceiptServiceExt as _;
use erp_finance::service::receivable::customer_refund::load_customer_refund_source;
use erp_returns::service::ReturnsService;

use super::*;

impl ReturnsProcess {
    /// 已完成提交命令回放仍须通过当前来源与发起职责校验。
    pub(super) async fn customer_refund_commit_replay(
        &self,
        receipt: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<String>> {
        let Some(id) = receipt.committed_resource_id(&self.db).await? else {
            return Ok(None);
        };
        self.authorize_refund_replay(DocumentType::CustomerRefund, "customer_refund:submit", &id, actor)
            .await?;
        Ok(Some(id))
    }

    /// 读取精确原资金事实并按已有领域规则准备退款，不写入数据库。
    pub(super) async fn customer_refund_commit_source(
        &self,
        req: &CommitCustomerRefundRequest,
        actor: &AuditActor,
    ) -> Result<(CustomerReceiptId, u64, CustomerRefund)> {
        let receipt = load_customer_refund_source(&self.db, &req.source_fact_id, &mut NoTransaction).await?;
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
