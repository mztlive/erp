use super::adapter::{
    build_receipt_reversal_snapshot, ensure_receipt_reversal_final_approve_posting,
    execute_receipt_reversal_domain_action, receipt_reversal_adapter, receipt_reversal_approval_view,
    receipt_reversal_object_readable, receipt_reversal_responsible_org_id, receipt_reversal_start_command,
    receipt_reversal_start_command_kind, receipt_reversal_subject_ref, require_receipt_reversal_binding,
    start_receipt_reversal_approval, RECENT_HISTORY_LIMIT,
};
use super::cancel_approval::{
    build_receipt_reversal_cancel_input, load_cancel_runtime, persist_receipt_reversal_cancel,
    ReceiptReversalCancelPersistInput,
};
use super::dto::{
    CancelReceiptReversalApprovalRequest, CommitReceiptReversalRequest, CreateReceiptReversalRequest,
    ReceiptReversalView, SubmitReceiptReversalRequest,
};
use std::str::FromStr;

use super::offset_batch::load_receivable_offset_facts;
use super::start_approval::{
    build_receipt_reversal_start_input, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, load_receipt_reversal_start_receipt,
    persist_receipt_reversal_runtime, persist_receipt_reversal_start, ReceiptReversalStartInput,
    ReceiptReversalStartPersistInput,
};
use super::version_conflict::conflict_if_stale_version;
use super::{return_command_no, ReturnsService};
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::idempotency::normalize_idempotency_key;
use crate::approval::execution::{prepare_cancel, prepare_start};
use crate::audit::{AuditActor, CommandReceipt, CommandReceiptServiceExt as _};
use crate::document_registry::{find_approval_binding, new_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use database::{
    AccessControlExt, DocumentRegistryExt, Executor, NoTransaction, ReceivableExt, ReturnsExt, Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::BusinessDocument;
use entities::document_registry::DocumentType;
use entities::ids::{CustomerAccountId, CustomerReceiptId, ReceiptAllocationId, ReceiptReversalId};
use entities::money::Amount;
use entities::receivable::{
    AllocationAction as ReceivableAllocationAction, CustomerReceipt, CustomerReceiptStatus,
    ReceiptAllocation, ReceiptAllocationData,
};
use entities::returns::{ReceiptReversal, ReceiptReversalData, ReceiptReversalStatus};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 回款冲正
    // -----------------------------------------------------------------------

    /// 查询回款冲正详情。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    pub async fn receipt_reversal_detail(&self, id: &str) -> Result<ReceiptReversalView> {
        self.receipt_reversal_view(id.to_string()).await
    }

    /// 登记回款冲正草稿，并在同一事务绑定已发布审批定义。
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
    /// * `NotFound` - 原回款不存在
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
            actor.id(),
        )?;
        persist_created_receipt_reversal(&self.db, &self.rbac, reversal.clone(), actor.clone()).await?;
        self.receipt_reversal_detail(&reversal.base.id).await
    }

    /// 按原回款一次创建回款冲正并启动审批。
    ///
    /// 单据注册、定义绑定、冲正实体、审批快照、运行事实、入口任务和审计在同一
    /// MongoDB 事务内完成。
    pub async fn commit_receipt_reversal(
        &self,
        req: CommitReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "receipt-reversal-commit-",
            actor.id(),
            "receipt_reversal.commit",
            "receipt_reversal",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(reversal_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.receipt_reversal_detail(&reversal_id).await;
        }
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))?;
        let source_fact_id = CustomerReceiptId::new(receipt.base.id.clone());
        let source_version = receipt.base.version;
        let mut reversal = ReceiptReversal::new(
            ReceiptReversalId::new(next_id()),
            ReceiptReversalData {
                reversal_no: return_command_no("CZ", actor.id(), &req.idempotency_key),
                original_customer_receipt_id: source_fact_id.clone(),
                reason_code: None,
                reason_text: req.reason,
                amount: req.amount.unwrap_or(receipt.amount),
                handled_by: actor.id().to_string(),
                reviewed_by: "finance_reviewer".to_string(),
                occurred_at: Instant::now(),
                evidence_attachment_id: None,
            },
            actor.id(),
        )?;
        let adapter = receipt_reversal_adapter()?;
        start_receipt_reversal_approval(&mut reversal)?;
        let id = reversal.base.id.clone();
        let subject = receipt_reversal_subject_ref(&id)?;
        let (organization_id, customer_id) =
            load_receipt_reversal_context(&self.db, &reversal.original_customer_receipt_id).await?;
        let _ = receipt_reversal_object_readable(&organization_id, actor.id())?;
        let now = Instant::now();
        let snapshot = build_receipt_reversal_snapshot(
            &reversal,
            &organization_id,
            customer_id.as_ref(),
            actor.id(),
            now,
        )?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::ReceiptReversal,
            business_object_id: id.clone(),
            business_object_version: reversal.base.version,
            context: BindingRevalidationContext {
                organization_id: organization_id.clone(),
                creator_id: actor.id().to_string(),
            },
        };
        let document =
            new_registered_document(&id, DocumentType::ReceiptReversal, reversal.reversal_no.clone())?;
        let create_audit =
            actor
                .clone()
                .resource_log("receipt_reversal.create", "receipt_reversal", id.clone())?;
        let submit_audit =
            actor
                .clone()
                .resource_log("receipt_reversal.submit", "receipt_reversal", id.clone())?;
        let command_audit = command_receipt.audit(actor.clone(), id.clone())?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let idempotency_key = req.idempotency_key;
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    validate_receipt_reversal_source(&db, &source_fact_id, source_version, session).await?;
                    let binding = persist_bound_receipt_reversal_document(
                        &db,
                        &rbac,
                        document,
                        &bind_command,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                    let start_input = build_receipt_reversal_start_input(ReceiptReversalStartInput {
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
                    db.receipt_reversals().create(&reversal, session).await?;
                    if let crate::approval::execution::PreparedExecution::Apply(writes) = prepared {
                        persist_receipt_reversal_runtime(
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
                Some(reversal_id) => reversal_id,
                None => return Err(error),
            },
        };
        self.receipt_reversal_detail(&detail_id).await
    }

    /// 提交回款冲正并调用统一 `start_approval`。
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
    /// * `NotFound` - 冲正单或原回款不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_receipt_reversal(
        &self,
        id: &str,
        req: SubmitReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let adapter = receipt_reversal_adapter()?;
        let mut reversal = self.load_receipt_reversal(id).await?;
        conflict_if_stale_version(reversal.matches_version(req.expected_version))?;
        start_receipt_reversal_approval(&mut reversal)?;
        self.dispatch_receipt_reversal_start(id, reversal, req.idempotency_key, actor, adapter)
            .await
    }

    /// 撤回回款冲正审批，成功后回到草稿且 `subject_version` 不回退。
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
    pub async fn cancel_receipt_reversal_approval(
        &self,
        id: &str,
        req: CancelReceiptReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let mut reversal = self.load_receipt_reversal(id).await?;
        conflict_if_stale_version(reversal.matches_version(req.expected_version))?;
        self.persist_cancelled_receipt_reversal(id, &mut reversal, &req, actor)
            .await?;
        self.receipt_reversal_detail(id).await
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_receipt_reversal_client_post() -> Result<ReceiptReversalView> {
        Err(Error::ConflictError(
            "回款冲正过账只能由审批最终通过动作执行，客户端不得直接过账".to_string(),
        ))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_receipt_reversal_start(
        &self,
        id: &str,
        reversal: ReceiptReversal,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::adapter::ReceiptReversalAdapter,
    ) -> Result<ReceiptReversalView> {
        let subject = receipt_reversal_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_receipt_reversal_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let (organization_id, customer_id) = self
            .receipt_reversal_context(&reversal.original_customer_receipt_id)
            .await?;
        let snapshot = build_receipt_reversal_snapshot(
            &reversal,
            &organization_id,
            customer_id.as_ref(),
            actor.id(),
            now,
        )?;
        let start = receipt_reversal_start_command(
            id,
            reversal.approval_subject_version,
            actor.id(),
            &idempotency_key,
        );
        let _ = (receipt_reversal_start_command_kind(&start), RECENT_HISTORY_LIMIT);
        let _ = receipt_reversal_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_receipt_reversal_start_receipt(
            &self.db,
            &subject,
            reversal.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_receipt_reversal_start_input(ReceiptReversalStartInput {
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
        persist_receipt_reversal_start(
            &self.db,
            ReceiptReversalStartPersistInput {
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
        self.receipt_reversal_detail(id).await
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_receipt_reversal(
        &self,
        id: &str,
        reversal: &mut ReceiptReversal,
        req: &CancelReceiptReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = receipt_reversal_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_receipt_reversal_binding(binding.as_ref())?.clone();
        let subject = receipt_reversal_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, reversal.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_receipt_reversal_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_receipt_reversal_domain_action(reversal, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "receipt_reversal.cancel_approval",
            "receipt_reversal",
            id.to_string(),
        )?;
        persist_receipt_reversal_cancel(
            &self.db,
            ReceiptReversalCancelPersistInput {
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

    /// 按主键读取回款冲正单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_receipt_reversal(&self, id: &str) -> Result<ReceiptReversal> {
        self.db
            .receipt_reversals()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))
    }

    /// 读取原回款往来主体与可选客户。
    ///
    /// # 错误
    /// 原回款不存在或往来主体为空时返回错误。
    async fn receipt_reversal_context(
        &self,
        original_receipt_id: &CustomerReceiptId,
    ) -> Result<(String, Option<CustomerAccountId>)> {
        load_receipt_reversal_context(&self.db, original_receipt_id).await
    }

    /// 最终通过过账（§8.3-3 事务不变量）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 按原回款核销分配反向写入 `REVERSE` 分配并原子冲减子账已核销进度；
    /// 原回款迁移为已冲正；冲正单迁移为已过账。任一校验失败整体回滚。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原回款不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 累计冲正超原回款、重复过账或超额冲减
    pub async fn post_receipt_reversal(&self, id: &str, actor: &AuditActor) -> Result<ReceiptReversalView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let reversal_id = id.to_string();
        let detail_id = reversal_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    apply_receipt_reversal_final_post(&db, &reversal_id, &actor_id, &actor_owned, session)
                        .await
                })
            })
            .await?;

        self.receipt_reversal_detail(&detail_id).await
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
    /// 返回冲正单视图（含只读审批结构）。
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
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
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
            approval: receipt_reversal_approval_view(binding.as_ref(), None, reversal.status),
        })
    }
}

/// 在创建事务内写入冲正单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_receipt_reversal(
    db: &Database,
    rbac: &SharedRbacService,
    reversal: ReceiptReversal,
    actor: AuditActor,
) -> Result<()> {
    let (organization_id, _) =
        load_receipt_reversal_context(db, &reversal.original_customer_receipt_id).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::ReceiptReversal,
        business_object_id: reversal.base.id.clone(),
        business_object_version: reversal.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let document = new_registered_document(
        &reversal.base.id,
        DocumentType::ReceiptReversal,
        reversal.reversal_no.clone(),
    )?;
    let audit = actor.clone().resource_log(
        "receipt_reversal.create",
        "receipt_reversal",
        reversal.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_receipt_reversal_document(&db, &rbac, document, &bind_command, &actor, session)
                    .await?;
                db.receipt_reversals().create(&reversal, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 查询原回款往来主体作为责任组织，并带回可选客户。
///
/// # 错误
/// 原回款不存在或往来主体为空时返回错误。
async fn load_receipt_reversal_context(
    db: &Database,
    original_receipt_id: &CustomerReceiptId,
) -> Result<(String, Option<CustomerAccountId>)> {
    let receipt = db
        .customer_receipts()
        .find_by_id(original_receipt_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
    let organization_id = receipt_reversal_responsible_org_id(receipt.counterparty_party_id.as_ref())?;
    Ok((organization_id, receipt.customer_id))
}

/// 在创建冲正单的同一事务中重读并校验原客户回款事实。
///
/// 原回款必须仍为调用前读取的版本且已经过账，避免为非正式事实创建审批任务。
async fn validate_receipt_reversal_source(
    db: &Database,
    source_fact_id: &CustomerReceiptId,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let receipt = db
        .customer_receipts()
        .find_by_id(source_fact_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))?;
    super::ensure_posted_source(
        receipt.base.version,
        expected_version,
        receipt.status == CustomerReceiptStatus::Posted,
        "只有已过账的客户回款才能发起冲正",
    )
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_receipt_reversal_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<entities::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = receipt_reversal_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("回款冲正单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}

/// 在最终通过事务内执行过账副作用并写回冲正单。
///
/// # 错误
/// 非审批中、原回款不存在或仓储失败时返回错误。
pub(super) async fn apply_receipt_reversal_final_post(
    db: &Database,
    reversal_id: &str,
    actor_id: &str,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let mut reversal = db
        .receipt_reversals()
        .find_by_id(reversal_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
    if reversal.status == ReceiptReversalStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正单据不能再过账".to_string()));
    }
    ensure_receipt_reversal_final_approve_posting(&reversal)?;
    execute_receipt_reversal_domain_action(
        &mut reversal,
        crate::approval::policy::ApprovalDomainAction::ReceiptReversalPost,
    )?;
    apply_receipt_reversal_posting(db, &reversal, actor_id, session).await?;
    reversal.mark_posted()?;
    db.receipt_reversals().update(&mut reversal, session).await?;
    let audit = actor.clone().resource_log(
        "receipt_reversal.post",
        "receipt_reversal",
        reversal.base.id.clone(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    // 冲正后刷新销售单回款进度与关闭状态（已结清可能退回部分回款）
    let allocations = db
        .receipt_allocations()
        .find_allocations_by_receipts(&[reversal.original_customer_receipt_id.clone()], session)
        .await?;
    let sales_order_ids = sales_order_ids_for_receipt_allocations(db, &allocations, session).await?;
    for sales_order_id in sales_order_ids {
        crate::sales_order::update_sales_order_money_progress(
            db,
            session,
            &entities::ids::SalesOrderId::new(sales_order_id),
            actor_id.to_string(),
            None,
        )
        .await?;
    }
    Ok(())
}

/// 在调用方事务内写入冲正副作用。
///
/// # 错误
/// 原回款不存在、累计超额或仓储失败时返回错误。
async fn apply_receipt_reversal_posting(
    db: &Database,
    reversal: &ReceiptReversal,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
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
        .fold(
            Amount::from_str("0.00").expect("固定零金额必须可解析"),
            |sum, other| sum.checked_add(other.amount),
        );
    if reversed_before.checked_add(reversal.amount) > receipt.amount {
        return Err(Error::BusinessLogicError(
            "累计冲正金额不得超过原回款金额".to_string(),
        ));
    }
    persist_reversal_offsets_and_mark_receipt(db, reversal, receipt, actor_id, session).await
}

/// 写入反向核销分配、冲减进度并把原回款置为已冲正。
///
/// # 错误
/// 超额冲减或仓储失败时返回错误。
async fn persist_reversal_offsets_and_mark_receipt(
    db: &Database,
    reversal: &ReceiptReversal,
    receipt: CustomerReceipt,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let allocations = db
        .receipt_allocations()
        .find_allocations_by_receipts(&[receipt.base.id.clone().into()], session)
        .await?;
    let (reverse_rows, chunks) = ReceiptAllocation::plan_reverse(&allocations, reversal.amount)?;
    let seqs = ReceiptAllocation::next_allocation_seq_range(&allocations, reverse_rows.len())?;
    revert_receipt_settlements(db, &chunks, actor_id, session).await?;
    persist_reverse_allocations(db, reversal, &receipt, &reverse_rows, &seqs, session).await?;
    let mut receipt = receipt;
    receipt.transition(CustomerReceiptStatus::Reversed)?;
    db.customer_receipts().update(&mut receipt, session).await?;
    Ok(())
}

/// 按冲减块回冲应收子账已核销进度。
///
/// # 错误
/// 分录缺失或超额冲减时返回错误。
async fn revert_receipt_settlements(
    db: &Database,
    chunks: &[entities::receivable::ReceiptReverseChunk],
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let facts = load_receivable_offset_facts(
        db,
        chunks.iter().map(|chunk| chunk.increase_entry_id.clone()),
        session,
    )
    .await?;
    for chunk in chunks {
        let entry = facts
            .entries
            .get(chunk.increase_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        if !facts.accounts.contains_key(entry.receivable_account_id.as_ref()) {
            return Err(Error::NotFound("应收往来子账不存在".to_string()));
        }
        let reverted = db
            .receivable_accounts()
            .revert_settlement(&entry.receivable_account_id, &chunk.amount, actor_id, session)
            .await?;
        if !reverted {
            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
        }
    }
    Ok(())
}

/// 由核销分配批量读取分录与账户，收集去重销售单 ID。
///
/// # 参数
/// * `db` - 数据库
/// * `allocations` - 原回款核销分配
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回排序去重后的销售单 ID。
///
/// # 错误
/// 缺任一分录或账户时失败关闭。
///
/// # 约束
/// 固定两次批量读取；进度刷新仍由 Service 逐单编排。
async fn sales_order_ids_for_receipt_allocations(
    db: &Database,
    allocations: &[ReceiptAllocation],
    session: &mut mongodb::ClientSession,
) -> Result<Vec<String>> {
    let facts = load_receivable_offset_facts(
        db,
        allocations
            .iter()
            .map(|allocation| allocation.receivable_entry_id.clone()),
        session,
    )
    .await?;
    let mut sales_order_ids = Vec::with_capacity(allocations.len());
    for allocation in allocations {
        let entry = facts
            .entries
            .get(allocation.receivable_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        let account = facts
            .accounts
            .get(entry.receivable_account_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        sales_order_ids.push(account.sales_order_id.to_string());
    }
    sales_order_ids.sort();
    sales_order_ids.dedup();
    Ok(sales_order_ids)
}

/// 写入反向核销分配。
///
/// # 错误
/// 仓储失败时返回错误。
async fn persist_reverse_allocations(
    db: &Database,
    reversal: &ReceiptReversal,
    receipt: &CustomerReceipt,
    reverse_rows: &[entities::receivable::ReceiptReversePlanRow],
    seqs: &[u32],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for (reverse, seq) in reverse_rows.iter().zip(seqs.iter()) {
        let allocation = ReceiptAllocation::new(
            ReceiptAllocationId::new(next_id()),
            ReceiptAllocationData {
                customer_receipt_id: receipt.base.id.clone().into(),
                receivable_entry_id: reverse.entry_id.clone(),
                allocation_seq: *seq,
                allocation_action: ReceivableAllocationAction::Reverse,
                allocated_amount: reverse.amount,
                allocated_at: reversal.occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        db.receipt_allocations().create(&allocation, session).await?;
    }
    Ok(())
}

#[cfg(test)]
mod receipt_reversal_approval_tests {
    use super::{execute_receipt_reversal_domain_action, start_receipt_reversal_approval, ReturnsService};
    use crate::approval::policy::ApprovalDomainAction;
    use entities::common::time::Instant;
    use entities::ids::{CustomerReceiptId, ReceiptReversalId};
    use entities::money::Amount;
    use entities::returns::{ReceiptReversal, ReceiptReversalData, ReceiptReversalStatus};
    use std::str::FromStr;

    fn draft_reversal() -> ReceiptReversal {
        ReceiptReversal::new(
            ReceiptReversalId::new("rr-1"),
            ReceiptReversalData {
                reversal_no: "RR-1".into(),
                original_customer_receipt_id: CustomerReceiptId::new("cr-1"),
                reason_code: None,
                reason_text: "错记回款冲正".into(),
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
        let source = include_str!("receipt_reversal.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::ReceiptReversal"));
        assert!(source.contains("persist_created_receipt_reversal"));
    }

    /// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
    #[test]
    fn create_path_calls_local_object_readable() {
        use super::super::adapter::receipt_reversal_object_readable;

        let production = include_str!("receipt_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("receipt_reversal_object_readable"));
        assert!(!production.contains("adapter_object_read_decision"));
        assert!(receipt_reversal_object_readable("org-1", "u1").unwrap());
        assert!(receipt_reversal_object_readable(" ", "u1").is_err());
        assert!(receipt_reversal_object_readable("org-1", "").is_err());
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("receipt_reversal.rs");
        assert!(source.contains("pub async fn submit_receipt_reversal"));
        assert!(source.contains("receipt_reversal_start_command"));
        assert!(source.contains("reversal.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_receipt_reversal，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_receipt_reversal() {
        let source = include_str!("receipt_reversal.rs");
        assert!(source.contains("pub async fn post_receipt_reversal"));
        assert!(source.contains("reversal.mark_posted"));
        assert!(source.contains("ReceiptReversalPost"));
        assert!(ReturnsService::reject_receipt_reversal_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("receipt_reversal.rs");
        assert!(source.contains("pub async fn cancel_receipt_reversal_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_receipt_reversal_cancel"));
        let _ = ReturnsService::reject_receipt_reversal_client_post();
        let mut reversal = draft_reversal();
        start_receipt_reversal_approval(&mut reversal).unwrap();
        execute_receipt_reversal_domain_action(
            &mut reversal,
            ApprovalDomainAction::ReceiptReversalCancelApproval,
        )
        .unwrap();
        assert_eq!(reversal.status, ReceiptReversalStatus::Draft);
        assert_eq!(reversal.approval_subject_version, 1);
    }

    /// 生产代码不得保留草稿直接过账或待复核旁路。
    #[test]
    fn production_closes_draft_post_and_pending_review() {
        let production = include_str!("receipt_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("ReceiptReversalStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }

    /// 提交/撤回必须使用实体 matches_version，并删除旧 helper。
    #[test]
    fn version_lock_uses_entity_matches_version() {
        let production = include_str!("receipt_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("reversal.matches_version(req.expected_version)"));
        assert!(production.contains("conflict_if_stale_version"));
        assert!(!production.contains("fn ensure_expected_version"));
    }

    /// 过账与冲减必须批量读取分录与账户。
    #[test]
    fn reversal_paths_batch_entry_and_account_reads() {
        let production = include_str!("receipt_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("load_receivable_offset_facts"));
        assert!(production.contains("sales_order_ids_for_receipt_allocations"));
        assert!(production.contains("revert_settlement"));
        assert!(!production.contains("find_by_id(&allocation.receivable_entry_id"));
        assert!(!production.contains("find_by_id(&chunk.increase_entry_id"));
        assert!(!production.contains("find_by_id(&entry.receivable_account_id"));
    }
}
