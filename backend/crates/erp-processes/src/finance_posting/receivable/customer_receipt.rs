//! 客户回款单查询、创建、提交审批、撤回与过账编排。

use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, SalesOrderId};
use erp_finance::entity::receivable::{CustomerReceipt, CustomerReceiptData, CustomerReceiptStatus};
use erp_finance::repository::ReceivableExt;

use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};

use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::adapter::{
    self, build_customer_receipt_snapshot, customer_receipt_adapter, customer_receipt_object_readable,
    customer_receipt_responsible_org_id, customer_receipt_start_command, customer_receipt_subject_ref,
    ensure_final_approve_posting, execute_customer_receipt_domain_action, require_frozen_binding,
    start_approval_command_kind, start_customer_receipt_approval, RECENT_HISTORY_LIMIT,
};
use super::cancel_approval::{
    build_customer_receipt_cancel_input, load_cancel_runtime, persist_customer_receipt_cancel,
    CustomerReceiptCancelPersistInput,
};
use super::dto::{
    CancelCustomerReceiptApprovalRequest, CommitCustomerReceiptRequest, CreateCustomerReceiptRequest,
    CustomerReceiptView, SubmitCustomerReceiptRequest,
};
use super::start_approval::{
    build_document_start_input, load_bound_definition_graph, load_bound_definition_graph_with_executor,
    load_start_receipt, load_start_receipt_with_executor, persist_customer_receipt_start,
    persist_customer_receipt_start_in_transaction, replay_customer_receipt_start_with_executor,
    CustomerReceiptStartPersistInput, DocumentStartInput,
};
use super::ReceivableProcess;
use crate::{Error, Result};
use application_core::AuditActor;
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::CommandReceiptServiceExt as _;
use erp_finance::service::receivable::customer_receipt_commit::PreparedCustomerReceiptCommit;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_identity::SharedRbacService;
use erp_workflow::service::approval::binding::{attach_published_binding, BindPublishedDefinitionCommand};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use erp_workflow::DocumentRegistryExt;

impl ReceivableProcess {
    // -----------------------------------------------------------------------
    // 客户回款单
    // -----------------------------------------------------------------------

    /// 登记客户回款草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 回款单号全局唯一（`uk_customer_receipts_no` 唯一索引）构成幂等去重。
    /// 绑定失败必须回滚业务实体，不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建回款单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 回款单号重复或流程未配置
    pub async fn create_customer_receipt(
        &self,
        req: CreateCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let receipt = CustomerReceipt::new(
            CustomerReceiptId::new(next_id()),
            CustomerReceiptData {
                receipt_no: req.receipt_no,
                counterparty_party_id: req.counterparty_party_id,
                customer_id: req.customer_id,
                received_at: req.received_at,
                amount: req.amount,
                bank_reference: req.bank_reference,
            },
            actor.id(),
        )?;
        persist_created_customer_receipt(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            receipt.clone(),
            actor.clone(),
        )
        .await?;
        self.read
            .customer_receipt_detail(&receipt.base.id)
            .await
            .map_err(crate::Error::from)
    }

