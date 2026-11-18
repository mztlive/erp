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
/// 返回初始草稿供应商退款单。
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
/// 返回指向该原付款的草稿退款单；省略金额时沿用原付款金额。
/// `req` 为一次提交命令；`source` 为原付款窄事实。
/// `actor_id` 为经办人，同时作为创建人。
/// 原因、经办复核人或金额不满足实体不变量时返回 `Logic`。
pub fn new_supplier_refund_commit(
    req: &CommitSupplierRefundRequest,
    source: SupplierRefundSourceFact,
    actor_id: &str,
) -> Result<SupplierRefund> {
    let mut result = SupplierRefund::new(
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

    result.settlement_method = req.settlement_method;
    result.source_purchase_order_id = req.source_purchase_order_id.clone();
    result.settlement_decision_key = req.settlement_decision_key.clone();
    Ok(result)
}

impl ReturnsService {
    /// 读取本域单据，使用调用方传入的原 Executor 与 NotFound 文案。
    /// 返回读到的供应商退款单。
    /// `id` 为退款单主键；沿用调用方执行器 `executor`。
    /// 不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
    pub async fn load_supplier_refund(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefund> {
        or_not_found(self.db.supplier_refunds().find_by_id(id, executor).await?, "供应商退款单不存在")
    }

    /// 在调用方创建根内插入本域单据。
    /// `record` 为待写入的退款单；沿用调用方执行器 `executor`。
    /// 传播仓储写入错误。
    pub async fn create_supplier_refund(
        &self,
        record: &SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_refunds().create(record, executor).await?;
        Ok(())
    }

    /// 在外层审批或命令事务内以原 CAS 更新本域单据。
    /// `record` 为待写回的退款单；沿用调用方执行器 `executor`。
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_supplier_refund(
        db: &mongodb::Database,
        record: &mut SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        db.supplier_refunds().update(record, executor).await?;
        Ok(())
    }

    /// 审批最终通过、尚未写反向财务事实时，沿调用方事务加载供应商退款单，先拒绝已冲正，再要求仍在审批中。
    /// 缺单返回 `NotFound`，已冲正返回 `BusinessLogicError`，非审批中返回 `ConflictError`；传播读取错误。
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

    /// 在确认原付款已过账后，沿原执行器累计同一原单已过账的供应商退款金额（排除本单），加上本次与原金额比较。
    /// 超限返回 `BusinessLogicError`；没有原付款引用也按业务规则错误返回；传播聚合错误。
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

    /// 正式财务写入成功后，在原事务把审批中的供应商退款单标记过账并以原版本 CAS 写回。
    /// 非审批中返回 `Logic`；传播仓储、版本竞争及其他写入错误。
    pub async fn persist_supplier_refund_post(
        &self,
        refund: &mut SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        refund.mark_posted()?;
        self.db.supplier_refunds().update(refund, executor).await?;
        Ok(())
    }

    /// 客户端请求直接过账供应商退款时恒返回 `ConflictError`；最终过账由审批运行时触发。
    pub fn reject_supplier_refund_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("供应商退款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }
}

/// 最终退款必须引用原付款，分录来源保持原拒绝信息。
/// 返回原付款 ID。
/// `refund` 为供应商退款单。
/// 没有原付款引用时返回 `BusinessLogicError`。
pub fn original_payment_id(refund: &SupplierRefund) -> Result<SupplierPaymentId> {
    refund
        .original_payment_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("按原应付分录退款由冲减分录完成".to_string()))
}
