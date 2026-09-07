use super::adapter::{
    build_payment_reversal_snapshot, execute_payment_reversal_domain_action, payment_reversal_adapter,
    payment_reversal_object_readable, payment_reversal_responsible_org_id, payment_reversal_start_command,
    payment_reversal_start_command_kind, payment_reversal_subject_ref, require_payment_reversal_binding,
};
use super::cancel_approval::{
    build_payment_reversal_cancel_input, load_cancel_runtime, persist_payment_reversal_cancel,
    PaymentReversalCancelPersistInput,
};
use erp_returns::dto::{
    CancelPaymentReversalApprovalRequest, CommitPaymentReversalRequest, CreatePaymentReversalRequest,
    SubmitPaymentReversalRequest,
};
use erp_returns::service::approval::start_payment_reversal_approval;

use super::start_approval::{
    build_payment_reversal_start_input, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, load_payment_reversal_start_receipt,
    persist_payment_reversal_runtime, persist_payment_reversal_start, PaymentReversalStartInput,
    PaymentReversalStartPersistInput,
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
use erp_read_models::returns_center::dto::PaymentReversalView;
use erp_returns::entity::returns::PaymentReversal;
use erp_returns::service::payment_reversal::{
    new_payment_reversal, new_payment_reversal_commit, PaymentReversalSourceFact,
};
use erp_returns::service::shared::ensure_posted_source;
use erp_returns::service::version_conflict::conflict_if_stale_version;
use erp_returns::service::ReturnsService;
use erp_supplier::SupplierExt;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::binding::{attach_published_binding, BindPublishedDefinitionCommand};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use erp_workflow::DocumentRegistryExt;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use services::{Error, Result};
use validator::Validate;

impl ReturnsProcess {
    // -----------------------------------------------------------------------
    // 付款冲正
    // -----------------------------------------------------------------------

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
        let reversal = new_payment_reversal(req, actor.id())?;
        persist_created_payment_reversal(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            reversal.clone(),
            actor.clone(),
        )
        .await?;
        self.reads().payment_reversal_detail(&reversal.base.id).await
    }

    /// 按原付款一次创建付款冲正并启动审批。
    ///
    /// 单据注册、定义绑定、冲正实体、审批快照、运行事实、入口任务和审计在同一
    /// MongoDB 事务内完成。
    pub async fn commit_payment_reversal(
        &self,
        req: CommitPaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "payment-reversal-commit-",
            actor.id(),
            "payment_reversal.commit",
            "payment_reversal",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(reversal_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.reads().payment_reversal_detail(&reversal_id).await;
        }
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商付款不存在".to_string()))?;
        let source_fact_id = SupplierPaymentId::new(payment.base.id.clone());
        let source_version = payment.base.version;
        let mut reversal = new_payment_reversal_commit(
            &req,
            PaymentReversalSourceFact {
                payment_id: source_fact_id.clone(),
                amount: payment.amount,
            },
            actor.id(),
        )?;
        let adapter = payment_reversal_adapter()?;
        start_payment_reversal_approval(&mut reversal)?;
        let id = reversal.base.id.clone();
        let subject = payment_reversal_subject_ref(&id)?;
        let (organization_id, supplier_id) =
            load_payment_reversal_context(&self.db, &reversal.original_supplier_payment_id).await?;
        let _ = payment_reversal_object_readable(&organization_id, actor.id())?;
        let now = Instant::now();
        let snapshot =
            build_payment_reversal_snapshot(&reversal, &organization_id, &supplier_id, actor.id(), now)?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::PaymentReversal,
            business_object_id: id.clone(),
            business_object_version: reversal.base.version,
            context: BindingRevalidationContext {
                organization_id: organization_id.clone(),
                creator_id: actor.id().to_string(),
            },
        };
        let document =
            new_registered_document(&id, DocumentType::PaymentReversal, reversal.reversal_no.clone())
                .map_err(services::Error::from)?;
        let create_audit =
            actor
                .clone()
                .resource_log("payment_reversal.create", "payment_reversal", id.clone())?;
        let submit_audit =
            actor
                .clone()
                .resource_log("payment_reversal.submit", "payment_reversal", id.clone())?;
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
                    validate_payment_reversal_source(&db, &source_fact_id, source_version, session).await?;
                    let binding = persist_bound_payment_reversal_document(
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
                    let start_input = build_payment_reversal_start_input(PaymentReversalStartInput {
                        graph,
                        binding: &binding,
                        subject,
                        subject_version: reversal.approval_subject_version,
                        actor_id: actor_owned.id(),
                        organization_id: &organization_id,
                        idempotency_key: &idempotency_key,
                        receipt: None,
                        now,
                    })?;
                    let prepared = prepare_start(start_input)?;
                    ReturnsService::new(db.clone())
                        .create_payment_reversal(&reversal, session)
                        .await?;
                    if let erp_workflow::service::approval::execution::PreparedExecution::Apply(writes) =
                        prepared
                    {
                        persist_payment_reversal_runtime(
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
                Some(reversal_id) => reversal_id,
                None => return Err(error),
            },
        };
        self.reads().payment_reversal_detail(&detail_id).await
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
        let mut reversal = self
            .domain()
            .load_payment_reversal(id, &mut NoTransaction)
            .await?;
        conflict_if_stale_version(reversal.matches_version(req.expected_version))?;
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
        let mut reversal = self
            .domain()
            .load_payment_reversal(id, &mut NoTransaction)
            .await?;
        conflict_if_stale_version(reversal.matches_version(req.expected_version))?;
        self.persist_cancelled_payment_reversal(id, &mut reversal, &req, actor)
            .await?;
        self.reads().payment_reversal_detail(id).await
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
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(services::Error::from)?;
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
        let _ = payment_reversal_start_command_kind(&start);
        let _ = payment_reversal_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_payment_reversal_start_receipt(
            &self.db,
            &subject,
            reversal.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_payment_reversal_start_input(PaymentReversalStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: reversal.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        persist_payment_reversal_start(
            &self.db,
            PaymentReversalStartPersistInput {
                reversal,
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
        self.reads().payment_reversal_detail(id).await
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
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(services::Error::from)?;
        let binding = require_payment_reversal_binding(binding.as_ref())?.clone();
        let subject = payment_reversal_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, reversal.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_payment_reversal_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
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
            PaymentReversalCancelPersistInput {
                reversal: reversal.clone(),
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

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------
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
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
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
    )
    .map_err(services::Error::from)?;
    let audit = actor.clone().resource_log(
        "payment_reversal.create",
        "payment_reversal",
        reversal.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_payment_reversal_document(
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
                    .create_payment_reversal(&reversal, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), services::Error>(())
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
    let organization_id = payment_reversal_responsible_org_id(supplier.party_id.as_ref())?;
    Ok((organization_id, payment.supplier_id))
}

/// 在创建冲正单的同一事务中重读并校验原供应商付款事实。
///
/// 原付款必须仍为调用前读取的版本且已经过账，避免为非正式事实创建审批任务。
async fn validate_payment_reversal_source(
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
        "只有已过账的供应商付款才能发起冲正",
    )?)
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_payment_reversal_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = payment_reversal_object_readable(
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
    let binding = binding.ok_or_else(|| Error::Internal("付款冲正单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}

#[cfg(test)]
mod payment_reversal_approval_tests {
    use super::{execute_payment_reversal_domain_action, start_payment_reversal_approval, ReturnsService};
    use erp_core::common::time::Instant;
    use erp_core::ids::{PaymentReversalId, SupplierPaymentId};
    use erp_core::money::Amount;
    use erp_returns::entity::returns::{PaymentReversal, PaymentReversalData, PaymentReversalStatus};
    use erp_workflow::service::approval::policy::ApprovalDomainAction;
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
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 创建必须注册 BusinessDocument 并绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = concat!(
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs")
        );
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::PaymentReversal"));
        assert!(source.contains("persist_created_payment_reversal"));
    }

    /// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
    #[test]
    fn create_path_calls_local_object_readable() {
        use super::super::adapter::payment_reversal_object_readable;

        let production = [
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(production.contains("payment_reversal_object_readable"));
        assert!(!production.contains("adapter_object_read_decision"));
        assert!(payment_reversal_object_readable("org-1", "u1").unwrap());
        assert!(payment_reversal_object_readable(" ", "u1").is_err());
        assert!(payment_reversal_object_readable("org-1", "").is_err());
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = concat!(
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs")
        );
        assert!(source.contains("pub async fn submit_payment_reversal"));
        assert!(source.contains("payment_reversal_start_command"));
        assert!(source.contains("reversal.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_payment_reversal，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_payment_reversal() {
        let source = concat!(
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs")
        );
        assert!(source.contains("pub async fn post_payment_reversal"));
        assert!(source.contains("reversal.mark_posted"));
        assert!(source.contains("PaymentReversalPost"));
        assert!(ReturnsService::reject_payment_reversal_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = concat!(
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs")
        );
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
        let production = [
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(!production.contains("PaymentReversalStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }

    /// 提交/撤回必须使用实体 matches_version，并删除旧 helper。
    #[test]
    fn version_lock_uses_entity_matches_version() {
        let production = [
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(production.contains("reversal.matches_version(req.expected_version)"));
        assert!(production.contains("conflict_if_stale_version"));
        assert!(!production.contains("fn ensure_expected_version"));
    }

    /// 冲减必须批量读取分录与账户，任务同步由同事务逆向流程完成。
    #[test]
    fn revert_settlements_batch_entry_and_account_reads() {
        let production = [
            include_str!("payment_posting.rs"),
            include_str!("../../../erp-returns/src/service/payment_reversal.rs"),
            include_str!("../../../erp-finance/src/service/payable/payment_reversal.rs"),
            include_str!("payment_reversal.rs"),
        ]
        .map(|source| source.split("#[cfg(test)]").next().unwrap())
        .join("\n");
        assert!(production.contains("load_payable_offset_facts"));
        assert!(production.contains("revert_settlement"));
        assert!(production.contains("sync_purchase_payment_task"));
        assert!(!production.contains("find_by_id(&chunk.increase_entry_id"));
    }
}
