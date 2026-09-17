//! 客户回款过账与创建持久化：审批运行时持有的事务内过账/撤回与创建绑定写入。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditActorLogs, AuditExt, CommandReceiptServiceExt as _};
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, SalesOrderId};
use erp_finance::entity::receivable::{CustomerReceipt, CustomerReceiptData, CustomerReceiptStatus};
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::customer_receipt_commit::PreparedCustomerReceiptCommit;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_identity::SharedRbacService;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::prepare_start;
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::adapter::{
    build_customer_receipt_snapshot, customer_receipt_object_readable, customer_receipt_responsible_org_id,
    customer_receipt_subject_ref, ensure_final_approve_posting, execute_customer_receipt_domain_action,
    require_frozen_binding, start_customer_receipt_approval,
};
use super::start_approval::{
    CustomerReceiptStartPersistInput, DocumentStartInput, build_document_start_input,
    load_bound_definition_graph, load_bound_definition_graph_with_executor, load_start_receipt,
    load_start_receipt_with_executor, persist_customer_receipt_start_in_transaction,
};
use crate::{Error, Result};

/// 提交事务的已捆绑输入：待提交候选、冻结分配与幂等身份。
pub(super) struct CommitTransactionRequest {
    /// 新建候选或已有草稿身份。
    pub(super) pending: PendingCustomerReceiptCommit,
    /// 冻结核销分配。
    pub(super) allocations: Vec<erp_finance::entity::receivable::PendingReceiptAllocation>,
    /// 幂等键。
    pub(super) idempotency_key: String,
    /// 适配器登记的责任角色。
    pub(super) owner_role: &'static str,
    /// 提交人。
    pub(super) actor: AuditActor,
    /// 提交幂等收据。
    pub(super) command_receipt: CommandReceipt,
}

/// 在唯一事务内落盘回款（或读取草稿）并执行启动装配与命令审计。
///
/// # 参数
/// * `db` - 数据库实例
/// * `rbac` - 授权源
/// * `object_read` - 对象读取端口
/// * `request` - 待提交候选、分配与幂等身份
///
/// # 返回
/// 返回进入审批后的回款实体。
///
/// # 错误
/// 新建冲突、草稿缺失、装配或写入失败时返回错误。
pub(super) async fn run_commit_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    request: CommitTransactionRequest,
) -> Result<CustomerReceipt> {
    let CommitTransactionRequest {
        pending,
        allocations,
        idempotency_key,
        owner_role,
        actor,
        command_receipt,
    } = request;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                let (receipt, binding) =
                    load_commit_receipt(&db, &rbac, object_read.as_ref(), pending, &actor, session).await?;
                persist_loaded_commit_start(
                    &db,
                    LoadedCommitStart {
                        receipt,
                        binding,
                        allocations,
                        idempotency_key,
                        owner_role,
                        actor: actor.clone(),
                    },
                    &command_receipt,
                    session,
                )
                .await
            })
        })
        .await
}

