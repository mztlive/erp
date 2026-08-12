use super::dto::{CreatePaymentReversalRequest, PaymentReversalView, PostPaymentReversalRequest};
use super::reversal_plan::{plan_payment_reverse, zero_amount};
use super::ReturnsService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use database::{AccessControlExt, NoTransaction, PayableExt, ReturnsExt, Transactional};
use entities::ids::{PaymentAllocationId, PaymentReversalId};
use entities::money::Amount;
use entities::payable::{
    AllocationAction as PayableAllocationAction, PaymentAllocation, PaymentAllocationData,
    SupplierPaymentStatus,
};
use entities::returns::{PaymentReversal, PaymentReversalData, PaymentReversalStatus};
use id_generator::next_id;
use validator::Validate;

impl ReturnsService {
    /// 登记付款冲正草稿（单集合写入，无事务；经办/复核分离）。
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
    pub async fn create_payment_reversal(
        &self,
        req: CreatePaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        req.validate()?;
        let reversal = PaymentReversal::new(
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
        )?;
        let audit = actor.clone().resource_log(
            "payment_reversal.create",
            "payment_reversal",
            reversal.base.id.clone(),
        )?;
        self.db
            .payment_reversals()
            .create(&reversal, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        self.payment_reversal_view(reversal.base.id.clone()).await
    }

    /// 付款冲正过账（§8.3-3 事务不变量，应付侧镜像）。
    ///
    /// 同一事务内：冲正单必须为草稿；按原付款核销分配反向写入 `REVERSE`
    /// 分配并原子冲减应付子账已核销进度；原付款迁移为已冲正；冲正单迁移为
    /// 已过账。累计有效冲正不得超过原付款金额。
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
    /// * `NotFound` - 冲正单或原付款不存在
    /// * `BusinessLogicError` - 累计冲正超原付款或重复过账
    pub async fn post_payment_reversal(
        &self,
        id: &str,
        req: PostPaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
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
                        .payment_reversals()
                        .find_by_id(&reversal_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
                    if reversal.status != PaymentReversalStatus::Draft {
                        return Err(Error::BusinessLogicError(
                            "冲正单已过账，请勿重复提交".to_string(),
                        ));
                    }
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
                        .find_reversals_by_payments(std::slice::from_ref(&original_id), session)
                        .await?
                        .iter()
                        .filter(|other| other.base.id != reversal.base.id)
                        .fold(zero_amount(), |sum, other| sum.checked_add(other.amount));
                    if reversed_before.checked_add(reversal.amount) > payment.amount {
                        return Err(Error::BusinessLogicError(
                            "累计冲正金额不得超过原付款金额".to_string(),
                        ));
                    }

                    let allocations = db
                        .payment_allocations()
                        .find_allocations_by_payments(&[original_id], session)
                        .await?;
                    let (reverse_rows, chunks) = plan_payment_reverse(&allocations, reversal.amount)?;
                    let next_seq = allocations
                        .iter()
                        .map(|allocation| allocation.allocation_seq)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    for chunk in &chunks {
                        let entry = db
                            .payable_entries()
                            .find_by_id(&chunk.increase_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
                        let reverted = db
                            .payable_accounts()
                            .revert_settlement(&entry.payable_account_id, &chunk.amount, &actor_id, session)
                            .await?;
                        if !reverted {
                            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
                        }
                    }
                    for (reverse_index, reverse) in reverse_rows.iter().enumerate() {
                        let allocation = PaymentAllocation::new(
                            PaymentAllocationId::new(next_id()),
                            PaymentAllocationData {
                                supplier_payment_id: payment.base.id.clone().into(),
                                payable_entry_id: reverse.entry_id.clone(),
                                allocation_seq: next_seq + reverse_index as u32,
                                allocation_action: PayableAllocationAction::Reverse,
                                allocated_amount: reverse.amount,
                                allocated_at: reversal.occurred_at,
                                reverses_allocation_id: Some(reverse.original_id.clone()),
                            },
                        )?;
                        db.payment_allocations().create(&allocation, session).await?;
                    }
                    let mut payment_mut = payment;
                    payment_mut.transition(SupplierPaymentStatus::Reversed)?;
                    db.supplier_payments().update(&mut payment_mut, session).await?;
                    reversal.transition(PaymentReversalStatus::Posted)?;
                    db.payment_reversals().update(&mut reversal, session).await?;
                    let audit = actor_owned.clone().resource_log(
                        "payment_reversal.post",
                        "payment_reversal",
                        reversal.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.payment_reversal_view(detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配付款冲正单视图。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    async fn payment_reversal_view(&self, id: String) -> Result<PaymentReversalView> {
        let reversal = self
            .db
            .payment_reversals()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
        Ok(PaymentReversalView {
            id: reversal.base.id.clone(),
            reversal_no: reversal.reversal_no,
            status: reversal.status,
            original_supplier_payment_id: reversal.original_supplier_payment_id.to_string(),
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