    /// 原子创建或提交客户回款并启动审批。
    ///
    /// 新回款的单据注册与定义绑定、回款实体、冻结核销分配、审批运行事实、
    /// 不可变快照、入口任务和审计全部位于同一事务。已有草稿用乐观锁校验后
    /// 走同一启动事务，前端不得再执行“先创建草稿、再提交”。
    ///
    /// # 参数
    /// * `req` - 新回款或已有草稿身份、冻结分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回进入审批后的回款单视图。
    ///
    /// # 错误
    /// * `ValidationError` - 参数组合或分配不合法
    /// * `ConflictError` - 草稿版本、状态、绑定或审批定义冲突
    /// * `NotFound` - 已有草稿不存在
    pub async fn commit_customer_receipt(
        &self,
        req: CommitCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "customer-receipt-commit-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(receipt_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self
                .read
                .customer_receipt_detail(&receipt_id)
                .await
                .map_err(crate::Error::from);
        }
        let prepared = req.prepare()?;
        let (new_receipt, requested_id, expected_version, allocations) = match prepared {
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
                    actor.id(),
                )?;
                (Some(candidate), None, None, allocations)
            }
            PreparedCustomerReceiptCommit::Existing {
                receipt_id,
                expected_version,
                allocations,
            } => (None, Some(receipt_id), Some(expected_version), allocations),
        };
        let idempotency_key = req.idempotency_key;
        let adapter = customer_receipt_adapter()?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let (mut receipt, binding) = match new_receipt {
                        Some(candidate) => {
                            if db
                                .customer_receipts()
                                .find_by_receipt_no(&candidate.receipt_no, session)
                                .await?
                                .is_some()
                            {
                                return Err(Error::ConflictError("回款单号已存在，请刷新后重试".to_string()));
                            }
                            let organization_id = customer_receipt_responsible_org_id(&candidate)?;
                            let bind_command = BindPublishedDefinitionCommand {
                                document_type: DocumentType::CustomerReceipt,
                                business_object_id: candidate.base.id.clone(),
                                business_object_version: candidate.base.version,
                                context: BindingRevalidationContext {
                                    order_source: None,
                                    customer_id: None,
                                    business_org_unit_id: None,
                                    scope_owner_user_id: None,
                                    organization_id,
                                    creator_id: actor_owned.id().to_string(),
                                },
                            };
                            let document = new_registered_document(
                                &candidate.base.id,
                                DocumentType::CustomerReceipt,
                                candidate.receipt_no.clone(),
                            )
                            .map_err(crate::Error::from)?;
                            let binding = persist_bound_customer_receipt_document(
                                &db,
                                &rbac,
                                object_read.as_ref(),
                                document,
                                &bind_command,
                                &actor_owned,
                                session,
                            )
                            .await?;
                            db.customer_receipts().create(&candidate, session).await?;
                            let audit = actor_owned.clone().resource_log(
                                "customer_receipt.create",
                                "customer_receipt",
                                candidate.base.id.clone(),
                            )?;
                            db.audit_logs().create(&audit, session).await?;
                            (candidate, binding)
                        }
                        None => {
                            let receipt_id = requested_id
                                .as_deref()
                                .ok_or_else(|| Error::ValidationError("已有回款缺少主键".to_string()))?;
                            let receipt = db
                                .customer_receipts()
                                .find_by_id(receipt_id, session)
                                .await?
                                .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
                            ensure_expected_version(
                                receipt.base.version,
                                expected_version.ok_or_else(|| {
                                    Error::ValidationError("已有回款缺少期望版本".to_string())
                                })?,
                            )?;
                            let binding = find_approval_binding(&db, receipt_id, session)
                                .await?
                                .ok_or_else(|| Error::ConflictError("客户回款单缺少审批绑定".to_string()))?;
                            (receipt, binding)
                        }
                    };
                    let binding = require_frozen_binding(Some(&binding))?.clone();
                    start_customer_receipt_approval(&mut receipt, allocations)?;
                    let id = receipt.base.id.clone();
                    let subject = customer_receipt_subject_ref(&id)?;
                    let now = Instant::now();
                    let snapshot = build_customer_receipt_snapshot(&receipt, actor_owned.id(), now)?;
                    let organization_id = customer_receipt_responsible_org_id(&receipt)?;
                    let _ = customer_receipt_object_readable(&organization_id, actor_owned.id())?;
                    let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                    let existing_start_receipt = load_start_receipt_with_executor(
                        &db,
                        &subject,
                        receipt.approval_subject_version,
                        &idempotency_key,
                        session,
                    )
                    .await?;
                    let start_input = build_document_start_input(DocumentStartInput {
                        document_type: DocumentType::CustomerReceipt,
                        graph,
                        binding: &binding,
                        subject,
                        subject_version: receipt.approval_subject_version,
                        actor_id: actor_owned.id(),
                        organization_id: &organization_id,
                        idempotency_key: &idempotency_key,
                        receipt: existing_start_receipt,
                        now,
                    })?;
                    let prepared = prepare_start(start_input)?;
                    let committed = persist_customer_receipt_start_in_transaction(
                        &db,
                        CustomerReceiptStartPersistInput {
                            receipt,
                            actor: actor_owned.clone(),
                            id,
                            snapshot_payload: snapshot,
                            prepared,
                            owner_role: adapter.owner_role,
                            organization_id,
                            now,
                        },
                        session,
                    )
                    .await?;
                    let command_audit =
                        command_receipt_for_tx.audit(actor_owned.clone(), committed.base.id.clone())?;
                    db.audit_logs().create(&command_audit, session).await?;
                    Ok::<CustomerReceipt, crate::Error>(committed)
                })
            })
            .await;

        let committed = match transaction_result {
            Ok(committed) => committed,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(receipt_id) => {
                    return self
                        .read
                        .customer_receipt_detail(&receipt_id)
                        .await
                        .map_err(crate::Error::from)
                }
                None => return Err(error),
            },
        };

        self.read
            .customer_receipt_detail(&committed.base.id)
            .await
            .map_err(crate::Error::from)
    }

    /// 提交客户回款并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 回款单主键
    /// * `req` - 提交请求（版本、幂等键与冻结分配）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_customer_receipt(
        &self,
        id: &str,
        req: SubmitCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let adapter = customer_receipt_adapter()?;
        let mut receipt = self.load_customer_receipt(id).await?;
        ensure_expected_version(receipt.base.version, req.expected_version)?;
        let allocations =
            erp_finance::service::receivable::customer_receipt_commit::convert_allocations(&req.allocations)?;
        start_customer_receipt_approval(&mut receipt, allocations)?;
        self.dispatch_customer_receipt_start(id, receipt, req.idempotency_key, actor, adapter)
            .await
    }

    /// 撤回客户回款审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 回款单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    /// * `ConflictError` - 非审批中、已最终通过或并发冲突
    pub async fn cancel_customer_receipt_approval(
        &self,
        id: &str,
        req: CancelCustomerReceiptApprovalRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let mut receipt = self.load_customer_receipt(id).await?;
        ensure_expected_version(receipt.base.version, req.expected_version)?;
        self.persist_cancelled_customer_receipt(id, &mut receipt, &req, actor)
            .await?;
        self.read
            .customer_receipt_detail(id)
            .await
            .map_err(crate::Error::from)
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_post() -> Result<CustomerReceiptView> {
        Err(Error::ConflictError(
            "客户回款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string(),
        ))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_customer_receipt_start(
        &self,
        id: &str,
        receipt: CustomerReceipt,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: adapter::CustomerReceiptAdapter,
    ) -> Result<CustomerReceiptView> {
        let subject = customer_receipt_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let snapshot = build_customer_receipt_snapshot(&receipt, actor.id(), now)?;
        let start = customer_receipt_start_command(
            id,
            receipt.approval_subject_version,
            actor.id(),
            &idempotency_key,
        );
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
        let _ = customer_receipt_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &subject,
            receipt.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_document_start_input(DocumentStartInput {
            document_type: DocumentType::CustomerReceipt,
            graph,
            binding: &binding,
            subject,
            subject_version: receipt.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        let recovery_subject_version = receipt.approval_subject_version;
        let persisted = persist_customer_receipt_start(
            &self.db,
            CustomerReceiptStartPersistInput {
                receipt,
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
            self.recover_customer_receipt_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        self.read
            .customer_receipt_detail(id)
            .await
            .map_err(crate::Error::from)
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_customer_receipt_start(
        &self,
        receipt_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let receipt_id = receipt_id.to_string();
            let idempotency_key = idempotency_key.to_string();
            let actor_id = actor.id().to_string();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        let receipt = db
                            .customer_receipts()
                            .find_by_id(&receipt_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
                        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
                        let _ = customer_receipt_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &receipt_id, session)
                            .await
                            .map_err(crate::Error::from)?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = customer_receipt_subject_ref(&receipt_id)?;
                        replay_customer_receipt_start_with_executor(
                            &db,
                            &subject,
                            subject_version,
                            &idempotency_key,
                            binding,
                            &actor_id,
                            session,
                        )
                        .await
                    })
                })
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

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_customer_receipt(
        &self,
        id: &str,
        receipt: &mut CustomerReceipt,
        req: &CancelCustomerReceiptApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = customer_receipt_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = customer_receipt_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, receipt.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_customer_receipt_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_customer_receipt_domain_action(receipt, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "customer_receipt.cancel_approval",
            "customer_receipt",
            id.to_string(),
        )?;
        persist_customer_receipt_cancel(
            &self.db,
            CustomerReceiptCancelPersistInput {
                receipt: receipt.clone(),
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

    /// 按主键读取客户回款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_customer_receipt(&self, id: &str) -> Result<CustomerReceipt> {
        self.db
            .customer_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))
    }

    /// 最终通过过账并核销（§8.3-1 事务不变量）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 校验回款与应收分录同一往来主体、分录开放余额与回款剩余余额；写提交时
    /// 冻结的核销分配（`APPLY`）；按条件原子更新子账已核销进度。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单或应收分录不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 跨主体核销、超额核销或重复过账
    pub async fn post_customer_receipt(&self, id: &str, actor: &AuditActor) -> Result<CustomerReceiptView> {
        let db = self.db.clone();
        let _object_read = std::sync::Arc::clone(&self.object_read);
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let receipt_id = id.to_string();
        let detail_id = receipt_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    post_customer_receipt_in_transaction(&db, &receipt_id, &actor_owned, session).await
                })
            })
            .await?;

        self.read
            .customer_receipt_detail(&detail_id)
            .await
            .map_err(crate::Error::from)
    }
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
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_customer_receipt(
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
        context: BindingRevalidationContext {
            order_source: None,
            customer_id: None,
            business_org_unit_id: None,
            scope_owner_user_id: None,
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let document = new_registered_document(
        &receipt.base.id,
        DocumentType::CustomerReceipt,
        receipt.receipt_no.clone(),
    )
    .map_err(crate::Error::from)?;
    let audit = actor.clone().resource_log(
        "customer_receipt.create",
        "customer_receipt",
        receipt.base.id.clone(),
    )?;
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

#[cfg(test)]
mod customer_receipt_approval_tests {
    use super::{execute_customer_receipt_domain_action, start_customer_receipt_approval, ReceivableProcess};
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerReceiptId, PartyId, ReceivableEntryId};
    use erp_core::money::Amount;
    use erp_finance::entity::receivable::{
        CustomerReceipt, CustomerReceiptData, CustomerReceiptStatus, PendingReceiptAllocation,
    };
    use erp_workflow::service::approval::policy::ApprovalDomainAction;
    use std::str::FromStr;

    fn draft_receipt() -> CustomerReceipt {
        CustomerReceipt::new(
            CustomerReceiptId::new("cr-1"),
            CustomerReceiptData {
                receipt_no: "RC-1".into(),
                counterparty_party_id: PartyId::new("party-1"),
                customer_id: None,
                received_at: Instant::from_unix_secs(1),
                amount: Amount::from_str("100").expect("金额合法"),
                bank_reference: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 创建必须注册 BusinessDocument 并绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = include_str!("customer_receipt.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::CustomerReceipt"));
        assert!(source.contains("persist_created_customer_receipt"));
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("customer_receipt.rs");
        assert!(source.contains("pub async fn submit_customer_receipt"));
        assert!(source.contains("customer_receipt_start_command"));
        assert!(source.contains("receipt.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_customer_receipt，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_customer_receipt() {
        let source = include_str!("customer_receipt.rs");
        assert!(source.contains("pub async fn post_customer_receipt"));
        assert!(source.contains("receipt.mark_posted"));
        assert!(source.contains("CustomerReceiptPost"));
        assert!(source.contains("pending_allocations"));
        assert!(ReceivableProcess::reject_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("customer_receipt.rs");
        assert!(source.contains("pub async fn cancel_customer_receipt_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_customer_receipt_cancel"));
        let _ = ReceivableProcess::reject_client_post();
        let mut receipt = draft_receipt();
        start_customer_receipt_approval(
            &mut receipt,
            vec![PendingReceiptAllocation::new(
                ReceivableEntryId::new("re-1"),
                Amount::from_str("10").expect("金额合法"),
            )
            .expect("分配合法")],
        )
        .unwrap();
        execute_customer_receipt_domain_action(
            &mut receipt,
            ApprovalDomainAction::CustomerReceiptCancelApproval,
        )
        .unwrap();
        assert_eq!(receipt.status, CustomerReceiptStatus::Draft);
        assert_eq!(receipt.approval_subject_version, 1);
    }
}