/// 在审批运行时持有的事务内过账客户回款并写入核销事实。
///
/// # 参数
/// * `db` - 数据库实例
/// * `receipt_id` - 客户回款单 ID
/// * `actor` - 已认证操作人
/// * `session` - 审批运行时持有的唯一事务会话
///
/// # 返回
/// 回款、核销、应收进度、销售回款进度和成功审计全部写入时返回 `Ok(())`。
///
/// # 错误
/// 回款/分录不存在、主体或额度不变量失败、任一写入失败时返回错误。
pub async fn post_customer_receipt_in_transaction(
    db: &Database,
    receipt_id: &str,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let actor_id = actor.id().to_string();
    let mut receipt = db
        .customer_receipts()
        .find_by_id(receipt_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
    if receipt.status == CustomerReceiptStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正回款不能再核销".to_string()));
    }
    ensure_final_approve_posting(&receipt)?;
    execute_customer_receipt_domain_action(
        &mut receipt,
        erp_workflow::service::approval::policy::ApprovalDomainAction::CustomerReceiptPost,
    )?;
    let mut sales_order_ids =
        erp_finance::service::receivable::customer_receipt_posting::settle_customer_receipt(
            db,
            &mut receipt,
            &actor_id,
            session,
        )
        .await?;
    let audit = actor.clone().resource_log(
        &format!("customer_receipt.post:{receipt_id}"),
        "customer_receipt",
        receipt.base.id.clone(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    sales_order_ids.sort();
    sales_order_ids.dedup();
    for sales_order_id in sales_order_ids {
        crate::order_to_cash::progress::update_sales_order_money_progress(
            db,
            session,
            &SalesOrderId::new(sales_order_id),
            actor_id.clone(),
            None,
        )
        .await?;
    }
    Ok(())
}

/// 在审批运行时持有的事务内撤回客户回款审批。
///
/// # 参数
/// * `db` - 数据库实例
/// * `receipt_id` - 客户回款单 ID
/// * `action` - 已校验的撤回领域动作
/// * `actor` - 已认证操作人
/// * `executor` - 调用方执行器
///
/// # 错误
/// 回款单不存在、动作不匹配、状态迁移或 CAS 写入失败时返回错误。
pub async fn cancel_customer_receipt_approval_in_transaction(
    db: &Database,
    receipt_id: &str,
    action: erp_workflow::service::approval::policy::ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut receipt = db
        .customer_receipts()
        .find_by_id(receipt_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
    execute_customer_receipt_domain_action(&mut receipt, action)?;
    db.customer_receipts().update(&mut receipt, executor).await?;
    let audit = actor.clone().resource_log(
        "customer_receipt.cancel_approval",
        "customer_receipt",
        receipt_id.to_string(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

/// 在创建事务内写入回款单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 参数
/// * `db` - 数据库实例
/// * `rbac` - 授权源
/// * `object_read` - 对象读取端口
/// * `receipt` - 新建回款候选
/// * `actor` - 提交人
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
pub(super) async fn persist_created_customer_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    receipt: CustomerReceipt,
    actor: AuditActor,
) -> Result<()> {
    let organization_id = customer_receipt_responsible_org_id(&receipt)?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::CustomerReceipt,
        business_object_id: receipt.base.id.clone(),
        business_object_version: receipt.base.version,
        context: BindingRevalidationContext::new(organization_id, actor.id().to_string()),
    };
    let document =
        new_registered_document(&receipt.base.id, DocumentType::CustomerReceipt, receipt.receipt_no.clone())
            .map_err(crate::Error::from)?;
    let audit =
        actor.clone().resource_log("customer_receipt.create", "customer_receipt", receipt.base.id.clone())?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_customer_receipt_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    document,
                    &bind_command,
                    &actor,
                    session,
                )
                .await?;
                db.customer_receipts().create(&receipt, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 参数
/// * `db` - 数据库实例
/// * `rbac` - 授权源
/// * `object_read` - 对象读取端口
/// * `document` - 待登记单据
/// * `bind_command` - 绑定命令
/// * `actor` - 提交人
/// * `session` - 调用方事务会话
///
/// # 返回
/// 返回已冻结的审批定义绑定。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
pub(super) async fn persist_bound_customer_receipt_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<ApprovalDefinitionBinding> {
    let _ = customer_receipt_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        session,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("客户回款单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}

/// 待提交的回款候选：新建候选或已有草稿身份。
///
/// 新建与已有分支经此收敛为统一四元组；事务闭包只做顺序写入与审计。
pub(super) struct PendingCustomerReceiptCommit {
    /// 新建回款候选；`None` 表示提交已有草稿。
    new_receipt: Option<CustomerReceipt>,
    /// 已有草稿主键；新建时为 `None`。
    requested_id: Option<String>,
    /// 已有草稿期望版本；新建时为 `None`。
    expected_version: Option<u64>,
    /// 冻结核销分配。
    pub(super) allocations: Vec<erp_finance::entity::receivable::PendingReceiptAllocation>,
}

/// 由已校验命令构造新建候选或已有草稿身份。
///
/// # 参数
/// * `prepared` - 已校验的提交命令
/// * `actor_id` - 提交人
///
/// # 返回
/// 返回新建候选或已有草稿身份与冻结分配。
///
/// # 错误
/// 新建候选构造或分配校验失败时返回错误。
pub(super) fn prepare_customer_receipt_commit_candidate(
    prepared: PreparedCustomerReceiptCommit,
    actor_id: &str,
) -> Result<PendingCustomerReceiptCommit> {
    match prepared {
        PreparedCustomerReceiptCommit::New { receipt, allocations } => {
            receipt.validate()?;
            let candidate = CustomerReceipt::new(
                CustomerReceiptId::new(next_id()),
                CustomerReceiptData {
                    receipt_no: receipt.receipt_no,
                    counterparty_party_id: receipt.counterparty_party_id,
                    customer_id: receipt.customer_id,
                    received_at: receipt.received_at,
                    amount: receipt.amount,
                    bank_reference: receipt.bank_reference,
                },
                actor_id,
            )?;
            Ok(PendingCustomerReceiptCommit {
                new_receipt: Some(candidate),
                requested_id: None,
                expected_version: None,
                allocations,
            })
        },
        PreparedCustomerReceiptCommit::Existing { receipt_id, expected_version, allocations } => {
            Ok(PendingCustomerReceiptCommit {
                new_receipt: None,
                requested_id: Some(receipt_id),
                expected_version: Some(expected_version),
                allocations,
            })
        },
    }
}

/// 在调用方事务内落盘新建回款的注册行、绑定、实体与创建审计。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 授权源
/// * `object_read` - 对象读取端口
/// * `candidate` - 新建回款候选
/// * `actor` - 提交人
/// * `session` - 调用方事务会话
///
/// # 返回
/// 返回已落盘回款与冻结绑定。
///
/// # 错误
/// 回款单号重复、绑定失败或写入失败时返回错误。
async fn create_new_commit_records(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    candidate: CustomerReceipt,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<(CustomerReceipt, ApprovalDefinitionBinding)> {
    if db.customer_receipts().find_by_receipt_no(&candidate.receipt_no, session).await?.is_some() {
        return Err(Error::ConflictError("回款单号已存在，请刷新后重试".to_string()));
    }
    let organization_id = customer_receipt_responsible_org_id(&candidate)?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::CustomerReceipt,
        business_object_id: candidate.base.id.clone(),
        business_object_version: candidate.base.version,
        context: BindingRevalidationContext::new(organization_id, actor.id().to_string()),
    };
    let document = new_registered_document(
        &candidate.base.id,
        DocumentType::CustomerReceipt,
        candidate.receipt_no.clone(),
    )
    .map_err(crate::Error::from)?;
    let binding = persist_bound_customer_receipt_document(
        db,
        rbac,
        object_read,
        document,
        &bind_command,
        actor,
        session,
    )
    .await?;
    db.customer_receipts().create(&candidate, session).await?;
    let audit = actor.clone().resource_log(
        "customer_receipt.create",
        "customer_receipt",
        candidate.base.id.clone(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok((candidate, binding))
}

/// 在调用方事务内读取已有草稿并校验期望版本与审批绑定。
///
/// # 参数
/// * `db` - 数据库
/// * `receipt_id` - 已有草稿主键
/// * `expected_version` - 调用方期望版本
/// * `session` - 调用方事务会话
///
/// # 返回
/// 返回已有回款与冻结绑定。
///
/// # 错误
/// 草稿不存在、版本不一致或缺少审批绑定时返回错误。
async fn load_existing_commit_records(
    db: &Database,
    receipt_id: &str,
    expected_version: u64,
    session: &mut mongodb::ClientSession,
) -> Result<(CustomerReceipt, ApprovalDefinitionBinding)> {
    let receipt = db
        .customer_receipts()
        .find_by_id(receipt_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
    ensure_expected_version(receipt.base.version, expected_version)?;
    let binding = find_approval_binding(db, receipt_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("客户回款单缺少审批绑定".to_string()))?;
    Ok((receipt, binding))
}

/// 提交启动事务的已加载输入：回款、冻结绑定与冻结分配。
pub(super) struct LoadedCommitStart {
    /// 已落盘或已校验的回款实体。
    pub(super) receipt: CustomerReceipt,
    /// 创建时冻结的定义绑定。
    pub(super) binding: ApprovalDefinitionBinding,
    /// 冻结核销分配。
    pub(super) allocations: Vec<erp_finance::entity::receivable::PendingReceiptAllocation>,
    /// 幂等键。
    pub(super) idempotency_key: String,
    /// 适配器登记的责任角色。
    pub(super) owner_role: &'static str,
    /// 提交人。
    pub(super) actor: AuditActor,
}

/// 在调用方事务内落盘新建回款或读取已有草稿，收敛为统一二元组。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 授权源
/// * `object_read` - 对象读取端口
/// * `pending` - 待提交的新建候选或已有草稿身份
/// * `actor` - 提交人
/// * `session` - 调用方事务会话
///
/// # 返回
/// 返回回款实体与冻结绑定。
///
/// # 错误
/// 新建冲突、草稿缺失或版本不一致时返回错误。
pub(super) async fn load_commit_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    pending: PendingCustomerReceiptCommit,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<(CustomerReceipt, ApprovalDefinitionBinding)> {
    let PendingCustomerReceiptCommit { new_receipt, requested_id, expected_version, .. } = pending;
    match new_receipt {
        Some(candidate) => create_new_commit_records(db, rbac, object_read, candidate, actor, session).await,
        None => {
            let receipt_id = requested_id
                .as_deref()
                .ok_or_else(|| Error::ValidationError("已有回款缺少主键".to_string()))?;
            let version =
                expected_version.ok_or_else(|| Error::ValidationError("已有回款缺少期望版本".to_string()))?;
            load_existing_commit_records(db, receipt_id, version, session).await
        },
    }
}

/// 在调用方事务内执行启动装配、运行事实持久化与命令审计。
///
/// # 参数
/// * `db` - 数据库
/// * `request` - 已加载的回款、绑定、分配与身份
/// * `command_receipt` - 提交幂等收据
/// * `session` - 调用方事务会话
///
/// # 返回
/// 返回进入审批后的回款实体。
///
/// # 错误
/// 快照、定义图、启动装配或写入失败时返回错误。
pub(super) async fn persist_loaded_commit_start(
    db: &Database,
    mut request: LoadedCommitStart,
    command_receipt: &CommandReceipt,
    session: &mut mongodb::ClientSession,
) -> Result<CustomerReceipt> {
    let binding = require_frozen_binding(Some(&request.binding))?.clone();
    start_customer_receipt_approval(&mut request.receipt, request.allocations)?;
    let id = request.receipt.base.id.clone();
    let subject = customer_receipt_subject_ref(&id)?;
    let now = Instant::now();
    let snapshot = build_customer_receipt_snapshot(&request.receipt, request.actor.id(), now)?;
    let organization_id = customer_receipt_responsible_org_id(&request.receipt)?;
    let _ = customer_receipt_object_readable(&organization_id, request.actor.id())?;
    let graph = load_bound_definition_graph_with_executor(db, &binding, session).await?;
    let existing_receipt = load_start_receipt_with_executor(
        db,
        &subject,
        request.receipt.approval_subject_version,
        &request.idempotency_key,
        session,
    )
    .await?;
    let start_input = build_document_start_input(DocumentStartInput {
        document_type: DocumentType::CustomerReceipt,
        graph,
        binding: &binding,
        subject,
        subject_version: request.receipt.approval_subject_version,
        actor_id: request.actor.id(),
        organization_id: &organization_id,
        idempotency_key: &request.idempotency_key,
        receipt: existing_receipt,
        now,
    })?;
    let prepared = prepare_start(start_input)?;
    let committed = persist_customer_receipt_start_in_transaction(
        db,
        CustomerReceiptStartPersistInput {
            receipt: request.receipt,
            actor: request.actor.clone(),
            id,
            snapshot_payload: snapshot,
            prepared,
            owner_role: request.owner_role,
            organization_id,
            now,
        },
        session,
    )
    .await?;
    let command_audit = command_receipt.audit(request.actor.clone(), committed.base.id.clone())?;
    db.audit_logs().create(&command_audit, session).await?;
    Ok(committed)
}

/// 由绑定与快照装配分发路径的统一启动输入。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `organization_id` - 责任组织
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
/// * `now` - 调用方时间
///
/// # 返回
/// 返回可交给统一 `prepare_start` 的启动编排输入装配后的待执行。
///
/// # 错误
/// 定义图、收据读取或输入装配失败时返回错误。
pub(super) async fn prepare_dispatch_start(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    subject: &bpm::SubjectRef,
    subject_version: u32,
    organization_id: &str,
    actor_id: &str,
    idempotency_key: &str,
    now: Instant,
) -> Result<erp_workflow::service::approval::execution::PreparedExecution> {
    let graph = load_bound_definition_graph(db, binding).await?;
    let existing_receipt = load_start_receipt(db, subject, subject_version, idempotency_key).await?;
    let start_input = build_document_start_input(DocumentStartInput {
        document_type: DocumentType::CustomerReceipt,
        graph,
        binding,
        subject: subject.clone(),
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt: existing_receipt,
        now,
    })?;
    Ok(prepare_start(start_input)?)
}
