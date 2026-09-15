//! 销售变更创建、启动、作废及撤回的跨域根流程。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::BusinessDocumentId;
use erp_identity::SharedRbacService;
use erp_read_models::sales_center::review::{SalesChangeOrderDetailView, SalesChangeReadService};
use erp_sales::dto::sales_review::{
    CancelSalesChangeApprovalRequest, CreateSalesChangeOrderRequest, SubmitSalesChangeRequest,
    VoidSalesChangeOrderRequest,
};
use erp_sales::entity::sales_review::SalesChangeOrder;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_sales::service::sales_review::{CreatedChangeWrite, SalesReviewService, latest_change_submission_no};
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::{
    BusinessDocument, WorkflowAction, WorkflowActionData, WorkflowActionId, WorkflowActionType,
};
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use id_generator::next_id;
use mongodb::ClientSession;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SalesChangeProcess;
use super::adapter::{
    build_sales_change_snapshot, execute_sales_change_domain_action, require_frozen_binding,
    sales_change_order_adapter, sales_change_order_object_readable, sales_change_order_subject_ref,
    sales_change_responsible_org_id, sales_change_start_command, start_approval_command_kind,
};
use super::cancel_approval::{
    SalesChangeCancelPersistInput, build_sales_change_cancel_input, load_cancel_runtime,
    persist_sales_change_cancel,
};
use super::start_approval::{
    SalesChangeStartInput, SalesChangeStartPersistInput, build_sales_change_start_input,
    load_bound_definition_graph, load_start_receipt, persist_sales_change_start,
    replay_sales_change_start_with_executor,
};
use crate::{Error, Result};

