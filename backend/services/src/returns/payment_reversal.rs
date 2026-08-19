use super::ReturnsService;
use super::adapter::{
    RECENT_HISTORY_LIMIT, build_payment_reversal_snapshot, ensure_payment_reversal_final_approve_posting,
    execute_payment_reversal_domain_action, payment_reversal_adapter, payment_reversal_approval_view,
    payment_reversal_object_readable, payment_reversal_responsible_org_id, payment_reversal_start_command,
    payment_reversal_start_command_kind, payment_reversal_subject_ref, require_payment_reversal_binding,
    start_payment_reversal_approval,
};
use super::cancel_approval::{
    build_payment_reversal_cancel_input, load_cancel_runtime, persist_payment_reversal_cancel,
};
use super::dto::{
    CancelPaymentReversalApprovalRequest, CreatePaymentReversalRequest, PaymentReversalView,
    SubmitPaymentReversalRequest,
};
use super::reversal_plan::{plan_payment_reverse, zero_amount};
use super::start_approval::{
    build_payment_reversal_start_input, load_bound_definition_graph, load_payment_reversal_start_receipt,
    persist_payment_reversal_start,
};
use crate::approval::binding::{
    BindPublishedDefinitionCommand, attach_published_binding, bind_published_definition_on_document_create,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::{prepare_cancel, prepare_start};
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, new_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use database::{
    AccessControlExt, DocumentRegistryExt, NoTransaction, PayableExt, ReturnsExt, SupplierExt, Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::BusinessDocument;
use entities::document_registry::DocumentType;
use entities::ids::{PaymentAllocationId, PaymentReversalId, SupplierAccountId, SupplierPaymentId};
use entities::money::Amount;
use entities::payable::{
    AllocationAction as PayableAllocationAction, PaymentAllocation, PaymentAllocationData, SupplierPayment,
    SupplierPaymentStatus,
};
use entities::returns::{PaymentReversal, PaymentReversalData, PaymentReversalStatus};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 付款冲正
    // -----------------------------------------------------------------------

    /// 查询付款冲正详情。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    pub async fn payment_reversal_detail(&self, id: &str) -> Result<PaymentReversalView> {
        self.payment_reversal_view(id.to_string()).await
    }

    /// 登记付款冲正草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 冲正单号全局唯一（唯一索引）构成幂等去重。经办人与复核人必须不同。
    /// 绑定失败必须回滚业务实体，不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建冲正单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 冲正单号重复或流程未配置
    /// * `NotFound` - 原付款或供应商不存在
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
        persist_created_payment_reversal(&self.db, &self.rbac, reversal.clone(), actor.clone()).await?;
        self.payment_reversal_detail(&reversal.base.id).await
    }

    /// 提交付款冲正并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 冲正单主键
    /// * `req` - 提交请求（版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原付款不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_payment_reversal(
        &self,
        id: &str,
        req: SubmitPaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        req.validate()?;
        let adapter = payment_reversal_adapter()?;
        let mut reversal = self.load_payment_reversal(id).await?;
        ensure_expected_version(reversal.base.version, req.expected_version)?;
        start_payment_reversal_approval(&mut reversal)?;
        self.dispatch_payment_reversal_start(id, reversal, req.idempotency_key, actor, adapter)
            .await
    }

    /// 撤回付款冲正审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 冲正单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    /// * `ConflictError` - 非审批中、已最终通过或并发冲突
    pub async fn cancel_payment_reversal_approval(
        &self,
        id: &str,
        req: CancelPaymentReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        req.validate()?;
        let mut reversal = self.load_payment_reversal(id).await?;
        ensure_expected_version(reversal.base.version, req.expected_version)?;
        self.persist_cancelled_payment_reversal(id, &mut reversal, &req, actor)
            .await?;
        self.payment_reversal_detail(id).await
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_payment_reversal_client_post() -> Result<PaymentReversalView> {
        Err(Error::ConflictError(
            "付款冲正过账只能由审批最终通过动作执行，客户端不得直接过账".to_string(),
        ))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_payment_reversal_start(
        &self,
        id: &str,
        reversal: PaymentReversal,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::adapter::PaymentReversalAdapter,
    ) -> Result<PaymentReversalView> {
        let subject = payment_reversal_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_payment_reversal_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let (organization_id, supplier_id) = self
            .payment_reversal_context(&reversal.original_supplier_payment_id)
            .await?;
        let snapshot =
            build_payment_reversal_snapshot(&reversal, &organization_id, &supplier_id, actor.id(), now)?;
        let start = payment_reversal_start_command(
            id,
            reversal.approval_subject_version,
            actor.id(),
            &idempotency_key,
        );
        let _ = (payment_reversal_start_command_kind(&start), RECENT_HISTORY_LIMIT);
        let _ = payment_reversal_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_payment_reversal_start_receipt(
            &self.db,
            &subject,
            reversal.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_payment_reversal_start_input(
            graph,
            &binding,
            subject,
            reversal.approval_subject_version,
            actor.id(),
            &organization_id,
            &idempotency_key,
            existing_receipt,
            now,
        )?;
        let prepared = prepare_start(start_input)?;
        persist_payment_reversal_start(
            &self.db,
            reversal,
            actor,
            id,
            snapshot,
            prepared,
            adapter.owner_role,
            organization_id,
            now,
        )
        .await?;
        self.payment_reversal_detail(id).await
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_payment_reversal(
        &self,
        id: &str,
        reversal: &mut PaymentReversal,
        req: &CancelPaymentReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = payment_reversal_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_payment_reversal_binding(binding.as_ref())?.clone();
        let subject = payment_reversal_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, reversal.approval_subject_version).await?;
        let now = Instant::now();
        let input = build_payment_reversal_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_payment_reversal_domain_action(reversal, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "payment_reversal.cancel_approval",
            "payment_reversal",
            id.to_string(),
        )?;
        persist_payment_reversal_cancel(
            &self.db,
            reversal.clone(),
            prepared,
            runtime.open_tasks,
            actor.id(),
            &req.reason,
            now,
            audit,
        )
        .await
    }

    /// 按主键读取付款冲正单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_payment_reversal(&self, id: &str) -> Result<PaymentReversal> {
        self.db
            .payment_reversals()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))
    }

    /// 读取原付款往来主体与供应商。
    ///
    /// # 错误
    /// 原付款或供应商不存在、往来主体为空时返回错误。
    async fn payment_reversal_context(
        &self,
        original_payment_id: &SupplierPaymentId,
    ) -> Result<(String, SupplierAccountId)> {
        load_payment_reversal_context(&self.db, original_payment_id).await
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

        self.payment_reversal_detail(&detail_id).await
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
    /// 返回冲正单视图（含只读审批结构）。
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
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
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
            approval: payment_reversal_approval_view(binding.as_ref(), None, reversal.status),
        })
    }
}

