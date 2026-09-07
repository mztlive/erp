use super::adapter::{
    build_supplier_refund_snapshot, execute_supplier_refund_domain_action, require_supplier_refund_binding,
    supplier_refund_adapter, supplier_refund_object_readable, supplier_refund_responsible_org_id,
    supplier_refund_start_command, supplier_refund_start_command_kind, supplier_refund_subject_ref,
};
use super::cancel_approval::{
    build_supplier_refund_cancel_input, load_cancel_runtime, persist_supplier_refund_cancel,
    SupplierRefundCancelPersistInput,
};
use erp_returns::dto::{
    CancelSupplierRefundApprovalRequest, CommitSupplierRefundRequest, CreateSupplierRefundRequest,
    SubmitSupplierRefundRequest,
};
use erp_returns::service::approval::start_supplier_refund_approval;

use super::start_approval::{
    build_supplier_refund_start_input, ensure_return_start_actor_active,
    ensure_return_start_replay_authorized, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, load_supplier_refund_start_receipt,
    persist_supplier_refund_runtime, persist_supplier_refund_start, replay_return_start_with_executor,
    replay_subject_versions, ReplayReturnStartInput, SupplierRefundStartInput,
    SupplierRefundStartPersistInput,
};
use super::ReturnsProcess;
use application_core::AuditActor;
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::AuditExt;
use erp_audit::CommandReceiptServiceExt as _;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierPaymentId};
use erp_finance::entity::payable::SupplierPaymentStatus;
use erp_finance::repository::PayableExt;
use erp_identity::SharedRbacService;
use erp_read_models::returns_center::dto::SupplierRefundView;
use erp_returns::entity::returns::SupplierRefund;
use erp_returns::service::shared::ensure_posted_source;
use erp_returns::service::supplier_refund::{
    new_supplier_refund, new_supplier_refund_commit, SupplierRefundSourceFact,
};
use erp_returns::service::version_conflict::conflict_if_stale_version;
use erp_returns::service::ReturnsService;
use erp_supplier::SupplierExt;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::binding::{attach_published_binding, BindPublishedDefinitionCommand};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use erp_workflow::DocumentRegistryExt;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use services::{Error, Result};
use validator::Validate;

