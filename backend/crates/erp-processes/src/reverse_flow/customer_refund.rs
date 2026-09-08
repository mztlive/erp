mod start;
use super::adapter::{
    build_customer_refund_snapshot, customer_refund_adapter, customer_refund_object_readable,
    customer_refund_responsible_org_id, customer_refund_start_command, customer_refund_subject_ref,
    execute_customer_refund_domain_action, require_frozen_binding, start_approval_command_kind,
};
use super::cancel_approval::{
    build_customer_refund_cancel_input, load_cancel_runtime, persist_customer_refund_cancel,
    CustomerRefundCancelPersistInput,
};
use super::start_approval::{
    build_customer_refund_start_input, ensure_return_start_actor_active,
    ensure_return_start_replay_authorized, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, load_start_receipt, persist_customer_refund_start,
    persist_runtime_writes, replay_return_start_with_executor, replay_subject_versions,
    CustomerRefundStartInput, CustomerRefundStartPersistInput, ReplayReturnStartInput,
};
use super::ReturnsProcess;
use crate::{Error, Result};
use application_core::AuditActor;
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::AuditExt;
use erp_audit::CommandReceiptServiceExt as _;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, CustomerReceiptId};
use erp_customer::CustomerExt;
use erp_identity::SharedRbacService;
use erp_read_models::returns_center::dto::CustomerRefundView;
use erp_returns::dto::{
    CancelCustomerRefundApprovalRequest, CommitCustomerRefundRequest, CreateCustomerRefundRequest,
    SubmitCustomerRefundRequest,
};
use erp_returns::service::approval::start_customer_refund_approval;
use erp_returns::service::version_conflict::conflict_if_stale_version;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::binding::{attach_published_binding, BindPublishedDefinitionCommand};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use erp_workflow::DocumentRegistryExt;
use persistence_core::{Executor, NoTransaction, Transactional};

use erp_finance::entity::receivable::CustomerReceiptStatus;
use erp_returns::entity::returns::CustomerRefund;
use mongodb::Database;
use validator::Validate;

impl ReturnsProcess {
    // -----------------------------------------------------------------------
    // 客户退款
    // -----------------------------------------------------------------------

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
        let refund = erp_returns::service::ReturnsService::prepare_customer_refund(req, actor.id())?;
        persist_created_customer_refund(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            refund.clone(),
            actor.clone(),
        )
        .await?;
        self.reads()
            .customer_refund_detail(&refund.base.id)
            .await
            .map_err(crate::Error::from)
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
            return self
                .reads()
                .customer_refund_detail(&refund_id)
                .await
                .map_err(crate::Error::from);
        }
        let receipt = erp_finance::service::receivable::customer_refund::load_customer_refund_source(
            &self.db,
            &req.source_fact_id,
            &mut NoTransaction,
        )
        .await?;
        let source_fact_id = CustomerReceiptId::new(receipt.base.id.clone());
        let source_version = receipt.base.version;
        let mut refund = erp_returns::service::ReturnsService::prepare_committed_customer_refund(
            &req,
            &customer_refund_source_fact(&receipt),
            actor.id(),
        )?;
        let customer_id = refund.customer_id.clone();
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
        let document = new_registered_document(&id, DocumentType::CustomerRefund, refund.refund_no.clone())
            .map_err(crate::Error::from)?;
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
        let object_read = std::sync::Arc::clone(&self.object_read);
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
                        object_read.as_ref(),
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
                    erp_returns::service::ReturnsService::new(db.clone())
                        .create_customer_refund_in_transaction(&refund, session)
                        .await?;
                    if let erp_workflow::service::approval::execution::PreparedExecution::Apply(writes) =
                        prepared
                    {
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
                    Ok::<(), crate::Error>(())
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
        self.reads()
            .customer_refund_detail(&detail_id)
            .await
            .map_err(crate::Error::from)
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
            return self
                .reads()
                .customer_refund_detail(id)
                .await
                .map_err(crate::Error::from);
        }
        let adapter = customer_refund_adapter()?;
        let mut refund = self.domain().load_customer_refund(id, &mut NoTransaction).await?;
        conflict_if_stale_version(refund.matches_version(req.expected_version))?;
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
        let mut refund = self.domain().load_customer_refund(id, &mut NoTransaction).await?;
        conflict_if_stale_version(refund.matches_version(req.expected_version))?;
        self.persist_cancelled_customer_refund(id, &mut refund, &req, actor)
            .await?;
        self.reads()
            .customer_refund_detail(id)
            .await
            .map_err(crate::Error::from)
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
        let _object_read = std::sync::Arc::clone(&self.object_read);
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

        self.reads()
            .customer_refund_detail(&detail_id)
            .await
            .map_err(crate::Error::from)
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
    session: &mut dyn Executor,
) -> Result<()> {
    let domain = erp_returns::service::ReturnsService::new(db.clone());
    let mut refund = domain
        .prepare_customer_refund_final_post(refund_id, session)
        .await?;
    execute_customer_refund_domain_action(
        &mut refund,
        erp_workflow::service::approval::policy::ApprovalDomainAction::CustomerRefundPost,
    )?;
    execute_refund_posting(
        &mut MongoRefundPosting {
            db,
            refund: &mut refund,
            actor_id,
            actor,
        },
        session,
    )
    .await?;
    Ok(())
}