/// 校验乐观锁版本。
///
/// # 错误
/// 不一致时返回冲突。
fn ensure_expected_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

/// 在创建事务内写入冲正单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_payment_reversal(
    db: &Database,
    rbac: &SharedRbacService,
    reversal: PaymentReversal,
    actor: AuditActor,
) -> Result<()> {
    let (organization_id, _) =
        load_payment_reversal_context(db, &reversal.original_supplier_payment_id).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::PaymentReversal,
        business_object_id: reversal.base.id.clone(),
        business_object_version: reversal.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let document = new_registered_document(
        &reversal.base.id,
        DocumentType::PaymentReversal,
        reversal.reversal_no.clone(),
    )?;
    let audit = actor.clone().resource_log(
        "payment_reversal.create",
        "payment_reversal",
        reversal.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_payment_reversal_document(&db, &rbac, document, &bind_command, &actor, session)
                    .await?;
                db.payment_reversals().create(&reversal, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 查询原付款供应商往来主体作为责任组织，并带回供应商。
///
/// # 错误
/// 原付款或供应商不存在、往来主体为空时返回错误。
async fn load_payment_reversal_context(
    db: &Database,
    original_payment_id: &SupplierPaymentId,
) -> Result<(String, SupplierAccountId)> {
    let payment = db
        .supplier_payments()
        .find_by_id(original_payment_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("原付款不存在".to_string()))?;
    let supplier = db
        .supplier_accounts()
        .find_by_id(&payment.supplier_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let organization_id = payment_reversal_responsible_org_id(&supplier.party_id.to_string())?;
    Ok((organization_id, payment.supplier_id))
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_payment_reversal_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let _ = payment_reversal_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("付款冲正单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding)?;
    db.business_documents().create(&document, session).await?;
    Ok(())
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
    let mut reversal = db
        .payment_reversals()
        .find_by_id(reversal_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
    if reversal.status == PaymentReversalStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正单据不能再过账".to_string()));
    }
    ensure_payment_reversal_final_approve_posting(&reversal)?;
    execute_payment_reversal_domain_action(
        &mut reversal,
        crate::approval::policy::ApprovalDomainAction::PaymentReversalPost,
    )?;
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
    let (reverse_rows, chunks) = plan_payment_reverse(&allocations, reversal.amount)?;
    let next_seq = allocations
        .iter()
        .map(|allocation| allocation.allocation_seq)
        .max()
        .unwrap_or(0)
        + 1;
    revert_payment_settlements(db, &chunks, actor_id, session).await?;
    persist_reverse_allocations(db, reversal, &payment, &reverse_rows, next_seq, session).await?;
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
    chunks: &[super::reversal_plan::PaymentReverseChunk],
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for chunk in chunks {
        let entry = db
            .payable_entries()
            .find_by_id(&chunk.increase_entry_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        let reverted = db
            .payable_accounts()
            .revert_settlement(&entry.payable_account_id, &chunk.amount, actor_id, session)
            .await?;
        if !reverted {
            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
        }
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
    reverse_rows: &[super::reversal_plan::PaymentReversePlanRow],
    next_seq: u32,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
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
    Ok(())
}

#[cfg(test)]
mod payment_reversal_approval_tests {
    use super::{ReturnsService, execute_payment_reversal_domain_action, start_payment_reversal_approval};
    use crate::approval::policy::ApprovalDomainAction;
    use entities::common::time::Instant;
    use entities::ids::{PaymentReversalId, SupplierPaymentId};
    use entities::money::Amount;
    use entities::returns::{PaymentReversal, PaymentReversalData, PaymentReversalStatus};
    use std::str::FromStr;

    fn draft_reversal() -> PaymentReversal {
        PaymentReversal::new(
            PaymentReversalId::new("prr-1"),
            PaymentReversalData {
                reversal_no: "PRR-1".into(),
                original_supplier_payment_id: SupplierPaymentId::new("sp-1"),
                reason_code: None,
                reason_text: "错付款冲正".into(),
                amount: Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(1),
                evidence_attachment_id: None,
            },
        )
        .expect("草稿必须可构造")
    }

    /// 创建必须注册 BusinessDocument 并绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = include_str!("payment_reversal.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::PaymentReversal"));
        assert!(source.contains("persist_created_payment_reversal"));
    }

    /// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
    #[test]
    fn create_path_calls_local_object_readable() {
        use super::super::adapter::payment_reversal_object_readable;

        let production = include_str!("payment_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("payment_reversal_object_readable"));
        assert!(!production.contains("adapter_object_read_decision"));
        assert!(payment_reversal_object_readable("org-1", "u1").unwrap());
        assert!(payment_reversal_object_readable(" ", "u1").is_err());
        assert!(payment_reversal_object_readable("org-1", "").is_err());
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("payment_reversal.rs");
        assert!(source.contains("pub async fn submit_payment_reversal"));
        assert!(source.contains("payment_reversal_start_command"));
        assert!(source.contains("reversal.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_payment_reversal，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_payment_reversal() {
        let source = include_str!("payment_reversal.rs");
        assert!(source.contains("pub async fn post_payment_reversal"));
        assert!(source.contains("reversal.mark_posted"));
        assert!(source.contains("PaymentReversalPost"));
        assert!(ReturnsService::reject_payment_reversal_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("payment_reversal.rs");
        assert!(source.contains("pub async fn cancel_payment_reversal_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_payment_reversal_cancel"));
        let _ = ReturnsService::reject_payment_reversal_client_post();
        let mut reversal = draft_reversal();
        start_payment_reversal_approval(&mut reversal).unwrap();
        execute_payment_reversal_domain_action(
            &mut reversal,
            ApprovalDomainAction::PaymentReversalCancelApproval,
        )
        .unwrap();
        assert_eq!(reversal.status, PaymentReversalStatus::Draft);
        assert_eq!(reversal.approval_subject_version, 1);
    }

    /// 生产代码不得保留草稿直接过账或待复核旁路。
    #[test]
    fn production_closes_draft_post_and_pending_review() {
        let production = include_str!("payment_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("PaymentReversalStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }
}
