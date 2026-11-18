//! 客户退款的本域草稿、资金来源事实校验、累计额度与事务内写入。
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, CustomerReceiptId, CustomerRefundId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::ReturnsService;
use super::shared::{
    CUSTOMER_REFUND_COMMAND_PREFIX, DEFAULT_FINANCE_REVIEWER, ensure_cumulative_within, or_not_found,
    reject_if_reversed, return_command_no,
};
use crate::dto::{CommitCustomerRefundRequest, CreateCustomerRefundRequest};
use crate::entity::returns::{CustomerRefund, CustomerRefundData, CustomerRefundStatus};
use crate::repository::ReturnsExt;
use crate::repository::prelude::*;
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
    /// 返回初始草稿退款单。
    /// 请求字段未通过 `Validate` 时返回 `ValidationError`；编号、原因、经办复核人、创建人、金额或原事实二选一不满足时返回 `Logic`。
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
    /// 返回指向该原回款的草稿退款单；省略金额时沿用原回款金额。
    /// `req` 为一次提交命令；`source` 为原回款窄事实。
    /// `actor_id` 为经办人，同时作为创建人。
    /// 原回款没有经营客户时返回 `BusinessLogicError`；实体不变量不满足时返回 `Logic`。
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
                customer_id,
                original_receipt_id: Some(source_fact_id),
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
    /// 返回读到的退款单。
    /// `id` 为退款单主键；沿用调用方执行器 `executor`。
    /// 不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
    pub async fn load_customer_refund(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerRefund> {
        or_not_found(self.db.customer_refunds().find_by_id(id, executor).await?, "客户退款单不存在")
    }
    /// 在调用方事务创建退款单；绑定和运行事实由流程保持原先后顺序。
    /// `refund` 为待写入的退款单；沿用调用方执行器 `executor`。
    /// 传播仓储写入错误。
    pub async fn create_customer_refund(
        &self,
        refund: &CustomerRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.customer_refunds().create(refund, executor).await?;
        Ok(())
    }
    /// 使用调用方执行器写回客户退款的原版本条件。
    /// `refund` 为待写回的退款单；沿用调用方执行器 `executor`。
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_customer_refund(
        &self,
        refund: &mut CustomerRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.customer_refunds().update(refund, executor).await?;
        Ok(())
    }
    /// 审批最终通过、尚未写反向财务事实时，沿调用方事务加载客户退款单，先拒绝已冲正，再要求仍在审批中。
    /// 缺单返回 `NotFound`，已冲正返回 `BusinessLogicError`，非审批中返回 `ConflictError`；传播读取错误。
    pub async fn prepare_customer_refund_final_post(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerRefund> {
        let refund = self.load_customer_refund(id, executor).await?;
        reject_if_reversed(refund.status == CustomerRefundStatus::Reversed, "已冲正退款不能再过账")?;
        super::approval::ensure_final_approve_posting(&refund)?;
        Ok(refund)
    }
    /// 仅支持原回款来源；按原应收分录退款保留原失败说明。
    /// 返回原回款 ID。
    /// `refund` 为客户退款单。
    /// 没有原回款引用时返回 `BusinessLogicError`。
    pub fn customer_refund_receipt_id(refund: &CustomerRefund) -> Result<CustomerReceiptId> {
        refund
            .original_receipt_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("按原应收分录退款由冲减分录完成".to_string()))
    }
    /// 在确认原回款已过账后，沿原执行器累计同一原单已过账的客户退款金额（排除本单），加上本次与原金额比较；累计口径不含回款冲正。
    /// 超限返回 `BusinessLogicError`；传播聚合错误。
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
        ensure_cumulative_within(
            receipt_amount,
            refunded_before,
            refund.amount,
            "累计退款金额不得超过原回款金额",
        )
    }
    /// 正式财务写入成功后，在原事务把审批中的客户退款单标记过账并以原版本 CAS 写回。
    /// 非审批中返回 `Logic`；传播仓储、版本竞争及其他写入错误。
    pub async fn persist_posted_customer_refund(
        &self,
        refund: &mut CustomerRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        refund.mark_posted()?;
        self.persist_customer_refund(refund, executor).await
    }
    /// 客户端请求直接过账客户退款时恒返回 `ConflictError`；最终过账由审批运行时触发。
    pub fn reject_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("客户退款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }
}
/// 创建退款的原事务重读回款后，先与准备阶段版本比对，再检查已过账与经营客户关联。
/// 版本变化返回 `ConflictError`；未过账或无经营客户返回 `BusinessLogicError`。
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
    // 验证源版本优先于状态；状态优先于经营客户缺项，保持原错误分类和文案。
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
    // 验证一体提交预读只要求经营客户，不提前新增posted守卫；金额为空仍冻结原全额。
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
    // 验证经营客户缺项仍在退款实体和金额规则之前拒绝。
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