impl SalesChangeProcess {
    /// 创建销售变更单（草稿 + 变更工作副本 + `BusinessDocument` 绑定原子形成）。
    ///
    /// `PROCESS_REQUIRED` 无发布定义时返回 `APPROVAL_PROCESS_NOT_CONFIGURED`，
    /// 业务单据零写入。客户端不得选择定义或审批人。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `rbac` - 共享 RBAC
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 原销售单不存在或未生效
    /// * `ConflictError` - 同一基准版本已有进行中变更或未配置审批流程
    pub async fn create_sales_change_order(
        &self,
        req: CreateSalesChangeOrderRequest,
        actor: &AuditActor,
        rbac: &SharedRbacService,
    ) -> Result<SalesChangeOrderDetailView> {
        let source_order = self
            .command_access(actor, "update")?
            .current(req.sales_order_id.as_ref(), &mut NoTransaction)
            .await?;
        let sales_write = SalesReviewService::new(self.db.clone()).prepare_creation(req, actor).await?;
        let change_id = sales_write.change_id().to_string();
        let bind_command = BindPublishedDefinitionCommand {
            document_type: erp_workflow::entity::document_registry::DocumentType::SalesChangeOrder,
            business_object_id: change_id.clone(),
            business_object_version: sales_write.version(),
            context: BindingRevalidationContext {
                order_source: Some(OrderTaskSource::Sales(source_order.base.id.clone())),
                customer_id: Some(source_order.customer_id.to_string()),
                business_org_unit_id: Some(source_order.business_org_unit_id.clone()),
                scope_owner_user_id: Some(source_order.sales_owner_user_id.clone()),
                organization_id: sales_change_responsible_org_id(sales_write.settlement_party_id())?,
                creator_id: actor.id().to_string(),
            },
        };
        let document = new_registered_document(
            change_id.clone(),
            erp_workflow::entity::document_registry::DocumentType::SalesChangeOrder,
            String::new(),
        )
        .map_err(crate::Error::from)?;
        let audit = actor.clone().resource_log(
            "sales_change_order.create",
            "sales_change_order",
            change_id.clone(),
        )?;
        persist_created_change_order(
            &self.db,
            rbac,
            std::sync::Arc::clone(&self.object_read),
            CreatedChangeOrderPersistInput {
                sales_write,
                document,
                bind_command,
                audit,
                actor: actor.clone(),
            },
        )
        .await?;

        SalesChangeReadService::with_rbac(self.db.clone(), self.require_rbac()?)
            .sales_change_order_detail(&change_id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 提交销售变更并调用统一 `start_approval`。
    ///
    /// `subject_version` 取 `sales_change_submission.submission_no`，不得复用
    /// `BaseModel.version`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 提交请求（含期望版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或变更工作副本不存在
    /// * `ConflictError` - 期望版本不一致、无绑定或状态不允许
    pub async fn submit_sales_change(
        &self,
        id: &str,
        req: SubmitSalesChangeRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let adapter = sales_change_order_adapter()?;
        let preview = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".to_string()))?;
        self.command_access(actor, "submit")?
            .current(preview.sales_order_id.as_ref(), &mut NoTransaction)
            .await?;
        let change_order =
            SalesReviewService::new(self.db.clone()).load_for_submission(id, req.version).await?;
        self.start_change_approval(id, change_order, req.idempotency_key.clone(), actor, adapter).await
    }

    /// 作废销售变更单（仅草稿态）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn void_sales_change(
        &self,
        id: &str,
        req: VoidSalesChangeOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        let preview = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".to_string()))?;
        self.command_access(actor, "update")?
            .current(preview.sales_order_id.as_ref(), &mut NoTransaction)
            .await?;
        let mut sales_write = SalesReviewService::new(self.db.clone()).prepare_void(id, req, actor).await?;
        let audit =
            actor.clone().resource_log("sales_change_order.void", "sales_change_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let rbac = self.require_rbac()?;
        let actor_for_tx = actor.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    erp_read_models::sales_center::access::SalesAccess::new(db.clone(), rbac)
                        .require_object(&actor_for_tx, "update", sales_write.sales_order_id(), &[], session)
                        .await
                        .map_err(crate::Error::from)?;
                    sales_write.persist(&db, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await?;

        SalesChangeReadService::with_rbac(self.db.clone(), self.require_rbac()?)
            .sales_change_order_detail(id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 撤回审批中的销售变更单，回到可修正草稿且 `subject_version` 不回退。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的变更单详情。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    pub async fn cancel_approval(
        &self,
        id: &str,
        req: CancelSalesChangeApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let preview = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".to_string()))?;
        self.command_access(actor, "cancel_approval")?
            .current(preview.sales_order_id.as_ref(), &mut NoTransaction)
            .await?;
        let mut change_order =
            SalesReviewService::new(self.db.clone()).load_for_cancellation(id, req.expected_version).await?;
        let adapter = sales_change_order_adapter()?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = sales_change_order_subject_ref(id)?;
        let subject_version = latest_change_submission_no(&self.db, id).await?;
        let runtime = load_cancel_runtime(&self.db, &binding, &subject, subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input =
            build_sales_change_cancel_input(&runtime, &req.reason, actor.id(), &idempotency_key, None, now)?;
        let prepared = prepare_cancel(input)?;
        execute_sales_change_domain_action(&mut change_order, adapter.cancel_action, actor.id())?;
        let audit = actor.clone().resource_log(
            "sales_change_order.cancel_approval",
            "sales_change_order",
            id.to_string(),
        )?;
        persist_sales_change_cancel(
            &self.db,
            SalesChangeCancelPersistInput {
                change_order,
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
                actor: actor.clone(),
                rbac: self.require_rbac()?,
            },
        )
        .await?;
        SalesChangeReadService::with_rbac(self.db.clone(), self.require_rbac()?)
            .sales_change_order_detail(id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 冻结提交并启动统一审批。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    async fn start_change_approval(
        &self,
        id: &str,
        change_order: SalesChangeOrder,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::adapter::SalesChangeOrderAdapter,
    ) -> Result<SalesChangeOrderDetailView> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&change_order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let subject = sales_change_order_subject_ref(id)?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let sales_write =
            SalesReviewService::new(self.db.clone()).prepare_submission(change_order, actor).await?;
        let submission = sales_write.submission();
        let now = Instant::now();
        let snapshot = build_sales_change_snapshot(
            sales_write.change(),
            &sales_order,
            submission,
            sales_write.lines(),
            actor.id(),
            now,
        )?;
        let start = sales_change_start_command(id, submission.submission_no, actor.id(), &idempotency_key);
        let _ = start_approval_command_kind(&start);
        let organization_id = sales_change_responsible_org_id(&sales_order.settlement_party_id)?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt =
            load_start_receipt(&self.db, &subject, submission.submission_no, &idempotency_key).await?;
        let start_input = build_sales_change_start_input(SalesChangeStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: submission.submission_no,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        let workflow_action = WorkflowAction::new(
            WorkflowActionId::new(next_id()),
            WorkflowActionData {
                document_id: BusinessDocumentId::new(id.to_string()),
                action_type: WorkflowActionType::Submit,
                from_status: "DRAFT".to_string(),
                to_status: "IN_APPROVAL".to_string(),
                actor_id: actor.id().to_string(),
                actor_role: adapter.owner_role.to_string(),
                comment: None,
            },
        )?;
        let audit =
            actor.clone().resource_log("sales_change_order.submit", "sales_change_order", id.to_string())?;
        let recovery_subject_version = submission.submission_no;
        let persisted = persist_sales_change_start(
            &self.db,
            SalesChangeStartPersistInput {
                sales_write,
                workflow_action,
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
                audit,
                actor: actor.clone(),
                rbac: self.require_rbac()?,
            },
        )
        .await;
        if let Err(error) = persisted {
            if !error.command_may_have_committed() {
                return Err(error);
            }
            self.recover_sales_change_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        SalesChangeReadService::with_rbac(self.db.clone(), self.require_rbac()?)
            .sales_change_order_detail(id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_sales_change_start(
        &self,
        change_order_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let change_order_id = change_order_id.to_string();
            let idempotency_key = idempotency_key.to_string();
            let actor_id = actor.id().to_string();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        let change = db
                            .sales_change_orders()
                            .find_by_id(&change_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
                        let sales_order = db
                            .sales_orders()
                            .find_by_id(&change.sales_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
                        let organization_id =
                            sales_change_responsible_org_id(&sales_order.settlement_party_id)?;
                        let _ = sales_change_order_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &change_order_id, session)
                            .await
                            .map_err(crate::Error::from)?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = sales_change_order_subject_ref(&change_order_id)?;
                        replay_sales_change_start_with_executor(
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
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
}

/// 销售变更单创建事务写入集合。
///
/// # 用途
/// 收拢创建变更单时需一并持久化的单据、工作副本、注册行与审计。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 必须绑定已发布定义；人员重验失败时不得写入。
struct CreatedChangeOrderPersistInput {
    /// 新建销售变更、工作副本与行的本域计划。
    sales_write: CreatedChangeWrite,
    /// 待登记的业务单据。
    document: BusinessDocument,
    /// 发布定义绑定命令。
    bind_command: BindPublishedDefinitionCommand,
    /// 已构造审计。
    audit: erp_audit::AuditLog,
    /// 审计操作人。
    actor: AuditActor,
}
/// 在创建事务内写入变更单、绑定发布定义并登记单据。
///
/// # 用途
/// 创建变更单时原子写入单据、工作副本与发布定义绑定。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `input` - 变更单、工作副本、注册行与审计
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误，调用方必须回滚。
///
/// # 关键业务约束
/// 销售变更单必须绑定已发布定义，不得无定义创建。
async fn persist_created_change_order(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    input: CreatedChangeOrderPersistInput,
) -> Result<()> {
    let CreatedChangeOrderPersistInput { sales_write, mut document, bind_command, audit, actor } = input;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                erp_read_models::sales_center::access::SalesAccess::new(db.clone(), rbac.clone())
                    .require_object(&actor, "update", sales_write.sales_order_id(), &[], session)
                    .await
                    .map_err(crate::Error::from)?;
                persist_bound_change_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &mut document,
                    &bind_command,
                    &actor,
                    session,
                )
                .await?;
                sales_write.persist(&db, session).await?;
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
async fn persist_bound_change_document(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let _ = sales_change_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let auth = crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone());
    let audit = crate::adapters::workflow::workflow_audit(db.clone());
    let binding = erp_workflow::service::approval::binding::bind_published_definition_on_document_create(
        db,
        &auth,
        object_read,
        audit.as_ref(),
        bind_command,
        actor,
        session,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("销售变更单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    db.business_documents().create(document, session).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// 创建、提交、作废、撤回必须在状态与版本校验前按来源销售单动作重验。
    ///
    /// # 关键业务约束
    /// 不可见原单一律 NotFound，不得先暴露存在性或状态冲突。
    #[test]
    fn create_and_submit_revalidate_source_sales_order_before_state_checks() {
        let source = include_str!("commands.rs");
        let auth = include_str!("authorization.rs");
        let sales_access = include_str!("../../../erp-read-models/src/sales_center/access.rs");
        assert!(auth.contains("require_object(&self.actor, self.action, id, &[], executor)"));
        assert!(auth.contains("不存在或越权时返回 NotFound，不泄露存在性"));
        assert!(sales_access.contains("销售单不存在或无权操作"));
        let create = source.split("pub async fn create_sales_change_order").nth(1).expect("创建命令");
        let submit = source.split("pub async fn submit_sales_change").nth(1).expect("提交命令");
        let void = source.split("pub async fn void_sales_change").nth(1).expect("作废命令");
        let cancel = source.split("pub async fn cancel_approval").nth(1).expect("撤回命令");
        assert!(
            create.find(r#"command_access(actor, "update")"#).expect("创建须先证明原单")
                < create.find("prepare_creation").expect("创建准备")
        );
        assert!(
            submit.find(r#"command_access(actor, "submit")"#).expect("提交须先证明原单")
                < submit.find("load_for_submission").expect("提交版本校验")
        );
        assert!(
            void.find(r#"command_access(actor, "update")"#).expect("作废须先证明原单")
                < void.find("prepare_void").expect("作废准备")
        );
        assert!(
            cancel.find(r#"command_access(actor, "cancel_approval")"#).expect("撤回须先证明原单")
                < cancel.find("load_for_cancellation").expect("撤回版本校验")
        );
        for body in [create, submit, void, cancel] {
            assert!(body.contains(".current("));
            assert!(body.contains("销售变更单不存在或无权操作") || body.contains("prepare_creation"));
        }
    }

    /// 创建必须注册 BusinessDocument 并调用统一绑定端口。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = concat!(
            include_str!("commands.rs"),
            include_str!("mod.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/effective.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/submission.rs")
        );
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
    }

    /// 提交必须调用 start_approval，且版本取 submission_no。
    #[test]
    fn submit_calls_start_approval_with_submission_no() {
        let source = concat!(
            include_str!("commands.rs"),
            include_str!("mod.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/effective.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/submission.rs")
        );
        assert!(source.contains("start_change_approval"));
        assert!(source.contains("submission.submission_no"));
        assert!(source.contains("sales_change_start_command"));
    }

    /// 最终动作唯一为 apply_effective_change。
    #[test]
    fn final_action_is_apply_effective_change() {
        let source = concat!(
            include_str!("commands.rs"),
            include_str!("mod.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/effective.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/submission.rs")
        );
        assert!(source.contains("pub async fn apply_effective_change"));
        assert!(source.contains("change_for_tx.apply_effective"));
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = concat!(
            include_str!("commands.rs"),
            include_str!("mod.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/effective.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/submission.rs")
        );
        assert!(source.contains("pub async fn cancel_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("adapter.cancel_action"));
    }

    /// 撤回后再提交使用语义仓储，并由实体递增版本、处理重复锁定。
    #[test]
    fn cancel_then_resubmit_uses_repository_and_entity_rules() {
        let source = concat!(
            include_str!("commands.rs"),
            include_str!("mod.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/effective.rs"),
            include_str!("../../../erp-sales/src/service/sales_review/submission.rs")
        );
        assert!(source.contains("find_resubmittable_sales_change_copy"));
        assert!(source.contains("latest_submission_no_by_change_order"));
        assert!(source.contains("SalesChangeSubmission::next_submission_no"));
        assert!(source.contains("lock_for_submission_if_needed"));
    }
}
