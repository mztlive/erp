//! SupplierRefund 本域草稿、累计限额与事务内状态持久化。

use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierPaymentId, SupplierRefundId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;

use super::ReturnsService;
use super::approval::ensure_supplier_refund_final_approve_posting;
use super::shared::{DEFAULT_FINANCE_REVIEWER, SUPPLIER_REFUND_COMMAND_PREFIX, return_command_no};
use crate::dto::{CommitSupplierRefundRequest, CreateSupplierRefundRequest};
use crate::entity::returns::{
    CumulativeAmountLimit, SupplierRefund, SupplierRefundData, SupplierRefundStatus,
};
use crate::repository::ReturnsExt;
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
            supplier_id: source.supplier_id.clone(),
            original_payment_id: Some(source.payment_id.clone()),
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
    pub async fn load_supplier_refund(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefund> {
        self.db
            .supplier_refunds()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))
    }

    /// 在调用方创建根内插入本域单据。
    pub async fn create_supplier_refund(
        &self,
        record: &SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_refunds().create(record, executor).await?;
        Ok(())
    }

    /// 在外层审批或命令事务内以原 CAS 更新本域单据。
    pub async fn persist_supplier_refund(
        db: &mongodb::Database,
        record: &mut SupplierRefund,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        db.supplier_refunds().update(record, executor).await?;
        Ok(())
    }

    /// 读取并执行最终过账的原状态闸门；签署的动作分派仍在 Process。
    pub async fn prepare_supplier_refund_post(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefund> {
        let record = self.load_supplier_refund(id, executor).await?;
        if record.status == SupplierRefundStatus::Reversed {
            return Err(Error::BusinessLogicError("已冲正退款不能再过账".to_string()));
        }
        ensure_supplier_refund_final_approve_posting(&record)?;
        Ok(record)
    }

    /// 在原付款已过账检查之后读取本域累计金额并检查本次限额。
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
        CumulativeAmountLimit::ensure_within_limit(original_amount, before, record.amount)
            .map_err(|_| Error::BusinessLogicError("累计退款金额不得超过原付款金额".to_string()))
    }

    /// 财务事实写入后标记本域已过账并以原 CAS 持久化。
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
    pub fn reject_supplier_refund_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("供应商退款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }
}

/// 最终退款必须引用原付款，分录来源保持原拒绝信息。
pub fn original_payment_id(refund: &SupplierRefund) -> Result<SupplierPaymentId> {
    refund
        .original_payment_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("按原应付分录退款由冲减分录完成".to_string()))
}
