use super::adapter::{
    build_supplier_refund_snapshot, ensure_supplier_refund_final_approve_posting,
    execute_supplier_refund_domain_action, require_supplier_refund_binding, start_supplier_refund_approval,
    supplier_refund_adapter, supplier_refund_approval_view, supplier_refund_object_readable,
    supplier_refund_responsible_org_id, supplier_refund_start_command, supplier_refund_start_command_kind,
    supplier_refund_subject_ref, RECENT_HISTORY_LIMIT,
};
use super::cancel_approval::{
    build_supplier_refund_cancel_input, load_cancel_runtime, persist_supplier_refund_cancel,
    SupplierRefundCancelPersistInput,
};
use super::dto::{
    CancelSupplierRefundApprovalRequest, CommitSupplierRefundRequest, CreateSupplierRefundRequest,
    SubmitSupplierRefundRequest, SupplierRefundView,
};
use super::reversal_plan::{plan_payment_reverse, zero_amount};
use super::start_approval::{
    build_supplier_refund_start_input, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, load_supplier_refund_start_receipt,
    persist_supplier_refund_runtime, persist_supplier_refund_start, SupplierRefundStartInput,
    SupplierRefundStartPersistInput,
};
use super::{return_command_no, ReturnsService};
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::{prepare_cancel, prepare_start};
use crate::audit::{AuditActor, CommandReceipt};
use crate::document_registry::{find_approval_binding, new_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use database::{
    AccessControlExt, DocumentRegistryExt, Executor, NoTransaction, PayableExt, ReturnsExt, SupplierExt,
    Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::BusinessDocument;
use entities::document_registry::DocumentType;
use entities::ids::{
    PayableEntryId, PayableEntryOffsetId, PaymentAllocationId, SupplierAccountId, SupplierPaymentId,
    SupplierRefundId,
};
use entities::money::Amount;
use entities::payable::{
    AllocationAction as PayableAllocationAction, EntryDirection as PayableEntryDirection, PayableEntry,
    PayableEntryData, PayableEntryOffset, PayableEntryOffsetData, PayableEntryType, PaymentAllocation,
    PaymentAllocationData, SupplierPaymentStatus,
};
use entities::returns::{SupplierRefund, SupplierRefundData, SupplierRefundStatus};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 供应商退款
    // -----------------------------------------------------------------------

    /// 查询供应商退款详情。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    pub async fn supplier_refund_detail(&self, id: &str) -> Result<SupplierRefundView> {
        self.supplier_refund_view(id.to_string()).await
    }

    /// 登记供应商退款草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 退款单号全局唯一（唯一索引）构成幂等去重。经办人与复核人必须不同。
    /// 绑定失败必须回滚业务实体，不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建退款单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 退款单号重复或流程未配置
    pub async fn create_supplier_refund(
        &self,
        req: CreateSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        let refund = SupplierRefund::new(
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
        )?;
        persist_created_supplier_refund(&self.db, &self.rbac, refund.clone(), actor.clone()).await?;
        self.supplier_refund_detail(&refund.base.id).await
    }

    /// 按原付款一次创建供应商退款并启动审批。
    ///
    /// 单据注册、定义绑定、退款实体、审批快照、运行事实、入口任务和审计在同一
    /// MongoDB 事务内完成。
    pub async fn commit_supplier_refund(
        &self,
        req: CommitSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        let command_receipt = CommandReceipt::new(
            "supplier-refund-commit-",
            actor,
            "supplier_refund.commit",
            "supplier_refund",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(refund_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.supplier_refund_detail(&refund_id).await;
        }
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商付款不存在".to_string()))?;
        let source_fact_id = SupplierPaymentId::new(payment.base.id.clone());
        let source_version = payment.base.version;
        let mut refund = SupplierRefund::new(
            SupplierRefundId::new(next_id()),
            SupplierRefundData {
                refund_no: return_command_no("GTK", actor.id(), &req.idempotency_key),
                purchase_return_order_id: None,
                supplier_id: payment.supplier_id.clone(),
                original_payment_id: Some(source_fact_id.clone()),
                original_payable_entry_id: None,
                reason_code: None,
                reason_text: req.reason,
                amount: req.amount.unwrap_or(payment.amount),
                handled_by: actor.id().to_string(),
                reviewed_by: "finance_reviewer".to_string(),
                occurred_at: Instant::now(),
                evidence_attachment_id: None,
            },
        )?;
        let adapter = supplier_refund_adapter()?;
        start_supplier_refund_approval(&mut refund)?;
        let id = refund.base.id.clone();
        let subject = supplier_refund_subject_ref(&id)?;
        let organization_id = load_supplier_refund_org_id(&self.db, &refund.supplier_id).await?;
        let _ = supplier_refund_object_readable(&organization_id, actor.id())?;
        let now = Instant::now();
        let snapshot = build_supplier_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::SupplierRefund,
            business_object_id: id.clone(),
            business_object_version: refund.base.version,
            context: BindingRevalidationContext {
                organization_id: organization_id.clone(),
                creator_id: actor.id().to_string(),
            },
        };
        let document = new_registered_document(&id, DocumentType::SupplierRefund, refund.refund_no.clone())?;
        let create_audit =
            actor
                .clone()
                .resource_log("supplier_refund.create", "supplier_refund", id.clone())?;
        let submit_audit =
            actor
                .clone()
                .resource_log("supplier_refund.submit", "supplier_refund", id.clone())?;
        let command_audit = command_receipt.audit(actor.clone(), id.clone())?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let idempotency_key = req.idempotency_key;
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    validate_supplier_refund_source(&db, &source_fact_id, source_version, session).await?;
                    let binding = persist_bound_supplier_refund_document(
                        &db,
                        &rbac,
                        document,
                        &bind_command,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                    let start_input = build_supplier_refund_start_input(SupplierRefundStartInput {
                        graph,
                        binding: &binding,
                        subject,
                        subject_version: refund.approval_subject_version,
                        actor_id: actor_owned.id(),
                        organization_id: &organization_id,
                        idempotency_key: &idempotency_key,
                        receipt: None,
                        now,
                    })?;
                    let prepared = prepare_start(start_input)?;
                    db.supplier_refunds().create(&refund, session).await?;
                    if let crate::approval::execution::PreparedExecution::Apply(writes) = prepared {
                        persist_supplier_refund_runtime(
                            &db,
                            &writes,
                            &snapshot,
                            adapter.owner_role,
                            &organization_id,
                            now,
                            session,
                        )
                        .await?;
                    }
                    db.audit_logs().create(&create_audit, session).await?;
                    db.audit_logs().create(&submit_audit, session).await?;
                    db.audit_logs().create(&command_audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        let detail_id = match transaction_result {
            Ok(()) => id,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(refund_id) => refund_id,
                None => return Err(error),
            },
        };
        self.supplier_refund_detail(&detail_id).await
    }

    /// 提交供应商退款并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 退款单主键
    /// * `req` - 提交请求（版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单或供应商不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_supplier_refund(
        &self,
        id: &str,
        req: SubmitSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        let adapter = supplier_refund_adapter()?;
        let mut refund = self.load_supplier_refund(id).await?;
        ensure_expected_version(refund.base.version, req.expected_version)?;
        start_supplier_refund_approval(&mut refund)?;
        self.dispatch_supplier_refund_start(id, refund, req.idempotency_key, actor, adapter)
            .await
    }

    /// 撤回供应商退款审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 退款单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    /// * `ConflictError` - 非审批中、已最终通过或并发冲突
    pub async fn cancel_supplier_refund_approval(
        &self,
        id: &str,
        req: CancelSupplierRefundApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        let mut refund = self.load_supplier_refund(id).await?;
        ensure_expected_version(refund.base.version, req.expected_version)?;
        self.persist_cancelled_supplier_refund(id, &mut refund, &req, actor)
            .await?;
        self.supplier_refund_detail(id).await
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_supplier_refund_client_post() -> Result<SupplierRefundView> {
        Err(Error::ConflictError(
            "供应商退款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string(),
        ))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_supplier_refund_start(
        &self,
        id: &str,
        refund: SupplierRefund,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::adapter::SupplierRefundAdapter,
    ) -> Result<SupplierRefundView> {
        let subject = supplier_refund_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_supplier_refund_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let organization_id = self.supplier_refund_responsible_org(&refund.supplier_id).await?;
        let snapshot = build_supplier_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let start =
            supplier_refund_start_command(id, refund.approval_subject_version, actor.id(), &idempotency_key);
        let _ = (supplier_refund_start_command_kind(&start), RECENT_HISTORY_LIMIT);
        let _ = supplier_refund_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_supplier_refund_start_receipt(
            &self.db,
            &subject,
            refund.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_supplier_refund_start_input(SupplierRefundStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: refund.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        persist_supplier_refund_start(
            &self.db,
            SupplierRefundStartPersistInput {
                refund,
                actor: actor.clone(),
                id: id.to_string(),
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
            },
        )
        .await?;
        self.supplier_refund_detail(id).await
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_supplier_refund(
        &self,
        id: &str,
        refund: &mut SupplierRefund,
        req: &CancelSupplierRefundApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = supplier_refund_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_supplier_refund_binding(binding.as_ref())?.clone();
        let subject = supplier_refund_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, refund.approval_subject_version).await?;
        let now = Instant::now();
        let input = build_supplier_refund_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_supplier_refund_domain_action(refund, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "supplier_refund.cancel_approval",
            "supplier_refund",
            id.to_string(),
        )?;
        persist_supplier_refund_cancel(
            &self.db,
            SupplierRefundCancelPersistInput {
                refund: refund.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await
    }

    /// 按主键读取供应商退款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_supplier_refund(&self, id: &str) -> Result<SupplierRefund> {
        self.db
            .supplier_refunds()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))
    }

    /// 读取供应商往来主体作为责任组织。
    ///
    /// # 错误
    /// 供应商不存在或往来主体为空时返回错误。
    async fn supplier_refund_responsible_org(&self, supplier_id: &SupplierAccountId) -> Result<String> {
        load_supplier_refund_org_id(&self.db, supplier_id).await
    }

    /// 最终通过过账（§8.3-3 事务不变量，应付侧镜像）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 按原付款（或其核销分配）反向写入 `REVERSE` 付款核销分配；按条件原子
    /// 冲减子账已核销进度；写反向应付分录（减少）与分录抵销；退款单迁移为
    /// 已过账。任一校验失败整体回滚，保留原事实。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单或原付款不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 累计退款超原付款、重复过账或超额冲减
    pub async fn post_supplier_refund(&self, id: &str, actor: &AuditActor) -> Result<SupplierRefundView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let refund_id = id.to_string();
        let detail_id = refund_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    apply_supplier_refund_final_post(&db, &refund_id, &actor_id, &actor_owned, session).await
                })
            })
            .await?;

        self.supplier_refund_detail(&detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配供应商退款单视图。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    async fn supplier_refund_view(&self, id: String) -> Result<SupplierRefundView> {
        let refund = self
            .db
            .supplier_refunds()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(SupplierRefundView {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no,
            status: refund.status,
            purchase_return_order_id: refund.purchase_return_order_id.map(|id| id.to_string()),
            supplier_id: refund.supplier_id.to_string(),
            original_payment_id: refund.original_payment_id.map(|id| id.to_string()),
            original_payable_entry_id: refund.original_payable_entry_id.map(|id| id.to_string()),
            reason_code: refund.reason_code,
            reason_text: refund.reason_text,
            amount: refund.amount,
            handled_by: refund.handled_by,
            reviewed_by: refund.reviewed_by,
            occurred_at: refund.occurred_at,
            version: refund.base.version,
            created_at: refund.base.created_at,
            approval: supplier_refund_approval_view(binding.as_ref(), None, refund.status),
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

/// 在创建事务内写入退款单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_supplier_refund(
    db: &Database,
    rbac: &SharedRbacService,
    refund: SupplierRefund,
    actor: AuditActor,
) -> Result<()> {
    let organization_id = load_supplier_refund_org_id(db, &refund.supplier_id).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::SupplierRefund,
        business_object_id: refund.base.id.clone(),
        business_object_version: refund.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let document = new_registered_document(
        &refund.base.id,
        DocumentType::SupplierRefund,
        refund.refund_no.clone(),
    )?;
    let audit = actor.clone().resource_log(
        "supplier_refund.create",
        "supplier_refund",
        refund.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_supplier_refund_document(&db, &rbac, document, &bind_command, &actor, session)
                    .await?;
                db.supplier_refunds().create(&refund, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 查询供应商往来主体作为责任组织。
///
/// # 错误
/// 供应商不存在或往来主体为空时返回错误。
async fn load_supplier_refund_org_id(db: &Database, supplier_id: &SupplierAccountId) -> Result<String> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    supplier_refund_responsible_org_id(supplier.party_id.as_ref())
}

/// 在创建退款的同一事务中重读并校验原供应商付款事实。
///
/// 原事实必须仍为调用前读取的版本且已经过账，避免从草稿、审批中或已冲正付款
/// 派生无法最终执行的退款审批。
async fn validate_supplier_refund_source(
    db: &Database,
    source_fact_id: &SupplierPaymentId,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let payment = db
        .supplier_payments()
        .find_by_id(source_fact_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原供应商付款不存在".to_string()))?;
    super::ensure_posted_source(
        payment.base.version,
        expected_version,
        payment.status == SupplierPaymentStatus::Posted,
        "只有已过账的供应商付款才能发起退款",
    )
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_supplier_refund_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<entities::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = supplier_refund_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("供应商退款单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}

/// 在最终通过事务内执行过账副作用并写回退款单。
///
/// # 错误
/// 非审批中、原付款不存在或仓储失败时返回错误。
pub(super) async fn apply_supplier_refund_final_post(
    db: &Database,
    refund_id: &str,
    actor_id: &str,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let mut refund = db
        .supplier_refunds()
        .find_by_id(refund_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
    if refund.status == SupplierRefundStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正退款不能再过账".to_string()));
    }
    ensure_supplier_refund_final_approve_posting(&refund)?;
    execute_supplier_refund_domain_action(
        &mut refund,
        crate::approval::policy::ApprovalDomainAction::SupplierRefundPost,
    )?;
    apply_supplier_refund_posting(db, &refund, actor_id, session).await?;
    refund.mark_posted()?;
    db.supplier_refunds().update(&mut refund, session).await?;
    let audit =
        actor
            .clone()
            .resource_log("supplier_refund.post", "supplier_refund", refund.base.id.clone())?;
    db.audit_logs().create(&audit, session).await?;
    Ok(())
}

/// 在调用方事务内写入退款入账副作用。
///
/// # 错误
/// 原付款不存在、累计超额或仓储失败时返回错误。
async fn apply_supplier_refund_posting(
    db: &Database,
    refund: &SupplierRefund,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let original_payment_id = refund
        .original_payment_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("按原应付分录退款由冲减分录完成".to_string()))?;
    let payment = db
        .supplier_payments()
        .find_by_id(&original_payment_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("原付款不存在".to_string()))?;
    if payment.status != SupplierPaymentStatus::Posted {
        return Err(Error::BusinessLogicError("只有已过账付款可以退款".to_string()));
    }
    let refunded_before: Amount = db
        .supplier_refunds()
        .find_refunds_by_originals(std::slice::from_ref(&original_payment_id), &[], session)
        .await?
        .iter()
        .filter(|other| other.base.id != refund.base.id)
        .fold(zero_amount(), |sum, other| sum.checked_add(other.amount));
    if refunded_before.checked_add(refund.amount) > payment.amount {
        return Err(Error::BusinessLogicError(
            "累计退款金额不得超过原付款金额".to_string(),
        ));
    }
    persist_refund_offsets_and_reversals(db, refund, &payment, actor_id, session).await
}

/// 写入反向核销分配、冲减进度与减少分录。
///
/// # 错误
/// 跨供应商、超额冲减或仓储失败时返回错误。
async fn persist_refund_offsets_and_reversals(
    db: &Database,
    refund: &SupplierRefund,
    payment: &entities::payable::SupplierPayment,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let allocations = db
        .payment_allocations()
        .find_allocations_by_payments(&[payment.base.id.clone().into()], session)
        .await?;
    let (reverse_rows, chunks) = plan_payment_reverse(&allocations, refund.amount)?;
    let next_seq = allocations
        .iter()
        .map(|allocation| allocation.allocation_seq)
        .max()
        .unwrap_or(0)
        + 1;
    let decrease_entry = create_decrease_offsets(db, refund, payment, actor_id, &chunks, session).await?;
    if let Some(entry) = decrease_entry {
        db.payable_entries().create(&entry, session).await?;
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
                allocated_at: refund.occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        db.payment_allocations().create(&allocation, session).await?;
    }
    Ok(())
}

/// 按冲减块写减少分录抵销并回冲已核销进度。
///
/// # 错误
/// 分录缺失、跨供应商或超额冲减时返回错误。
async fn create_decrease_offsets(
    db: &Database,
    refund: &SupplierRefund,
    payment: &entities::payable::SupplierPayment,
    actor_id: &str,
    chunks: &[super::reversal_plan::PaymentReverseChunk],
    session: &mut mongodb::ClientSession,
) -> Result<Option<PayableEntry>> {
    let mut decrease_entry: Option<PayableEntry> = None;
    for (offset_index, chunk) in chunks.iter().enumerate() {
        let entry = db
            .payable_entries()
            .find_by_id(&chunk.increase_entry_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        let account = db
            .payable_accounts()
            .find_by_id(&entry.payable_account_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
        if account.supplier_id != payment.supplier_id {
            return Err(Error::BusinessLogicError("禁止跨供应商退款".to_string()));
        }
        let reverted = db
            .payable_accounts()
            .revert_settlement(&entry.payable_account_id, &chunk.amount, actor_id, session)
            .await?;
        if !reverted {
            return Err(Error::BusinessLogicError("退款冲减超过已核销金额".to_string()));
        }
        if decrease_entry.is_none() {
            decrease_entry = Some(build_decrease_entry(refund, &entry.payable_account_id)?);
        }
        persist_decrease_offset(db, decrease_entry.as_ref(), chunk, offset_index, session).await?;
    }
    Ok(decrease_entry)
}

/// 构造供应商退款减少应付分录。
///
/// # 错误
/// 分录字段校验失败时返回错误。
fn build_decrease_entry(
    refund: &SupplierRefund,
    payable_account_id: &entities::ids::PayableAccountId,
) -> Result<PayableEntry> {
    Ok(PayableEntry::new(
        PayableEntryId::new(next_id()),
        PayableEntryData {
            payable_account_id: payable_account_id.clone(),
            entry_type: PayableEntryType::SupplierRefund,
            direction: PayableEntryDirection::Decrease,
            amount: refund.amount,
            due_date: entities::common::time::BusinessDate::today(),
            source_fact_type: "supplier_refund".to_string(),
            source_document_id: refund.base.id.clone(),
            source_revision_id: refund.base.id.clone(),
            source_sequence: 1,
            posted_at: refund.occurred_at,
        },
    )?)
}

/// 写入一条减少分录抵销。
///
/// # 错误
/// 减少分录缺失或仓储失败时返回错误。
async fn persist_decrease_offset(
    db: &Database,
    decrease_entry: Option<&PayableEntry>,
    chunk: &super::reversal_plan::PaymentReverseChunk,
    offset_index: usize,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let decrease_id = decrease_entry
        .ok_or_else(|| Error::Internal("退款减少分录未创建".to_string()))?
        .base
        .id
        .clone()
        .into();
    db.payable_entry_offsets()
        .create(
            &PayableEntryOffset::new(
                PayableEntryOffsetId::new(next_id()),
                PayableEntryOffsetData {
                    decrease_entry_id: decrease_id,
                    increase_entry_id: chunk.increase_entry_id.clone(),
                    offset_sequence: offset_index as u32 + 1,
                    offset_amount: chunk.amount,
                },
            )?,
            session,
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod supplier_refund_approval_tests {
    use super::{execute_supplier_refund_domain_action, start_supplier_refund_approval, ReturnsService};
    use crate::approval::policy::ApprovalDomainAction;
    use entities::common::time::Instant;
    use entities::ids::{SupplierAccountId, SupplierPaymentId, SupplierRefundId};
    use entities::money::Amount;
    use entities::returns::{SupplierRefund, SupplierRefundData, SupplierRefundStatus};
    use std::str::FromStr;

    fn draft_refund() -> SupplierRefund {
        SupplierRefund::new(
            SupplierRefundId::new("srf-1"),
            SupplierRefundData {
                refund_no: "SRF-1".into(),
                purchase_return_order_id: None,
                supplier_id: SupplierAccountId::new("sup-1"),
                original_payment_id: Some(SupplierPaymentId::new("sp-1")),
                original_payable_entry_id: None,
                reason_code: None,
                reason_text: "错付款退回".into(),
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
        let source = include_str!("supplier_refund.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::SupplierRefund"));
        assert!(source.contains("persist_created_supplier_refund"));
    }

    /// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
    #[test]
    fn create_path_calls_local_object_readable() {
        use super::super::adapter::supplier_refund_object_readable;

        let production = include_str!("supplier_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("supplier_refund_object_readable"));
        assert!(!production.contains("adapter_object_read_decision"));
        assert!(supplier_refund_object_readable("org-1", "u1").unwrap());
        assert!(supplier_refund_object_readable(" ", "u1").is_err());
        assert!(supplier_refund_object_readable("org-1", "").is_err());
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("supplier_refund.rs");
        assert!(source.contains("pub async fn submit_supplier_refund"));
        assert!(source.contains("supplier_refund_start_command"));
        assert!(source.contains("refund.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_supplier_refund，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_supplier_refund() {
        let source = include_str!("supplier_refund.rs");
        assert!(source.contains("pub async fn post_supplier_refund"));
        assert!(source.contains("refund.mark_posted"));
        assert!(source.contains("SupplierRefundPost"));
        assert!(ReturnsService::reject_supplier_refund_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("supplier_refund.rs");
        assert!(source.contains("pub async fn cancel_supplier_refund_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_supplier_refund_cancel"));
        let _ = ReturnsService::reject_supplier_refund_client_post();
        let mut refund = draft_refund();
        start_supplier_refund_approval(&mut refund).unwrap();
        execute_supplier_refund_domain_action(
            &mut refund,
            ApprovalDomainAction::SupplierRefundCancelApproval,
        )
        .unwrap();
        assert_eq!(refund.status, SupplierRefundStatus::Draft);
        assert_eq!(refund.approval_subject_version, 1);
    }

    /// 生产代码不得保留草稿直接过账或待复核旁路。
    #[test]
    fn production_closes_draft_post_and_pending_review() {
        let production = include_str!("supplier_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("SupplierRefundStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }
}
