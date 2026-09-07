//! 付款冲正、应付冲减、付款工作项与审计的同事务逆向流程。

mod receipt_reversal;
pub use receipt_reversal::ReceiptReversalProcess;

use application_core::AuditActor;
use database::ReturnsExt;
use entities::returns::{CumulativeAmountLimit, PaymentReversal};
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::PaymentAllocationId;
use erp_core::money::Amount;
use erp_finance::entity::payable::{
    AllocationAction as PayableAllocationAction, PaymentAllocation, PaymentAllocationData, SupplierPayment,
    SupplierPaymentStatus,
};
use erp_finance::repository::PayableExt;
use erp_identity::SharedRbacService;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;
use services::returns::{load_payable_offset_facts, PaymentReversalView, ReturnsService};
use services::{Error, Result};
use std::collections::HashSet;

/// 付款冲正最终通过流程；所有财务与工作项副作用复用调用方事务。
pub struct PaymentReversalProcess {
    db: Database,
}
impl PaymentReversalProcess {
    /// 绑定组合根数据库；审批运行时已完成授权。
    pub fn new(db: Database, _rbac: SharedRbacService) -> Self {
        Self { db }
    }

    /// 最终通过过账（§8.3-3 事务不变量，应付侧镜像）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 按原付款核销分配反向写入 `REVERSE` 分配并原子冲减应付子账已核销进度；
    /// 原付款迁移为已冲正；冲正单迁移为已过账。任一校验失败整体回滚。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原付款不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 累计冲正超原付款、重复过账或超额冲减
    pub async fn post_payment_reversal(&self, id: &str, actor: &AuditActor) -> Result<PaymentReversalView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let reversal_id = id.to_string();
        let detail_id = reversal_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    apply_payment_reversal_final_post(&db, &reversal_id, &actor_id, &actor_owned, session)
                        .await
                })
            })
            .await?;

        ReturnsService::new(self.db.clone())
            .payment_reversal_detail(&detail_id)
            .await
    }

    /// 在审批最终通过持有的唯一事务内执行付款冲正。
    pub async fn post_payment_reversal_in_transaction(
        &self,
        id: &str,
        actor: &AuditActor,
        session: &mut mongodb::ClientSession,
    ) -> Result<()> {
        apply_payment_reversal_final_post(&self.db, id, actor.id(), actor, session).await
    }
}

/// 在最终通过事务内执行过账副作用并写回冲正单。
///
/// # 错误
/// 非审批中、原付款不存在或仓储失败时返回错误。
async fn apply_payment_reversal_final_post(
    db: &Database,
    reversal_id: &str,
    actor_id: &str,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let mut reversal = ReturnsService::prepare_payment_reversal_post(db, reversal_id, session).await?;
    apply_payment_reversal_posting(db, &reversal, actor_id, session).await?;
    reversal.mark_posted()?;
    db.payment_reversals().update(&mut reversal, session).await?;
    let audit = actor.clone().resource_log(
        "payment_reversal.post",
        "payment_reversal",
        reversal.base.id.clone(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(())
}

/// 在调用方事务内写入冲正副作用。
///
/// # 错误
/// 原付款不存在、累计超额或仓储失败时返回错误。
async fn apply_payment_reversal_posting(
    db: &Database,
    reversal: &PaymentReversal,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let original_id = reversal.original_supplier_payment_id.clone();
    let payment = db
        .supplier_payments()
        .find_by_id(&original_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("原付款不存在".to_string()))?;
    if payment.status != SupplierPaymentStatus::Posted {
        return Err(Error::BusinessLogicError("只有已过账付款可以冲正".to_string()));
    }
    let reversed_before: Amount = db
        .payment_reversals()
        .posted_reversal_total_by_payment(&original_id, &reversal.base.id, session)
        .await?;
    CumulativeAmountLimit::ensure_within_limit(payment.amount, reversed_before, reversal.amount)
        .map_err(|_| Error::BusinessLogicError("累计冲正金额不得超过原付款金额".to_string()))?;
    persist_reversal_offsets_and_mark_payment(db, reversal, payment, actor_id, session).await
}

/// 写入反向核销分配、冲减进度并把原付款置为已冲正。
///
/// # 错误
/// 超额冲减或仓储失败时返回错误。
async fn persist_reversal_offsets_and_mark_payment(
    db: &Database,
    reversal: &PaymentReversal,
    payment: SupplierPayment,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let allocations = db
        .payment_allocations()
        .find_allocations_by_payments(&[payment.base.id.clone().into()], session)
        .await?;
    let (reverse_rows, chunks) = PaymentAllocation::plan_reverse(&allocations, reversal.amount)?;
    let seqs = PaymentAllocation::next_allocation_seq_range(&allocations, reverse_rows.len())?;
    revert_payment_settlements(db, &chunks, actor_id, session).await?;
    persist_reverse_allocations(db, reversal, &payment, &reverse_rows, &seqs, session).await?;
    let mut payment = payment;
    payment.transition(SupplierPaymentStatus::Reversed)?;
    db.supplier_payments().update(&mut payment, session).await?;
    Ok(())
}

/// 按冲减块回冲应付子账已核销进度。
///
/// # 错误
/// 分录缺失或超额冲减时返回错误。
async fn revert_payment_settlements(
    db: &Database,
    chunks: &[erp_finance::entity::payable::PaymentReverseChunk],
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let facts = load_payable_offset_facts(
        db,
        chunks.iter().map(|chunk| chunk.increase_entry_id.clone()),
        session,
    )
    .await?;
    let mut affected_accounts = HashSet::new();
    for chunk in chunks {
        let entry = facts
            .entries
            .get(chunk.increase_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        if !facts.accounts.contains_key(entry.payable_account_id.as_ref()) {
            return Err(Error::NotFound("应付往来子账不存在".to_string()));
        }
        let reverted = db
            .payable_accounts()
            .revert_settlement(&entry.payable_account_id, &chunk.amount, actor_id, session)
            .await?;
        if !reverted {
            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
        }
        affected_accounts.insert(entry.payable_account_id.clone());
    }
    for account_id in affected_accounts {
        crate::finance_posting::payable::payment_task::sync_purchase_payment_task(db, &account_id, session)
            .await?;
    }
    Ok(())
}

/// 写入反向核销分配。
///
/// # 错误
/// 仓储失败时返回错误。
async fn persist_reverse_allocations(
    db: &Database,
    reversal: &PaymentReversal,
    payment: &SupplierPayment,
    reverse_rows: &[erp_finance::entity::payable::PaymentReversePlanRow],
    seqs: &[u32],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for (reverse, seq) in reverse_rows.iter().zip(seqs.iter()) {
        let allocation = PaymentAllocation::new(
            PaymentAllocationId::new(next_id()),
            PaymentAllocationData {
                supplier_payment_id: payment.base.id.clone().into(),
                payable_entry_id: reverse.entry_id.clone(),
                allocation_seq: *seq,
                allocation_action: PayableAllocationAction::Reverse,
                allocated_amount: reverse.amount,
                allocated_at: reversal.occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        db.payment_allocations().create(&allocation, session).await?;
    }
    Ok(())
}
