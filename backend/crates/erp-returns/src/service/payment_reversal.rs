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
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 退款/冲正消费的最小原付款事实，不携带完整财务聚合。
pub struct PaymentReversalSourceFact {
    /// 原付款稳定身份。
    pub payment_id: SupplierPaymentId,
    /// 原付款金额，供省略金额时沿用。
    pub amount: Amount,
}

/// 从已校验请求构造草稿，保留原 ID 与字段求值顺序。
///
/// # 参数
/// * `req` - 创建请求。
/// * `actor_id` - 已认证创建人。
///
/// # 返回
/// 返回初始草稿付款冲正单。
///
/// # 错误
/// 编号、原因、经办复核人、创建人为空或超长、金额非正或经办与复核人相同时返回 `Logic`。
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
///
/// # 参数
/// * `req` - 一次提交命令。
/// * `source` - 原付款窄事实。
/// * `actor_id` - 经办人，同时作为创建人。
///
/// # 返回
/// 返回指向该原付款的草稿冲正单；省略金额时沿用原付款金额。
///
/// # 错误
/// 原因、经办复核人或金额不满足实体不变量时返回 `Logic`。
pub fn new_payment_reversal_commit(
    req: &CommitPaymentReversalRequest,
    source: PaymentReversalSourceFact,
    actor_id: &str,
) -> Result<PaymentReversal> {
    let result = PaymentReversal::new(
        PaymentReversalId::new(next_id()),
        PaymentReversalData {
            reversal_no: return_command_no(PAYMENT_REVERSAL_COMMAND_PREFIX, actor_id, &req.idempotency_key),
            original_supplier_payment_id: source.payment_id,
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
    ///
    /// # 参数
    /// * `id` - 冲正单主键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回读到的付款冲正单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
    pub async fn load_payment_reversal(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PaymentReversal> {
        or_not_found(self.db.payment_reversals().find_by_id(id, executor).await?, "付款冲正单不存在")
    }

    /// 在调用方创建根内插入本域单据。
    ///
    /// # 参数
    /// * `record` - 待写入的冲正单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// 仓储写入失败时返回对应错误。
    pub async fn create_payment_reversal(
        &self,
        record: &PaymentReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.payment_reversals().create(record, executor).await?;
        Ok(())
    }

    /// 在外层审批或命令事务内以原 CAS 更新本域单据。
    ///
    /// # 参数
    /// * `db` - 数据库句柄。
    /// * `record` - 待写回的冲正单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 写回成功时无返回值。
    ///
    /// # 错误
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_payment_reversal(
        db: &mongodb::Database,
        record: &mut PaymentReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        db.payment_reversals().update(record, executor).await?;
        Ok(())
    }

    /// 读取并执行最终过账的原状态闸门；签署的动作分派仍在 Process。
    ///
    /// # 参数
    /// * `id` - 冲正单主键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回仍可过账的冲正单。
    ///
    /// # 错误
    /// 单据不存在时返回 `NotFound`；已冲正时返回 `BusinessLogicError`；非审批中时返回 `ConflictError`；仓储读取失败时返回对应错误。
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
    ///
    /// # 参数
    /// * `record` - 本次冲正单，其主键从累计中排除。
    /// * `original_amount` - 原付款金额。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 累计未超限时返回成功。
    ///
    /// # 错误
    /// 已过账累计加上本次超过原付款金额时返回 `BusinessLogicError`；聚合失败时返回对应错误。
    pub async fn validate_payment_reversal_amount(
        &self,
        record: &PaymentReversal,
        original_amount: Amount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let before = self
            .db
            .payment_reversals()
            .posted_reversal_total_by_payment(&record.original_supplier_payment_id, &record.base.id, executor)
            .await?;
        ensure_cumulative_within(original_amount, before, record.amount, "累计冲正金额不得超过原付款金额")
    }

    /// 财务事实写入后标记本域已过账并以原 CAS 持久化。
    ///
    /// # 参数
    /// * `reversal` - 待标记过账的冲正单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 过账状态写回成功时无返回值。
    ///
    /// # 错误
    /// 状态不是审批中时返回 `Logic`；仓储更新失败时返回对应错误。
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
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 不返回成功值。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_payment_reversal_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("付款冲正过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }
}