impl ReturnsProcess {
    // -----------------------------------------------------------------------
    // 供应商退款
    // -----------------------------------------------------------------------

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
        let refund = new_supplier_refund(req, actor.id())?;
        persist_created_supplier_refund(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            refund.clone(),
            actor.clone(),
        )
        .await?;
        self.reads().supplier_refund_detail(&refund.base.id).await
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
        let command_receipt = CommandReceipt::from_payload(
            "supplier-refund-commit-",
            actor.id(),
            "supplier_refund.commit",
            "supplier_refund",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(refund_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.reads().supplier_refund_detail(&refund_id).await;
        }
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商付款不存在".to_string()))?;
        let source_fact_id = SupplierPaymentId::new(payment.base.id.clone());
        let source_version = payment.base.version;
        let mut refund = new_supplier_refund_commit(
            &req,
            SupplierRefundSourceFact {
                payment_id: source_fact_id.clone(),
                supplier_id: payment.supplier_id.clone(),
                amount: payment.amount,
            },
            actor.id(),
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
        let document = new_registered_document(&id, DocumentType::SupplierRefund, refund.refund_no.clone())
            .map_err(services::Error::from)?;
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
        let object_read = std::sync::Arc::clone(&self.object_read);
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
                        object_read.as_ref(),
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
                    ReturnsService::new(db.clone())
                        .create_supplier_refund(&refund, session)
                        .await?;
                    if let erp_workflow::service::approval::execution::PreparedExecution::Apply(writes) =
                        prepared
                    {
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
                    Ok::<(), services::Error>(())
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
        self.reads().supplier_refund_detail(&detail_id).await
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
        mut req: SubmitSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        req.idempotency_key = normalize_idempotency_key(&req.idempotency_key)?
            .as_str()
            .to_string();
        if self
            .replay_supplier_refund_start(id, &req.idempotency_key, actor)
            .await?
            .is_some()
        {
            return self.reads().supplier_refund_detail(id).await;
        }
        let adapter = supplier_refund_adapter()?;
        let mut refund = self.domain().load_supplier_refund(id, &mut NoTransaction).await?;
        conflict_if_stale_version(refund.matches_version(req.expected_version))?;
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
        let mut refund = self.domain().load_supplier_refund(id, &mut NoTransaction).await?;
        conflict_if_stale_version(refund.matches_version(req.expected_version))?;
        self.persist_cancelled_supplier_refund(id, &mut refund, &req, actor)
            .await?;
        self.reads().supplier_refund_detail(id).await
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
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(services::Error::from)?;
        let binding = require_supplier_refund_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let organization_id = self.supplier_refund_responsible_org(&refund.supplier_id).await?;
        let snapshot = build_supplier_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let start =
            supplier_refund_start_command(id, refund.approval_subject_version, actor.id(), &idempotency_key);
        let _ = supplier_refund_start_command_kind(&start);
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
        let recovery_subject_version = refund.approval_subject_version;
        let persisted = persist_supplier_refund_start(
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
        .await;
        if let Err(error) = persisted {
            if !error.command_may_have_committed() {
                return Err(error);
            }
            self.recover_supplier_refund_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        self.reads().supplier_refund_detail(id).await
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_supplier_refund_start(
        &self,
        refund_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let recovered = self
                .replay_supplier_refund_start_version(refund_id, subject_version, idempotency_key, actor)
                .await;
            match recovered {
                Ok(Some(instance_id)) => return Ok(instance_id),
                Ok(None) => {}
                Err(error) if error.command_may_have_committed() => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 在任何当前状态/版本门禁前，以当前或下一主题版本精确回放既有启动。
    async fn replay_supplier_refund_start(
        &self,
        refund_id: &str,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<Option<String>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let refund_id = refund_id.to_string();
        let idempotency_key = idempotency_key.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    ensure_return_start_actor_active(&db, &rbac, &actor, session).await?;
                    let refund = ReturnsService::new(db.clone())
                        .load_supplier_refund(&refund_id, session)
                        .await?;
                    let supplier = db
                        .supplier_accounts()
                        .find_by_id(&refund.supplier_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
                    let organization_id = supplier_refund_responsible_org_id(supplier.party_id.as_ref())?;
                    ensure_return_start_replay_authorized(
                        &db,
                        &rbac,
                        &actor,
                        DocumentType::SupplierRefund,
                        "supplier_refund:submit",
                        &organization_id,
                        session,
                    )
                    .await?;
                    let binding = find_approval_binding(&db, &refund_id, session)
                        .await
                        .map_err(services::Error::from)?;
                    let binding = require_supplier_refund_binding(binding.as_ref())?;
                    let subject = supplier_refund_subject_ref(&refund_id)?;
                    for subject_version in replay_subject_versions(refund.approval_subject_version)? {
                        if let Some(instance_id) = replay_return_start_with_executor(
                            &db,
                            ReplayReturnStartInput {
                                document_type: DocumentType::SupplierRefund,
                                subject: &subject,
                                subject_version,
                                idempotency_key: &idempotency_key,
                                binding,
                                actor_id: actor.id(),
                            },
                            session,
                        )
                        .await?
                        {
                            return Ok(Some(instance_id));
                        }
                    }
                    Ok(None)
                })
            })
            .await
    }

    /// 单一候选版本的 fresh-session 当前授权与 exact receipt 回放。
    async fn replay_supplier_refund_start_version(
        &self,
        refund_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<Option<String>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let refund_id = refund_id.to_string();
        let idempotency_key = idempotency_key.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    ensure_return_start_actor_active(&db, &rbac, &actor, session).await?;
                    let refund = ReturnsService::new(db.clone())
                        .load_supplier_refund(&refund_id, session)
                        .await?;
                    let supplier = db
                        .supplier_accounts()
                        .find_by_id(&refund.supplier_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
                    let organization_id = supplier_refund_responsible_org_id(supplier.party_id.as_ref())?;
                    ensure_return_start_replay_authorized(
                        &db,
                        &rbac,
                        &actor,
                        DocumentType::SupplierRefund,
                        "supplier_refund:submit",
                        &organization_id,
                        session,
                    )
                    .await?;
                    let binding = find_approval_binding(&db, &refund_id, session)
                        .await
                        .map_err(services::Error::from)?;
                    let binding = require_supplier_refund_binding(binding.as_ref())?;
                    let subject = supplier_refund_subject_ref(&refund_id)?;
                    replay_return_start_with_executor(
                        &db,
                        ReplayReturnStartInput {
                            document_type: DocumentType::SupplierRefund,
                            subject: &subject,
                            subject_version,
                            idempotency_key: &idempotency_key,
                            binding,
                            actor_id: actor.id(),
                        },
                        session,
                    )
                    .await
                })
            })
            .await
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
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(services::Error::from)?;
        let binding = require_supplier_refund_binding(binding.as_ref())?.clone();
        let subject = supplier_refund_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, refund.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_supplier_refund_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
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
        let _object_read = std::sync::Arc::clone(&self.object_read);
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

        self.reads().supplier_refund_detail(&detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------
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
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
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
    )
    .map_err(services::Error::from)?;
    let audit = actor.clone().resource_log(
        "supplier_refund.create",
        "supplier_refund",
        refund.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_supplier_refund_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    document,
                    &bind_command,
                    &actor,
                    session,
                )
                .await?;
                ReturnsService::new(db.clone())
                    .create_supplier_refund(&refund, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), services::Error>(())
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
    Ok(ensure_posted_source(
        payment.base.version,
        expected_version,
        payment.status == SupplierPaymentStatus::Posted,
        "只有已过账的供应商付款才能发起退款",
    )?)
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_supplier_refund_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = supplier_refund_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = services::workflow_compose::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        session,
    )
    .await?;
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
    let mut refund = ReturnsService::new(db.clone())
        .prepare_supplier_refund_post(refund_id, session)
        .await?;
    execute_supplier_refund_domain_action(
        &mut refund,
        erp_workflow::service::approval::policy::ApprovalDomainAction::SupplierRefundPost,
    )?;
    apply_supplier_refund_posting(db, &refund, actor_id, session).await?;
    ReturnsService::new(db.clone())
        .persist_supplier_refund_post(&mut refund, session)
        .await?;
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
    let original_payment_id = erp_returns::service::supplier_refund::original_payment_id(refund)?;
    let payment = erp_finance::service::payable::supplier_refund::load_posted_payment_for_refund(
        db,
        &original_payment_id,
        session,
    )
    .await?;
    ReturnsService::new(db.clone())
        .validate_supplier_refund_amount(refund, payment.amount, session)
        .await?;
    erp_finance::service::payable::supplier_refund::persist_refund_offsets_and_reversals(
        db,
        &erp_finance::service::payable::supplier_refund::SupplierRefundPostingFact {
            refund_id: refund.base.id.clone(),
            amount: refund.amount,
            occurred_at: refund.occurred_at,
        },
        &payment,
        actor_id,
        session,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod supplier_refund_approval_tests {
    use super::{execute_supplier_refund_domain_action, start_supplier_refund_approval, ReturnsService};
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierAccountId, SupplierPaymentId, SupplierRefundId};
    use erp_core::money::Amount;
    use erp_returns::entity::returns::{SupplierRefund, SupplierRefundData, SupplierRefundStatus};
    use erp_workflow::service::approval::policy::ApprovalDomainAction;
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
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 创建必须注册 BusinessDocument 并绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = concat!(
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs")
        );
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::SupplierRefund"));
        assert!(source.contains("persist_created_supplier_refund"));
    }

    /// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
    #[test]
    fn create_path_calls_local_object_readable() {
        use super::super::adapter::supplier_refund_object_readable;

        let production = [
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(production.contains("supplier_refund_object_readable"));
        assert!(!production.contains("adapter_object_read_decision"));
        assert!(supplier_refund_object_readable("org-1", "u1").unwrap());
        assert!(supplier_refund_object_readable(" ", "u1").is_err());
        assert!(supplier_refund_object_readable("org-1", "").is_err());
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = concat!(
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs")
        );
        assert!(source.contains("pub async fn submit_supplier_refund"));
        assert!(source.contains("supplier_refund_start_command"));
        assert!(source.contains("refund.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_supplier_refund，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_supplier_refund() {
        let source = concat!(
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs")
        );
        assert!(source.contains("pub async fn post_supplier_refund"));
        assert!(source.contains("refund.mark_posted"));
        assert!(source.contains("SupplierRefundPost"));
        assert!(ReturnsService::reject_supplier_refund_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = concat!(
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs")
        );
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
        let production = [
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(!production.contains("SupplierRefundStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }

    /// 提交/撤回必须使用实体 matches_version，并删除旧 helper。
    #[test]
    fn version_lock_uses_entity_matches_version() {
        let production = [
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(production.contains("refund.matches_version(req.expected_version)"));
        assert!(production.contains("conflict_if_stale_version"));
        assert!(!production.contains("fn ensure_expected_version"));
    }

    /// 冲减块必须批量读取分录与账户，逐账户原子回冲仍留在 Service。
    #[test]
    fn decrease_offsets_batch_entry_and_account_reads() {
        let production = [
            include_str!("supplier_refund.rs"),
            include_str!("../../../erp-returns/src/service/supplier_refund.rs"),
            include_str!("../../../erp-finance/src/service/payable/supplier_refund.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(production.contains("load_payable_offset_facts"));
        assert!(production.contains("revert_settlement"));
        assert!(!production.contains("find_by_id(&chunk.increase_entry_id"));
        assert!(!production.contains("find_by_id(&entry.payable_account_id"));
    }
}
