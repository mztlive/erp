//! 客户退款的本域草稿、资金来源事实校验、累计额度与事务内写入。
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, CustomerReceiptId, CustomerRefundId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::ReturnsService;
use super::shared::{CUSTOMER_REFUND_COMMAND_PREFIX, DEFAULT_FINANCE_REVIEWER, return_command_no};
use crate::dto::{CommitCustomerRefundRequest, CreateCustomerRefundRequest};
use crate::entity::returns::{
    CumulativeAmountLimit, CustomerRefund, CustomerRefundData, CustomerRefundStatus,
};
use crate::repository::ReturnsExt;
use crate::{Error, Result};
/// 客户退款消费的原回款最小事实；读取时点由流程控制。
pub struct CustomerRefundSourceFact {
    /// 原回款身份。
    pub id: CustomerReceiptId,
    /// 当前版本。
    pub version: u64,
    /// 原资金总额。
    pub amount: Amount,
    /// 经营客户可空；退款在原位置拒绝缺项。
    pub customer_id: Option<CustomerAccountId>,
    /// 原回款是否已过账。
    pub is_posted: bool,
}
impl ReturnsService {
    /// 校验并构造客户退款草稿；保留身份、金额、双人复核及来源二选一规则。
    pub fn prepare_customer_refund(
        req: CreateCustomerRefundRequest,
        actor_id: &str,
    ) -> Result<CustomerRefund> {
        req.validate()?;
        let refund = CustomerRefund::new(
            CustomerRefundId::new(next_id()),
            CustomerRefundData {
                refund_no: req.refund_no,
                sales_return_case_id: req.sales_return_case_id,
                customer_id: req.customer_id,
                original_receipt_id: req.original_receipt_id,
                original_receivable_entry_id: req.original_receivable_entry_id,
                reason_code: req.reason_code,
                reason_text: req.reason_text,
                amount: req.amount,
                handled_by: req.handled_by,
                reviewed_by: req.reviewed_by,
                occurred_at: req.occurred_at,
                evidence_attachment_id: None,
            },
            actor_id,
        )?;
        Ok(refund)
    }
    /// 原回款预读成功后，在原时点生成一次提交的退款身份、业务号与发生时间。
    pub fn prepare_committed_customer_refund(
        req: &CommitCustomerRefundRequest,
        source: &CustomerRefundSourceFact,
        actor_id: &str,
    ) -> Result<CustomerRefund> {
        let source_fact_id = source.id.clone();
        let customer_id = source
            .customer_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("原回款未关联经营客户，不能退款".to_string()))?;
        let refund = CustomerRefund::new(
            CustomerRefundId::new(next_id()),
            CustomerRefundData {
                refund_no: return_command_no(CUSTOMER_REFUND_COMMAND_PREFIX, actor_id, &req.idempotency_key),
                sales_return_case_id: None,
                customer_id: customer_id.clone(),
                original_receipt_id: Some(source_fact_id.clone()),
                original_receivable_entry_id: None,
                reason_code: None,
                reason_text: req.reason.clone(),
                amount: req.amount.unwrap_or(source.amount),
                handled_by: actor_id.to_string(),
                reviewed_by: DEFAULT_FINANCE_REVIEWER.to_string(),
                occurred_at: Instant::now(),
                evidence_attachment_id: None,
            },
            actor_id,
        )?;
        Ok(refund)
    }
    /// 按主键读取客户退款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    pub async fn load_customer_refund(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerRefund> {
        self.db
            .customer_refunds()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))
    }
    /// 在调用方事务创建退款单；绑定和运行事实由流程保持原先后顺序。
    pub async fn create_customer_refund_in_transaction(
        &self,
        refund: &CustomerRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.customer_refunds().create(refund, executor).await?;
        Ok(())
    }
    /// 使用调用方执行器写回客户退款的原版本条件。
    pub async fn persist_customer_refund(
        &self,
        refund: &mut CustomerRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.customer_refunds().update(refund, executor).await?;
        Ok(())
    }
    /// 最终通过前读取并验证客户退款状态；已冲正错误优先于审批状态错误。
    pub async fn prepare_customer_refund_final_post(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerRefund> {
        let refund = self.load_customer_refund(id, executor).await?;
        if refund.status == CustomerRefundStatus::Reversed {
            return Err(Error::BusinessLogicError("已冲正退款不能再过账".to_string()));
        }
        super::approval::ensure_final_approve_posting(&refund)?;
        Ok(refund)
    }
    /// 仅支持原回款来源；按原应收分录退款保留原失败说明。
    pub fn customer_refund_receipt_id(refund: &CustomerRefund) -> Result<CustomerReceiptId> {
        refund
            .original_receipt_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("按原应收分录退款由冲减分录完成".to_string()))
    }
    /// 按退款独立累计数据源校验金额上限；不合并回款冲正累计。
    pub async fn validate_customer_refund_amount(
        &self,
        refund: &CustomerRefund,
        original_receipt_id: &CustomerReceiptId,
        receipt_amount: Amount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let refunded_before: Amount = self
            .db
            .customer_refunds()
            .posted_refund_total_by_receipt(original_receipt_id, &refund.base.id, executor)
            .await?;
        CumulativeAmountLimit::ensure_within_limit(receipt_amount, refunded_before, refund.amount)
            .map_err(|_| Error::BusinessLogicError("累计退款金额不得超过原回款金额".to_string()))
    }
    /// 财务写入全部成功后才标记已过账并执行退款CAS。
    pub async fn persist_posted_customer_refund(
        &self,
        refund: &mut CustomerRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        refund.mark_posted()?;
        self.persist_customer_refund(refund, executor).await
    }
    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("客户退款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }
}
/// 在创建退款的同一事务中复验原资金，版本错误必须早于Posted与缺客户错误。
pub fn ensure_customer_refund_source(source: &CustomerRefundSourceFact, expected_version: u64) -> Result<()> {
    super::shared::ensure_posted_source(
        source.version,
        expected_version,
        source.is_posted,
        "只有已过账的客户回款才能发起退款",
    )?;
    if source.customer_id.is_none() {
        return Err(Error::BusinessLogicError("原回款未关联经营客户，不能退款".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
    fn source() -> CustomerRefundSourceFact {
        CustomerRefundSourceFact {
            id: CustomerReceiptId::new("receipt-1"),
            version: 3,
            amount: amount("100"),
            customer_id: Some(CustomerAccountId::new("customer-1")),
            is_posted: true,
        }
    }
    /// 源版本优先于状态；状态优先于经营客户缺项，保持原错误分类和文案。
    #[test]
    fn refund_source_preserves_version_posted_customer_first_error_order() {
        let mut fact = source();
        fact.is_posted = false;
        fact.customer_id = None;
        assert!(
            matches!(ensure_customer_refund_source(&fact,2),Err(Error::ConflictError(message)) if message=="原资金记录已变化，请刷新后重试")
        );
        assert!(
            matches!(ensure_customer_refund_source(&fact,3),Err(Error::BusinessLogicError(message)) if message=="只有已过账的客户回款才能发起退款")
        );
        fact.is_posted = true;
        assert!(
            matches!(ensure_customer_refund_source(&fact,3),Err(Error::BusinessLogicError(message)) if message=="原回款未关联经营客户，不能退款")
        );
        fact.customer_id = Some(CustomerAccountId::new("customer-1"));
        ensure_customer_refund_source(&fact, 3).unwrap();
    }
    /// 一体提交预读只要求经营客户，不提前新增posted守卫；金额为空仍冻结原全额。
    #[test]
    fn refund_commit_preparation_keeps_default_amount_identity_and_delayed_source_gate() {
        let mut fact = source();
        fact.is_posted = false;
        let request = CommitCustomerRefundRequest {
            source_fact_id: "receipt-1".into(),
            amount: None,
            reason: "退款".into(),
            idempotency_key: " key-1 ".into(),
        };
        let refund = ReturnsService::prepare_committed_customer_refund(&request, &fact, "actor-1").unwrap();
        assert_eq!(refund.amount, fact.amount);
        assert_eq!(refund.customer_id, fact.customer_id.unwrap());
        assert_eq!(refund.original_receipt_id.as_ref(), Some(&fact.id));
        assert_eq!(refund.refund_no, return_command_no("TK", "actor-1", "key-1"));
        assert_eq!(refund.handled_by, "actor-1");
        assert_eq!(refund.reviewed_by, "finance_reviewer");
        assert_eq!(refund.status, CustomerRefundStatus::Draft);
        assert!(refund.evidence_attachment_id.is_none());
    }
    /// 经营客户缺项仍在退款实体和金额规则之前拒绝。
    #[test]
    fn refund_commit_missing_customer_precedes_invalid_amount() {
        let mut fact = source();
        fact.customer_id = None;
        let request = CommitCustomerRefundRequest {
            source_fact_id: "receipt-1".into(),
            amount: Some(amount("0")),
            reason: "退款".into(),
            idempotency_key: "key-1".into(),
        };
        assert!(
            matches!(ReturnsService::prepare_committed_customer_refund(&request,&fact,"actor-1"),Err(Error::BusinessLogicError(message)) if message=="原回款未关联经营客户，不能退款")
        );
    }
}
