//! PaymentReversal 本域草稿、累计限额与事务内状态持久化。

use erp_core::common::time::Instant;
use erp_core::ids::{PaymentReversalId, SupplierPaymentId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;

use super::ReturnsService;
use super::approval::ensure_payment_reversal_final_approve_posting;
use super::shared::{
    DEFAULT_FINANCE_REVIEWER, PAYMENT_REVERSAL_COMMAND_PREFIX, ensure_cumulative_within, or_not_found,
    reject_if_reversed, return_command_no,
};
use crate::dto::{CommitPaymentReversalRequest, CreatePaymentReversalRequest};
use crate::entity::returns::{PaymentReversal, PaymentReversalData, PaymentReversalStatus};
use crate::repository::ReturnsExt;
use crate::{Error, Result};

/// 退款/冲正消费的最小原付款事实，不携带完整财务聚合。
pub struct PaymentReversalSourceFact {
    /// 原付款稳定身份。
    pub payment_id: SupplierPaymentId,
    /// 原付款金额，供省略金额时沿用。
    pub amount: Amount,
}

/// 从已校验请求构造草稿，保留原 ID 与字段求值顺序。
pub fn new_payment_reversal(req: CreatePaymentReversalRequest, actor_id: &str) -> Result<PaymentReversal> {
    let result = PaymentReversal::new(
        PaymentReversalId::new(next_id()),
        PaymentReversalData {
            reversal_no: req.reversal_no,
            original_supplier_payment_id: req.original_supplier_payment_id,
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
    Ok(result)
}

/// 在原资金预读之后构造提交草稿，保留原编号、时钟和默认经办/复核人。
pub fn new_payment_reversal_commit(
    req: &CommitPaymentReversalRequest,
    source: PaymentReversalSourceFact,
    actor_id: &str,
) -> Result<PaymentReversal> {
    let result = PaymentReversal::new(
        PaymentReversalId::new(next_id()),
        PaymentReversalData {
            reversal_no: return_command_no(PAYMENT_REVERSAL_COMMAND_PREFIX, actor_id, &req.idempotency_key),
            original_supplier_payment_id: source.payment_id.clone(),
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
    Ok(result)
}

impl ReturnsService {
    /// 读取本域单据，使用调用方传入的原 Executor 与 NotFound 文案。
    pub async fn load_payment_reversal(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PaymentReversal> {
        or_not_found(self.db.payment_reversals().find_by_id(id, executor).await?, "付款冲正单不存在")
    }

    /// 在调用方创建根内插入本域单据。
    pub async fn create_payment_reversal(
        &self,
        record: &PaymentReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.payment_reversals().create(record, executor).await?;
        Ok(())
    }

    /// 在外层审批或命令事务内以原 CAS 更新本域单据。
    pub async fn persist_payment_reversal(
        db: &mongodb::Database,
        record: &mut PaymentReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        db.payment_reversals().update(record, executor).await?;
        Ok(())
    }

    /// 读取并执行最终过账的原状态闸门；签署的动作分派仍在 Process。
    pub async fn prepare_payment_reversal_post(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PaymentReversal> {
        let record = self.load_payment_reversal(id, executor).await?;
        reject_if_reversed(record.status == PaymentReversalStatus::Reversed, "已冲正单据不能再过账")?;
        ensure_payment_reversal_final_approve_posting(&record)?;
        Ok(record)
    }

    /// 在原付款已过账检查之后读取本域累计金额并检查本次限额。
    pub async fn validate_payment_reversal_amount(
        &self,
        record: &PaymentReversal,
        original_amount: Amount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let original_id = record.original_supplier_payment_id.clone();
        let before = self
            .db
            .payment_reversals()
            .posted_reversal_total_by_payment(&original_id, &record.base.id, executor)
            .await?;
        ensure_cumulative_within(original_amount, before, record.amount, "累计冲正金额不得超过原付款金额")
    }

    /// 财务事实写入后标记本域已过账并以原 CAS 持久化。
    pub async fn persist_payment_reversal_post(
        &self,
        reversal: &mut PaymentReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        reversal.mark_posted()?;
        self.db.payment_reversals().update(reversal, executor).await?;
        Ok(())
    }

    /// 客户端直接过账恒按原冲突规则拒绝。
    pub fn reject_payment_reversal_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("付款冲正过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }
}
