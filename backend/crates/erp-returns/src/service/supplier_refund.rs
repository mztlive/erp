//! SupplierRefund 本域草稿、累计限额与事务内状态持久化。

use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierPaymentId, SupplierRefundId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;

use super::ReturnsService;
use super::approval::ensure_supplier_refund_final_approve_posting;
use super::shared::{
    DEFAULT_FINANCE_REVIEWER, SUPPLIER_REFUND_COMMAND_PREFIX, ensure_cumulative_within, or_not_found,
    reject_if_reversed, return_command_no,
};
use crate::dto::{CommitSupplierRefundRequest, CreateSupplierRefundRequest};
use crate::entity::returns::{SupplierRefund, SupplierRefundData, SupplierRefundStatus};
use crate::repository::ReturnsExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 退款/冲正消费的最小原付款事实，不携带完整财务聚合。
pub struct SupplierRefundSourceFact {
    /// 原付款稳定身份。
    pub payment_id: SupplierPaymentId,
    /// 原付款所属供应商。
    pub supplier_id: SupplierAccountId,
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
/// 返回初始草稿供应商退款单。
///
/// # 错误
/// 编号、原因、经办复核人、创建人为空或超长、金额非正、经办与复核人相同，或原付款与原应付未二选一时返回 `Logic`。
pub fn new_supplier_refund(req: CreateSupplierRefundRequest, actor_id: &str) -> Result<SupplierRefund> {
    let result = SupplierRefund::new(
        SupplierRefundId::new(next_id()),
        SupplierRefundData {
            refund_no: req.refund_no,
            purchase_return_order_id: req.purchase_return_order_id,
            supplier_id: req.supplier_id,
            original_payment_id: req.original_payment_id,
            original_payable_entry_id: req.original_payable_entry_id,
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
/// 返回指向该原付款的草稿退款单；省略金额时沿用原付款金额。
///
/// # 错误
/// 原因、经办复核人或金额不满足实体不变量时返回 `Logic`。
pub fn new_supplier_refund_commit(
    req: &CommitSupplierRefundRequest,
    source: SupplierRefundSourceFact,
    actor_id: &str,
) -> Result<SupplierRefund> {
    let result = SupplierRefund::new(
        SupplierRefundId::new(next_id()),
        SupplierRefundData {
            refund_no: return_command_no(SUPPLIER_REFUND_COMMAND_PREFIX, actor_id, &req.idempotency_key),
            purchase_return_order_id: None,
            supplier_id: source.supplier_id,
            original_payment_id: Some(source.payment_id),
            original_payable_entry_id: None,
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
    /// * `id` - 退款单主键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回读到的供应商退款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
    pub async fn load_supplier_refund(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefund> {
        or_not_found(self.db.supplier_refunds().find_by_id(id, executor).await?, "供应商退款单不存在")
    }

    /// 在调用方创建根内插入本域单据。
    ///
    /// # 参数
    /// * `record` - 待写入的退款单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// 仓储写入失败时返回对应错误。
    pub async fn create_supplier_refund(
        &self,
        record: &SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_refunds().create(record, executor).await?;
        Ok(())
    }

    /// 在外层审批或命令事务内以原 CAS 更新本域单据。
    ///
    /// # 参数
    /// * `db` - 数据库句柄。
    /// * `record` - 待写回的退款单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 写回成功时无返回值。
    ///
    /// # 错误
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_supplier_refund(
        db: &mongodb::Database,
        record: &mut SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        db.supplier_refunds().update(record, executor).await?;
        Ok(())
    }

    /// 读取并执行最终过账的原状态闸门；签署的动作分派仍在 Process。
    ///
    /// # 参数
    /// * `id` - 退款单主键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回仍可过账的退款单。
    ///
    /// # 错误
    /// 单据不存在时返回 `NotFound`；已冲正时返回 `BusinessLogicError`；非审批中时返回 `ConflictError`；仓储读取失败时返回对应错误。
    pub async fn prepare_supplier_refund_post(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefund> {
        let record = self.load_supplier_refund(id, executor).await?;
        reject_if_reversed(record.status == SupplierRefundStatus::Reversed, "已冲正退款不能再过账")?;
        ensure_supplier_refund_final_approve_posting(&record)?;
        Ok(record)
    }

    /// 在原付款已过账检查之后读取本域累计金额并检查本次限额。
    ///
    /// # 参数
    /// * `record` - 本次退款单，其主键从累计中排除。
    /// * `original_amount` - 原付款金额。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 累计未超限时返回成功。
    ///
    /// # 错误
    /// 退款未引用原付款，或累计超过原付款金额时返回 `BusinessLogicError`；聚合失败时返回对应错误。
    pub async fn validate_supplier_refund_amount(
        &self,
        record: &SupplierRefund,
        original_amount: Amount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let original_id = original_payment_id(record)?;
        let before = self
            .db
            .supplier_refunds()
            .posted_refund_total_by_payment(&original_id, &record.base.id, executor)
            .await?;
        ensure_cumulative_within(original_amount, before, record.amount, "累计退款金额不得超过原付款金额")
    }

    /// 财务事实写入后标记本域已过账并以原 CAS 持久化。
    ///
    /// # 参数
    /// * `refund` - 待标记过账的退款单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 过账状态写回成功时无返回值。
    ///
    /// # 错误
    /// 状态不是审批中时返回 `Logic`；仓储更新失败时返回对应错误。
    pub async fn persist_supplier_refund_post(
        &self,
        refund: &mut SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        refund.mark_posted()?;
        self.db.supplier_refunds().update(refund, executor).await?;
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
    pub fn reject_supplier_refund_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("供应商退款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }
}

/// 最终退款必须引用原付款，分录来源保持原拒绝信息。
///
/// # 参数
/// * `refund` - 供应商退款单。
///
/// # 返回
/// 返回原付款 ID。
///
/// # 错误
/// 没有原付款引用时返回 `BusinessLogicError`。
pub fn original_payment_id(refund: &SupplierRefund) -> Result<SupplierPaymentId> {
    refund
        .original_payment_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("按原应付分录退款由冲减分录完成".to_string()))
}