/// 客户退款最终通过后各域实际写入的固定顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefundPostingStep {
    Finance,
    Refund,
    Audit,
    SalesProgress,
}
#[async_trait::async_trait]
trait RefundPostingSteps: Send {
    async fn apply(&mut self, step: RefundPostingStep, executor: &mut dyn Executor) -> Result<()>;
}
async fn execute_refund_posting(
    steps: &mut impl RefundPostingSteps,
    executor: &mut dyn Executor,
) -> Result<()> {
    for step in [
        RefundPostingStep::Finance,
        RefundPostingStep::Refund,
        RefundPostingStep::Audit,
        RefundPostingStep::SalesProgress,
    ] {
        steps.apply(step, executor).await?;
    }
    Ok(())
}
struct MongoRefundPosting<'a> {
    db: &'a Database,
    refund: &'a mut CustomerRefund,
    actor_id: &'a str,
    actor: &'a AuditActor,
}
#[async_trait::async_trait]
impl RefundPostingSteps for MongoRefundPosting<'_> {
    async fn apply(&mut self, step: RefundPostingStep, session: &mut dyn Executor) -> Result<()> {
        let db = self.db;
        let refund = &mut *self.refund;
        let actor_id = self.actor_id;
        let actor = self.actor;
        match step {
            RefundPostingStep::Finance => {
                apply_customer_refund_posting(db, refund, actor_id, session).await?;
            }
            RefundPostingStep::Refund => {
                erp_returns::service::ReturnsService::new(db.clone())
                    .persist_posted_customer_refund(refund, session)
                    .await?;
            }
            RefundPostingStep::SalesProgress => {
                let receipt_id = erp_returns::service::ReturnsService::customer_refund_receipt_id(refund)?;
                let sales =
                    erp_finance::service::receivable::receipt_reversal::receipt_allocation_sales_order_ids(
                        db,
                        &receipt_id,
                        session,
                    )
                    .await?;
                for sales_id in sales {
                    crate::order_to_cash::progress::update_sales_order_money_progress(
                        db,
                        session,
                        &sales_id,
                        actor_id.to_string(),
                        None,
                    )
                    .await?;
                }
            }
            RefundPostingStep::Audit => {
                let audit = actor.clone().resource_log(
                    "customer_refund.post",
                    "customer_refund",
                    refund.base.id.clone(),
                )?;
                db.audit_logs().create(&audit, session).await?;
            }
        }
        Ok(())
    }
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
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
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
    )
    .map_err(crate::Error::from)?;
    let audit = actor.clone().resource_log(
        "customer_refund.create",
        "customer_refund",
        refund.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_customer_refund_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    document,
                    &bind_command,
                    &actor,
                    session,
                )
                .await?;
                erp_returns::service::ReturnsService::new(db.clone())
                    .create_customer_refund_in_transaction(&refund, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::Error>(())
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
    let receipt = erp_finance::service::receivable::customer_refund::load_customer_refund_source(
        db,
        source_fact_id,
        executor,
    )
    .await?;
    erp_returns::service::customer_refund::ensure_customer_refund_source(
        &customer_refund_source_fact(&receipt),
        expected_version,
    )?;
    Ok(())
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_customer_refund_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = customer_refund_object_readable(
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
    session: &mut dyn Executor,
) -> Result<()> {
    let original_receipt_id = erp_returns::service::ReturnsService::customer_refund_receipt_id(refund)?;
    let receipt = erp_finance::service::receivable::customer_refund::load_posted_refund_receipt(
        db,
        &original_receipt_id,
        session,
    )
    .await?;
    erp_returns::service::ReturnsService::new(db.clone())
        .validate_customer_refund_amount(refund, &original_receipt_id, receipt.amount, session)
        .await?;
    erp_finance::service::receivable::customer_refund::persist_refund_offsets_and_reversals(
        db,
        &erp_finance::service::receivable::customer_refund::CustomerRefundPosting {
            refund_id: refund.base.id.clone(),
            amount: refund.amount,
            occurred_at: refund.occurred_at,
        },
        &receipt,
        actor_id,
        session,
    )
    .await?;
    Ok(())
}

/// 在原读取位置解释财务实体，仅投影退款消费的版本/金额/客户/状态事实。
fn customer_refund_source_fact(
    receipt: &erp_finance::entity::receivable::CustomerReceipt,
) -> erp_returns::service::customer_refund::CustomerRefundSourceFact {
    erp_returns::service::customer_refund::CustomerRefundSourceFact {
        id: receipt.base.id.clone().into(),
        version: receipt.base.version,
        amount: receipt.amount,
        customer_id: receipt.customer_id.clone(),
        is_posted: receipt.status == CustomerReceiptStatus::Posted,
    }
}

#[cfg(test)]
mod customer_refund_approval_tests {
    use super::{execute_customer_refund_domain_action, start_customer_refund_approval};
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAccountId, CustomerReceiptId, CustomerRefundId};
    use erp_core::money::Amount;
    use erp_returns::entity::returns::{CustomerRefund, CustomerRefundData, CustomerRefundStatus};
    use erp_workflow::service::approval::policy::ApprovalDomainAction;
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
        let source = [
            include_str!("customer_refund.rs"),
            include_str!("customer_refund/start.rs"),
        ]
        .join("\n");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::CustomerRefund"));
        assert!(source.contains("persist_created_customer_refund"));
    }

    /// 本阶段只登记并调用本地对象读取权，不得改写共享闸门。
    #[test]
    fn create_path_calls_local_object_readable() {
        use super::super::adapter::customer_refund_object_readable;

        let production = [
            include_str!("customer_refund.rs")
                .split("#[cfg(test)]")
                .next()
                .expect("生产代码"),
            include_str!("customer_refund/start.rs"),
        ]
        .join("\n");
        assert!(production.contains("customer_refund_object_readable"));
        assert!(!production.contains("adapter_object_read_decision"));
        assert!(customer_refund_object_readable("org-1", "u1").unwrap());
        assert!(customer_refund_object_readable(" ", "u1").is_err());
        assert!(customer_refund_object_readable("org-1", "").is_err());
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = [
            include_str!("customer_refund.rs"),
            include_str!("customer_refund/start.rs"),
        ]
        .join("\n");
        assert!(source.contains("pub async fn submit_customer_refund"));
        assert!(source.contains("customer_refund_start_command"));
        assert!(source.contains("refund.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_customer_refund，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_customer_refund() {
        let source = [
            include_str!("customer_refund.rs"),
            include_str!("customer_refund/start.rs"),
        ]
        .join("\n");
        assert!(source.contains("pub async fn post_customer_refund"));
        assert!(
            include_str!("../../../erp-returns/src/service/customer_refund.rs")
                .contains("refund.mark_posted")
        );
        assert!(source.contains("CustomerRefundPost"));
        assert!(erp_returns::service::ReturnsService::reject_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = [
            include_str!("customer_refund.rs"),
            include_str!("customer_refund/start.rs"),
        ]
        .join("\n");
        assert!(source.contains("pub async fn cancel_customer_refund_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_customer_refund_cancel"));
        let _ = erp_returns::service::ReturnsService::reject_client_post();
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
        let production = [
            include_str!("customer_refund.rs")
                .split("#[cfg(test)]")
                .next()
                .expect("生产代码"),
            include_str!("customer_refund/start.rs"),
        ]
        .join("\n");
        assert!(!production.contains("CustomerRefundStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }

    /// 冲减块必须批量读取分录与账户，逐账户原子回冲仍留在 Service。
    #[test]
    fn decrease_offsets_batch_entry_and_account_reads() {
        let production = include_str!("../../../erp-finance/src/service/receivable/customer_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("load_receivable_offset_facts"));
        assert!(production.contains("revert_settlement"));
        assert!(!production.contains("find_by_id(&chunk.increase_entry_id"));
        assert!(!production.contains("find_by_id(&entry.receivable_account_id"));
    }
}

#[cfg(test)]
mod posting_contract_tests {
    use super::*;
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct RecordingSteps {
        executor: usize,
        calls: Vec<RefundPostingStep>,
        fail_at: Option<RefundPostingStep>,
    }
    #[async_trait::async_trait]
    impl RefundPostingSteps for RecordingSteps {
        async fn apply(&mut self, step: RefundPostingStep, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(step);
            if self.fail_at == Some(step) {
                return Err(Error::ConflictError("原退款写入冲突".into()));
            }
            Ok(())
        }
    }
    #[tokio::test]
    async fn refund_finance_status_audit_keep_original_order_and_same_executor() {
        let mut executor = TestExecutor { _identity: 1 };
        let mut steps = RecordingSteps {
            executor: &mut executor as *mut TestExecutor as usize,
            calls: vec![],
            fail_at: None,
        };
        execute_refund_posting(&mut steps, &mut executor).await.unwrap();
        assert_eq!(
            steps.calls,
            [
                RefundPostingStep::Finance,
                RefundPostingStep::Refund,
                RefundPostingStep::Audit,
                RefundPostingStep::SalesProgress
            ]
        );
    }
    #[tokio::test]
    async fn refund_failure_stops_later_domains_with_original_error() {
        let order = [
            RefundPostingStep::Finance,
            RefundPostingStep::Refund,
            RefundPostingStep::Audit,
            RefundPostingStep::SalesProgress,
        ];
        for (index, step) in order.iter().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let mut steps = RecordingSteps {
                executor: &mut executor as *mut TestExecutor as usize,
                calls: vec![],
                fail_at: Some(*step),
            };
            assert!(
                matches!(execute_refund_posting(&mut steps,&mut executor).await,Err(Error::ConflictError(message)) if message=="原退款写入冲突")
            );
            assert_eq!(steps.calls, order[..=index]);
        }
    }
}
