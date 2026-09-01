use super::adapter::{
    build_customer_refund_snapshot, customer_refund_adapter, customer_refund_object_readable,
    customer_refund_responsible_org_id, customer_refund_start_command, customer_refund_subject_ref,
    document_approval_view, ensure_final_approve_posting, execute_customer_refund_domain_action,
    require_frozen_binding, start_approval_command_kind, start_customer_refund_approval,
    RECENT_HISTORY_LIMIT,
};
use super::cancel_approval::{
    build_customer_refund_cancel_input, load_cancel_runtime, persist_customer_refund_cancel,
    CustomerRefundCancelPersistInput,
};
use super::dto::{
    CancelCustomerRefundApprovalRequest, CommitCustomerRefundRequest, CreateCustomerRefundRequest,
    CustomerRefundListParams, CustomerRefundView, PageView, SortDir, SubmitCustomerRefundRequest,
};
use super::start_approval::{
    build_customer_refund_start_input, ensure_return_start_actor_active,
    ensure_return_start_replay_authorized, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, load_start_receipt, persist_customer_refund_start,
    persist_runtime_writes, replay_return_start_with_executor, replay_subject_versions,
    CustomerRefundStartInput, CustomerRefundStartPersistInput,
};
use super::{return_command_no, ReturnsService};
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::idempotency::normalize_idempotency_key;
use crate::approval::execution::{
    command_may_have_committed, command_recovery_delay, prepare_cancel, prepare_start,
};
use crate::audit::{AuditActor, CommandReceipt, CommandReceiptServiceExt as _};
use crate::document_registry::{find_approval_binding, new_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use database::{
    AccessControlExt, CustomerExt, DocumentRegistryExt, Executor, NoTransaction, ReceivableExt, ReturnsExt,
    Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::BusinessDocument;
use entities::document_registry::DocumentType;
use entities::ids::{
    CustomerAccountId, CustomerReceiptId, CustomerRefundId, ReceiptAllocationId, ReceivableEntryId,
    ReceivableEntryOffsetId,
};
use std::str::FromStr;

use entities::money::Amount;
use entities::receivable::{
    AllocationAction as ReceivableAllocationAction, CustomerReceiptStatus,
    EntryDirection as ReceivableEntryDirection, ReceiptAllocation, ReceiptAllocationData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryOffset, ReceivableEntryOffsetData, ReceivableEntryType,
};
use entities::returns::{CustomerRefund, CustomerRefundData, CustomerRefundStatus};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

/// 客户退款列表筛选条件类型。
type CustomerRefundFilter = <mongodb::Database as ReturnsExt>::CustomerRefundFilter;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 客户退款
    // -----------------------------------------------------------------------

    /// 分页查询客户退款列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn customer_refund_list(
        &self,
        params: &CustomerRefundListParams,
    ) -> Result<PageView<CustomerRefundView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerRefundFilter {
            refund_no: query.refund_no,
            customer_id: query.customer_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .customer_refunds()
            .search_customer_refunds(&filter, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            items.push(self.customer_refund_view(row.id).await?);
        }
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询客户退款详情。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    pub async fn customer_refund_detail(&self, id: &str) -> Result<CustomerRefundView> {
        self.customer_refund_view(id.to_string()).await
    }

    /// 登记客户退款草稿，并在同一事务绑定已发布审批定义。
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
    pub async fn create_customer_refund(
        &self,
        req: CreateCustomerRefundRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
        req.validate()?;
        let refund = CustomerRefund::new(
            CustomerRefundId::new(next_id()),
            CustomerRefundData {
                refund_no: req.refund_no,
                sales_return_case_id: req.sales_return_case_id,
                customer_id: req.customer_id,
                original_receipt_id: req.original_receipt_id,
                original_receivable_entry_id: req.original_receivable_entry_id,
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
        persist_created_customer_refund(&self.db, &self.rbac, refund.clone(), actor.clone()).await?;
        self.customer_refund_detail(&refund.base.id).await
    }

    /// 按原回款一次创建客户退款并启动审批。
    ///
    /// 单据注册、定义绑定、退款实体、审批快照、运行事实、入口任务和两类审计
    /// 全部在同一 MongoDB 事务内写入。
    pub async fn commit_customer_refund(
        &self,
        req: CommitCustomerRefundRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "customer-refund-commit-",
            actor.id(),
            "customer_refund.commit",
            "customer_refund",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(refund_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.customer_refund_detail(&refund_id).await;
        }
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))?;
        let source_fact_id = CustomerReceiptId::new(receipt.base.id.clone());
        let source_version = receipt.base.version;
        let customer_id = receipt
            .customer_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("原回款未关联经营客户，不能退款".to_string()))?;
        let mut refund = CustomerRefund::new(
            CustomerRefundId::new(next_id()),
            CustomerRefundData {
                refund_no: return_command_no("TK", actor.id(), &req.idempotency_key),
                sales_return_case_id: None,
                customer_id: customer_id.clone(),
                original_receipt_id: Some(source_fact_id.clone()),
                original_receivable_entry_id: None,
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
        let adapter = customer_refund_adapter()?;
        start_customer_refund_approval(&mut refund)?;
        let id = refund.base.id.clone();
        let subject = customer_refund_subject_ref(&id)?;
        let organization_id = load_customer_responsible_org_id(&self.db, &customer_id).await?;
        let _ = customer_refund_object_readable(&organization_id, actor.id())?;
        let now = Instant::now();
        let snapshot = build_customer_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::CustomerRefund,
            business_object_id: id.clone(),
            business_object_version: refund.base.version,
            context: BindingRevalidationContext {
                organization_id: organization_id.clone(),
                creator_id: actor.id().to_string(),
            },
        };
        let document = new_registered_document(&id, DocumentType::CustomerRefund, refund.refund_no.clone())?;
        let create_audit =
            actor
                .clone()
                .resource_log("customer_refund.create", "customer_refund", id.clone())?;
        let submit_audit =
            actor
                .clone()
                .resource_log("customer_refund.submit", "customer_refund", id.clone())?;
        let command_audit = command_receipt.audit(actor.clone(), id.clone())?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let idempotency_key = req.idempotency_key;
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    validate_customer_refund_source(&db, &source_fact_id, source_version, session).await?;
                    let binding = persist_bound_customer_refund_document(
                        &db,
                        &rbac,
                        document,
                        &bind_command,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                    let start_input = build_customer_refund_start_input(CustomerRefundStartInput {
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
                    db.customer_refunds().create(&refund, session).await?;
                    if let crate::approval::execution::PreparedExecution::Apply(writes) = prepared {
                        persist_runtime_writes(
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
        self.customer_refund_detail(&detail_id).await
    }

    /// 提交客户退款并调用统一 `start_approval`。
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
    /// * `NotFound` - 退款单或客户不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_customer_refund(
        &self,
        id: &str,
        mut req: SubmitCustomerRefundRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
        req.validate()?;
        req.idempotency_key = normalize_idempotency_key(&req.idempotency_key)?
            .as_str()
            .to_string();
        if self
            .replay_customer_refund_start(id, &req.idempotency_key, actor)
            .await?
            .is_some()
        {
            return self.customer_refund_detail(id).await;
        }
        let adapter = customer_refund_adapter()?;
        let mut refund = self.load_customer_refund(id).await?;
        ensure_expected_version(refund.base.version, req.expected_version)?;
        start_customer_refund_approval(&mut refund)?;
        self.dispatch_customer_refund_start(id, refund, req.idempotency_key, actor, adapter)
            .await
    }

    /// 撤回客户退款审批，成功后回到草稿且 `subject_version` 不回退。
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
    pub async fn cancel_customer_refund_approval(
        &self,
        id: &str,
        req: CancelCustomerRefundApprovalRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
        req.validate()?;
        let mut refund = self.load_customer_refund(id).await?;
        ensure_expected_version(refund.base.version, req.expected_version)?;
        self.persist_cancelled_customer_refund(id, &mut refund, &req, actor)
            .await?;
        self.customer_refund_detail(id).await
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_post() -> Result<CustomerRefundView> {
        Err(Error::ConflictError(
            "客户退款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string(),
        ))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_customer_refund_start(
        &self,
        id: &str,
        refund: CustomerRefund,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::adapter::CustomerRefundAdapter,
    ) -> Result<CustomerRefundView> {
        let subject = customer_refund_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let organization_id = self.customer_responsible_org_id(&refund.customer_id).await?;
        let snapshot = build_customer_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let start =
            customer_refund_start_command(id, refund.approval_subject_version, actor.id(), &idempotency_key);
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let _ = customer_refund_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &subject,
            refund.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_customer_refund_start_input(CustomerRefundStartInput {
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
        let persisted = persist_customer_refund_start(
            &self.db,
            CustomerRefundStartPersistInput {
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
            if !command_may_have_committed(&error) {
                return Err(error);
            }
            self.recover_customer_refund_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        self.customer_refund_detail(id).await
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_customer_refund_start(
        &self,
        refund_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let refund_id = refund_id.to_string();
            let idempotency_key = idempotency_key.to_string();
            let actor = actor.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        ensure_return_start_actor_active(&db, &actor, session).await?;
                        let refund = db
                            .customer_refunds()
                            .find_by_id(&refund_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
                        let customer = db
                            .customer_accounts()
                            .find_by_id(&refund.customer_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
                        let organization_id = customer_refund_responsible_org_id(customer.party_id.as_ref())?;
                        ensure_return_start_replay_authorized(
                            &db,
                            &rbac,
                            &actor,
                            DocumentType::CustomerRefund,
                            "customer_refund:submit",
                            &organization_id,
                            session,
                        )
                        .await?;
                        let binding = find_approval_binding(&db, &refund_id, session).await?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = customer_refund_subject_ref(&refund_id)?;
                        replay_return_start_with_executor(
                            &db,
                            DocumentType::CustomerRefund,
                            &subject,
                            subject_version,
                            &idempotency_key,
                            binding,
                            actor.id(),
                            session,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(instance_id)) => return Ok(instance_id),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 在任何当前状态/版本门禁前，以当前或下一主题版本精确回放既有启动。
    async fn replay_customer_refund_start(
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
                    ensure_return_start_actor_active(&db, &actor, session).await?;
                    let refund = db
                        .customer_refunds()
                        .find_by_id(&refund_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
                    let customer = db
                        .customer_accounts()
                        .find_by_id(&refund.customer_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
                    let organization_id = customer_refund_responsible_org_id(customer.party_id.as_ref())?;
                    ensure_return_start_replay_authorized(
                        &db,
                        &rbac,
                        &actor,
                        DocumentType::CustomerRefund,
                        "customer_refund:submit",
                        &organization_id,
                        session,
                    )
                    .await?;
                    let binding = find_approval_binding(&db, &refund_id, session).await?;
                    let binding = require_frozen_binding(binding.as_ref())?;
                    let subject = customer_refund_subject_ref(&refund_id)?;
                    for subject_version in replay_subject_versions(refund.approval_subject_version)? {
                        if let Some(instance_id) = replay_return_start_with_executor(
                            &db,
                            DocumentType::CustomerRefund,
                            &subject,
                            subject_version,
                            &idempotency_key,
                            binding,
                            actor.id(),
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

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_customer_refund(
        &self,
        id: &str,
        refund: &mut CustomerRefund,
        req: &CancelCustomerRefundApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = customer_refund_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = customer_refund_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, refund.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_customer_refund_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_customer_refund_domain_action(refund, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "customer_refund.cancel_approval",
            "customer_refund",
            id.to_string(),
        )?;
        persist_customer_refund_cancel(
            &self.db,
            CustomerRefundCancelPersistInput {
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

    /// 按主键读取客户退款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_customer_refund(&self, id: &str) -> Result<CustomerRefund> {
        self.db
            .customer_refunds()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))
    }

    /// 读取客户往来主体作为责任组织。
    ///
    /// # 错误
    /// 客户不存在或往来主体为空时返回错误。
    async fn customer_responsible_org_id(&self, customer_id: &CustomerAccountId) -> Result<String> {
        load_customer_responsible_org_id(&self.db, customer_id).await
    }

    /// 最终通过过账（§8.3-3 事务不变量）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 按原回款（或其核销分配）反向写入 `REVERSE` 回款核销分配；按条件原子
    /// 冲减子账已核销进度；写反向应收分录（减少）与分录抵销；退款单迁移为
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
    /// * `NotFound` - 退款单或原回款不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 累计退款超原回款、重复过账或超额冲减
    pub async fn post_customer_refund(&self, id: &str, actor: &AuditActor) -> Result<CustomerRefundView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let refund_id = id.to_string();
        let detail_id = refund_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    apply_customer_refund_final_post(&db, &refund_id, &actor_id, &actor_owned, session).await
                })
            })
            .await?;

        self.customer_refund_detail(&detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配客户退款单视图。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    async fn customer_refund_view(&self, id: String) -> Result<CustomerRefundView> {
        let refund = self
            .db
            .customer_refunds()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(CustomerRefundView {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no,
            status: refund.status,
            sales_return_case_id: refund.sales_return_case_id.map(|id| id.to_string()),
            customer_id: refund.customer_id.to_string(),
            original_receipt_id: refund.original_receipt_id.map(|id| id.to_string()),
            original_receivable_entry_id: refund.original_receivable_entry_id.map(|id| id.to_string()),
            reason_code: refund.reason_code,
            reason_text: refund.reason_text,
            amount: refund.amount,
            handled_by: refund.handled_by,
            reviewed_by: refund.reviewed_by,
            occurred_at: refund.occurred_at,
            version: refund.base.version,
            created_at: refund.base.created_at,
            approval: document_approval_view(binding.as_ref(), None, refund.status),
        })
    }
}

/// 在最终通过事务内执行客户退款过账副作用并写回退款单。
///
/// # 错误
/// 非审批中、原回款不存在、累计超额或仓储写入失败时返回错误。
pub(super) async fn apply_customer_refund_final_post(
    db: &Database,
    refund_id: &str,
    actor_id: &str,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let mut refund = db
        .customer_refunds()
        .find_by_id(refund_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
    if refund.status == CustomerRefundStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正退款不能再过账".to_string()));
    }
    ensure_final_approve_posting(&refund)?;
    execute_customer_refund_domain_action(
        &mut refund,
        crate::approval::policy::ApprovalDomainAction::CustomerRefundPost,
    )?;
    apply_customer_refund_posting(db, &refund, actor_id, session).await?;
    refund.mark_posted()?;
    db.customer_refunds().update(&mut refund, session).await?;
    let audit =
        actor
            .clone()
            .resource_log("customer_refund.post", "customer_refund", refund.base.id.clone())?;
    db.audit_logs().create(&audit, session).await?;
    Ok(())
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
async fn persist_created_customer_refund(
    db: &Database,
    rbac: &SharedRbacService,
    refund: CustomerRefund,
    actor: AuditActor,
) -> Result<()> {
    let organization_id = load_customer_responsible_org_id(db, &refund.customer_id).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::CustomerRefund,
        business_object_id: refund.base.id.clone(),
        business_object_version: refund.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let document = new_registered_document(
        &refund.base.id,
        DocumentType::CustomerRefund,
        refund.refund_no.clone(),
    )?;
    let audit = actor.clone().resource_log(
        "customer_refund.create",
        "customer_refund",
        refund.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_customer_refund_document(&db, &rbac, document, &bind_command, &actor, session)
                    .await?;
                db.customer_refunds().create(&refund, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 查询客户往来主体作为责任组织。
///
/// # 错误
/// 客户不存在或往来主体为空时返回错误。
async fn load_customer_responsible_org_id(db: &Database, customer_id: &CustomerAccountId) -> Result<String> {
    let customer = db
        .customer_accounts()
        .find_by_id(customer_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
    customer_refund_responsible_org_id(customer.party_id.as_ref())
}

/// 在创建退款的同一事务中重读并校验原回款事实。
///
/// 原事实必须仍为调用前读取的版本且已经过账，避免为草稿、审批中或已冲正回款
/// 创建无法最终执行的审批任务。
async fn validate_customer_refund_source(
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
        "只有已过账的客户回款才能发起退款",
    )?;
    if receipt.customer_id.is_none() {
        return Err(Error::BusinessLogicError(
            "原回款未关联经营客户，不能退款".to_string(),
        ));
    }
    Ok(())
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_customer_refund_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<entities::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = customer_refund_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("客户退款单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}

/// 在调用方事务内写入退款出账副作用。
///
/// # 错误
/// 原回款不存在、累计超额或仓储失败时返回错误。
async fn apply_customer_refund_posting(
    db: &Database,
    refund: &CustomerRefund,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let original_receipt_id = refund
        .original_receipt_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("按原应收分录退款由冲减分录完成".to_string()))?;
    let receipt = db
        .customer_receipts()
        .find_by_id(&original_receipt_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
    if receipt.status != CustomerReceiptStatus::Posted {
        return Err(Error::BusinessLogicError("只有已过账回款可以退款".to_string()));
    }
    let refunded_before: Amount = db
        .customer_refunds()
        .find_refunds_by_originals(std::slice::from_ref(&original_receipt_id), &[], session)
        .await?
        .iter()
        .filter(|other| other.base.id != refund.base.id)
        .fold(
            Amount::from_str("0.00").expect("固定零金额必须可解析"),
            |sum, other| sum.checked_add(other.amount),
        );
    if refunded_before.checked_add(refund.amount) > receipt.amount {
        return Err(Error::BusinessLogicError(
            "累计退款金额不得超过原回款金额".to_string(),
        ));
    }
    persist_refund_offsets_and_reversals(db, refund, &receipt, actor_id, session).await
}

/// 写入反向核销分配、冲减进度与减少分录。
///
/// # 错误
/// 跨主体、超额冲减或仓储失败时返回错误。
async fn persist_refund_offsets_and_reversals(
    db: &Database,
    refund: &CustomerRefund,
    receipt: &entities::receivable::CustomerReceipt,
    actor_id: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let allocations = db
        .receipt_allocations()
        .find_allocations_by_receipts(&[receipt.base.id.clone().into()], session)
        .await?;
    let (reverse_rows, chunks) = ReceiptAllocation::plan_reverse(&allocations, refund.amount)?;
    let seqs = ReceiptAllocation::next_allocation_seq_range(&allocations, reverse_rows.len())?;
    let decrease_entry = create_decrease_offsets(db, refund, receipt, actor_id, &chunks, session).await?;
    if let Some(entry) = decrease_entry {
        db.receivable_entries().create(&entry, session).await?;
    }
    for (reverse, seq) in reverse_rows.iter().zip(seqs.iter()) {
        let allocation = ReceiptAllocation::new(
            ReceiptAllocationId::new(next_id()),
            ReceiptAllocationData {
                customer_receipt_id: receipt.base.id.clone().into(),
                receivable_entry_id: reverse.entry_id.clone(),
                allocation_seq: *seq,
                allocation_action: ReceivableAllocationAction::Reverse,
                allocated_amount: reverse.amount,
                allocated_at: refund.occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        db.receipt_allocations().create(&allocation, session).await?;
    }
    Ok(())
}

/// 按冲减块写减少分录抵销并回冲已核销进度。
///
/// # 错误
/// 分录缺失、跨主体或超额冲减时返回错误。
async fn create_decrease_offsets(
    db: &Database,
    refund: &CustomerRefund,
    receipt: &entities::receivable::CustomerReceipt,
    actor_id: &str,
    chunks: &[entities::receivable::ReceiptReverseChunk],
    session: &mut mongodb::ClientSession,
) -> Result<Option<ReceivableEntry>> {
    let mut decrease_entry: Option<ReceivableEntry> = None;
    for (offset_index, chunk) in chunks.iter().enumerate() {
        let entry = db
            .receivable_entries()
            .find_by_id(&chunk.increase_entry_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        let account = db
            .receivable_accounts()
            .find_by_id(&entry.receivable_account_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        if account.counterparty_party_id != receipt.counterparty_party_id {
            return Err(Error::BusinessLogicError("禁止跨往来主体退款".to_string()));
        }
        let reverted = db
            .receivable_accounts()
            .revert_settlement(&entry.receivable_account_id, &chunk.amount, actor_id, session)
            .await?;
        if !reverted {
            return Err(Error::BusinessLogicError("退款冲减超过已核销金额".to_string()));
        }
        if decrease_entry.is_none() {
            decrease_entry = Some(ReceivableEntry::new(
                ReceivableEntryId::new(next_id()),
                ReceivableEntryData {
                    receivable_account_id: entry.receivable_account_id.clone(),
                    entry_type: ReceivableEntryType::Refund,
                    direction: ReceivableEntryDirection::Decrease,
                    amount: refund.amount,
                    due_date: entities::common::time::BusinessDate::today(),
                    source_fact_type: "customer_refund".to_string(),
                    source_document_id: refund.base.id.clone(),
                    source_revision_id: refund.base.id.clone(),
                    source_sequence: 1,
                    posted_at: refund.occurred_at,
                },
            )?);
        }
        let decrease_id = decrease_entry
            .as_ref()
            .ok_or_else(|| Error::Internal("退款减少分录未创建".to_string()))?
            .base
            .id
            .clone()
            .into();
        db.receivable_entry_offsets()
            .create(
                &ReceivableEntryOffset::new(
                    ReceivableEntryOffsetId::new(next_id()),
                    ReceivableEntryOffsetData {
                        decrease_entry_id: decrease_id,
                        increase_entry_id: chunk.increase_entry_id.clone(),
                        offset_sequence: offset_index as u32 + 1,
                        offset_amount: chunk.amount,
                    },
                )?,
                session,
            )
            .await?;
    }
    Ok(decrease_entry)
}

#[cfg(test)]
mod customer_refund_approval_tests {
    use super::{execute_customer_refund_domain_action, start_customer_refund_approval, ReturnsService};
    use crate::approval::policy::ApprovalDomainAction;
    use entities::common::time::Instant;
    use entities::ids::{CustomerAccountId, CustomerReceiptId, CustomerRefundId};
    use entities::money::Amount;
    use entities::returns::{CustomerRefund, CustomerRefundData, CustomerRefundStatus};
    use std::str::FromStr;

    fn draft_refund() -> CustomerRefund {
        CustomerRefund::new(
            CustomerRefundId::new("crf-1"),
            CustomerRefundData {
                refund_no: "RF-1".into(),
                sales_return_case_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                original_receipt_id: Some(CustomerReceiptId::new("cr-1")),
                original_receivable_entry_id: None,
                reason_code: None,
                reason_text: "质量退款".into(),
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
        let source = include_str!("customer_refund.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::CustomerRefund"));
        assert!(source.contains("persist_created_customer_refund"));
    }

    /// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
    #[test]
    fn create_path_calls_local_object_readable() {
        use super::super::adapter::customer_refund_object_readable;

        let production = include_str!("customer_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("customer_refund_object_readable"));
        assert!(!production.contains("adapter_object_read_decision"));
        assert!(customer_refund_object_readable("org-1", "u1").unwrap());
        assert!(customer_refund_object_readable(" ", "u1").is_err());
        assert!(customer_refund_object_readable("org-1", "").is_err());
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("customer_refund.rs");
        assert!(source.contains("pub async fn submit_customer_refund"));
        assert!(source.contains("customer_refund_start_command"));
        assert!(source.contains("refund.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_customer_refund，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_customer_refund() {
        let source = include_str!("customer_refund.rs");
        assert!(source.contains("pub async fn post_customer_refund"));
        assert!(source.contains("refund.mark_posted"));
        assert!(source.contains("CustomerRefundPost"));
        assert!(ReturnsService::reject_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("customer_refund.rs");
        assert!(source.contains("pub async fn cancel_customer_refund_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_customer_refund_cancel"));
        let _ = ReturnsService::reject_client_post();
        let mut refund = draft_refund();
        start_customer_refund_approval(&mut refund).unwrap();
        execute_customer_refund_domain_action(
            &mut refund,
            ApprovalDomainAction::CustomerRefundCancelApproval,
        )
        .unwrap();
        assert_eq!(refund.status, CustomerRefundStatus::Draft);
        assert_eq!(refund.approval_subject_version, 1);
    }

    /// 生产代码不得保留草稿直接过账或待复核旁路。
    #[test]
    fn production_closes_draft_post_and_pending_review() {
        let production = include_str!("customer_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("CustomerRefundStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }
}
