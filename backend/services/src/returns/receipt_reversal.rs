use super::dto::{CreateReceiptReversalRequest, PostReceiptReversalRequest, ReceiptReversalView};
use super::reversal_plan::{plan_receipt_reverse, zero_amount};
use super::ReturnsService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use database::{AccessControlExt, NoTransaction, ReceivableExt, ReturnsExt, Transactional};
use entities::ids::{ReceiptAllocationId, ReceiptReversalId};
use entities::money::Amount;
use entities::receivable::{
    AllocationAction as ReceivableAllocationAction, CustomerReceiptStatus, ReceiptAllocation,
    ReceiptAllocationData,
};
use entities::returns::{ReceiptReversal, ReceiptReversalData, ReceiptReversalStatus};
use id_generator::next_id;
use validator::Validate;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 回款冲正与付款冲正
    // -----------------------------------------------------------------------

    /// 登记回款冲正草稿（单集合写入，无事务；经办/复核分离）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建冲正单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 冲正单号重复
    pub async fn create_receipt_reversal(
        &self,
        req: CreateReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let reversal = ReceiptReversal::new(
            ReceiptReversalId::new(next_id()),
            ReceiptReversalData {
                reversal_no: req.reversal_no,
                original_customer_receipt_id: req.original_customer_receipt_id,
                reason_code: req.reason_code,
                reason_text: req.reason_text,
                amount: req.amount,
                handled_by: req.handled_by,
                reviewed_by: req.reviewed_by,
                occurred_at: req.occurred_at,
                evidence_attachment_id: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "receipt_reversal.create",
            "receipt_reversal",
            reversal.base.id.clone(),
        )?;
        self.db
            .receipt_reversals()
            .create(&reversal, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        self.receipt_reversal_view(reversal.base.id.clone()).await
    }

    /// 回款冲正过账（§8.3-3 事务不变量）。
    ///
    /// 同一事务内：冲正单必须为草稿；按原回款核销分配反向写入 `REVERSE`
    /// 分配并原子冲减子账已核销进度；原回款迁移为已冲正；冲正单迁移为已过账。
    /// 累计有效冲正不得超过原回款金额。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    /// * `req` - 过账请求（占位）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原回款不存在
    /// * `BusinessLogicError` - 累计冲正超原回款或重复过账
    pub async fn post_receipt_reversal(
        &self,
        id: &str,
        req: PostReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let reversal_id = id.to_string();
        let detail_id = reversal_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut reversal = db
                        .receipt_reversals()
                        .find_by_id(&reversal_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
                    if reversal.status != ReceiptReversalStatus::Draft {
                        return Err(Error::BusinessLogicError(
                            "冲正单已过账，请勿重复提交".to_string(),
                        ));
                    }
                    let original_id = reversal.original_customer_receipt_id.clone();
                    let receipt = db
                        .customer_receipts()
                        .find_by_id(&original_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
                    if receipt.status != CustomerReceiptStatus::Posted {
                        return Err(Error::BusinessLogicError("只有已过账回款可以冲正".to_string()));
                    }
                    let reversed_before: Amount = db
                        .receipt_reversals()
                        .find_reversals_by_receipts(std::slice::from_ref(&original_id), session)
                        .await?
                        .iter()
                        .filter(|other| other.base.id != reversal.base.id)
                        .fold(zero_amount(), |sum, other| sum.checked_add(other.amount));
                    if reversed_before.checked_add(reversal.amount) > receipt.amount {
                        return Err(Error::BusinessLogicError(
                            "累计冲正金额不得超过原回款金额".to_string(),
                        ));
                    }

                    let allocations = db
                        .receipt_allocations()
                        .find_allocations_by_receipts(&[original_id], session)
                        .await?;
                    let (reverse_rows, chunks) = plan_receipt_reverse(&allocations, reversal.amount)?;
                    let next_seq = allocations
                        .iter()
                        .map(|allocation| allocation.allocation_seq)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    for chunk in &chunks {
                        let entry = db
                            .receivable_entries()
                            .find_by_id(&chunk.increase_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
                        let reverted = db
                            .receivable_accounts()
                            .revert_settlement(
                                &entry.receivable_account_id,
                                &chunk.amount,
                                &actor_id,
                                session,
                            )
                            .await?;
                        if !reverted {
                            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
                        }
                    }
                    for (reverse_index, reverse) in reverse_rows.iter().enumerate() {
                        let allocation = ReceiptAllocation::new(
                            ReceiptAllocationId::new(next_id()),
                            ReceiptAllocationData {
                                customer_receipt_id: receipt.base.id.clone().into(),
                                receivable_entry_id: reverse.entry_id.clone(),
                                allocation_seq: next_seq + reverse_index as u32,
                                allocation_action: ReceivableAllocationAction::Reverse,
                                allocated_amount: reverse.amount,
                                allocated_at: reversal.occurred_at,
                                reverses_allocation_id: Some(reverse.original_id.clone()),
                            },
                        )?;
                        db.receipt_allocations().create(&allocation, session).await?;
                    }
                    let mut receipt_mut = receipt;
                    receipt_mut.transition(CustomerReceiptStatus::Reversed)?;
                    db.customer_receipts().update(&mut receipt_mut, session).await?;
                    reversal.transition(ReceiptReversalStatus::Posted)?;
                    db.receipt_reversals().update(&mut reversal, session).await?;
                    let audit = actor_owned.clone().resource_log(
                        "receipt_reversal.post",
                        "receipt_reversal",
                        reversal.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.receipt_reversal_view(detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配回款冲正单视图。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    async fn receipt_reversal_view(&self, id: String) -> Result<ReceiptReversalView> {
        let reversal = self
            .db
            .receipt_reversals()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
        Ok(ReceiptReversalView {
            id: reversal.base.id.clone(),
            reversal_no: reversal.reversal_no,
            status: reversal.status,
            original_customer_receipt_id: reversal.original_customer_receipt_id.to_string(),
            reason_code: reversal.reason_code,
            reason_text: reversal.reason_text,
            amount: reversal.amount,
            handled_by: reversal.handled_by,
            reviewed_by: reversal.reviewed_by,
            occurred_at: reversal.occurred_at,
            version: reversal.base.version,
            created_at: reversal.base.created_at,
        })
    }
}
