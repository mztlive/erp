use application_core::{AuditActor, CommandFingerprint};
use bpm::model::SubjectRef;
use erp_approval_runtime::approver_directory::resolve_start_approvers;
use erp_core::common::time::Instant;
use erp_core::ids::{
    BusinessDocumentId, SalesOrderId, SalesOrderSubmissionId, SalesOrderWorkingCopyId, WorkflowActionId,
};
use erp_permissions::sales_order as sales_order_permissions;
use erp_sales::dto::sales_order::{SalesOrderDraftRequest, SubmissionView, SubmitSalesOrderRequest};
use erp_sales::entity::command_receipt::SalesCommandResult;
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderSubmission, SalesOrderSubmissionLine, SalesOrderWorkingCopy,
    SalesOrderWorkingCopyLine, WorkingPurpose,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::SalesOrderService;
use erp_sales::service::sales_order::command::identity::{
    sales_submission_audit_id, sales_submission_fingerprint,
};
use erp_sales::service::sales_order::mapper::{build_submission, build_submission_lines, submission_view};
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{WorkflowAction, WorkflowActionData, WorkflowActionType};
use erp_workflow::service::approval::execution::{PreparedExecution, command_recovery_delay, prepare_start};
use erp_workflow::service::document_registry::find_approval_binding;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::adapter::{
    SalesApprovalPorts, SalesOrderStartCommand, build_sales_order_snapshot,
    execute_sales_order_domain_action, reject_legacy_card_sales_decision, reject_legacy_card_sales_work_item,
    require_frozen_binding, sales_approval_ports, sales_order_responsible_org_id, sales_order_start_command,
};
use super::super::adapters::catalog::CatalogQualificationAdapter;
use super::super::authorization::SalesCommandAccess;
use super::super::start_approval::{
    SalesOrderStartInput, SalesOrderStartPersistInput, SalesOrderWorkingCopyPersistPlan,
    build_sales_order_start_input, load_bound_definition_graph_with_executor, load_start_receipt,
    persist_sales_order_start,
};
use crate::order_to_cash::command_event::{
    SalesCommandEvent, finish_receipt_recovery, replay_submission_with_executor,
};
use crate::order_to_cash::subject_ref_for_sales_business;
use crate::{Error, Result};

/// 原提交命令的幂等身份，在预读和实际提交之间保持一致。
#[derive(Clone)]
struct SubmissionIdentity {
    idempotency_key: String,
    audit_id: String,
    fingerprint: String,
    key_hash: CommandFingerprint,
}

impl SubmissionIdentity {
    fn new(id: &str, req: &SubmitSalesOrderRequest, actor: &AuditActor) -> Result<Self> {
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        Ok(Self {
            audit_id: sales_submission_audit_id(actor.id(), id, &idempotency_key),
            fingerprint: sales_submission_fingerprint(actor.id(), id, req)?,
            key_hash: CommandFingerprint::from_parts([idempotency_key.clone()]),
            idempotency_key,
        })
    }
}

/// 销售单提交并启动审批所需的单据集合。
///
/// # 用途
/// 将销售单、工作副本与冻结提交打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 提交行必须由当前工作副本冻结。
struct ApprovalSubmissionStart<'a> {
    resubmission_reason: Option<String>,
    /// 销售单主键。
    id: &'a str,
    /// 已进入审批中的销售单。
    order: SalesOrder,
    /// 已锁定的工作副本。
    working_copy: SalesOrderWorkingCopy,
    /// 冻结提交头。
    submission: SalesOrderSubmission,
    /// 冻结提交行。
    submission_lines: Vec<SalesOrderSubmissionLine>,
    /// 工作副本行，用于可售引用重验。
    copy_lines: Vec<SalesOrderWorkingCopyLine>,
    /// 草稿替换与补开副本的事务写入计划。
    working_copy_plan: SalesOrderWorkingCopyPersistPlan,
}

struct PreparedSubmissionCopy {
    order: SalesOrder,
    working_copy: SalesOrderWorkingCopy,
    copy_lines: Vec<SalesOrderWorkingCopyLine>,
    working_copy_plan: SalesOrderWorkingCopyPersistPlan,
}

/// 提交阶段确定的授权、定义、快照和运行时间；不执行写入。
struct SalesSubmissionApprovalContext {
    access: SalesCommandAccess,
    expected_order_version: u64,
    ports: SalesApprovalPorts,
    subject: SubjectRef,
    binding: ApprovalDefinitionBinding,
    now: Instant,
    snapshot: ApprovalSubjectSnapshotPayload,
    organization_id: String,
}

struct PreparedSalesSubmission {
    access: SalesCommandAccess,
    expected_order_version: u64,
    input: SalesOrderStartPersistInput,
}

/// 禁止回退 `CARD_SALES_APPROVAL` 或卡券专用工作项。
///
/// # 参数
/// * `start` - 目标启动命令。
///
/// # 返回
/// 主体种类不是旧卡券定义时返回。
///
/// # 错误
/// 主体种类为旧卡券定义时返回冲突。
pub(super) fn ensure_unified_start_command(start: &SalesOrderStartCommand) -> Result<()> {
    reject_legacy_card_sales_work_item("DOCUMENT_APPROVAL")?;
    if start.subject_kind == "CARD_SALES_APPROVAL" {
        return reject_legacy_card_sales_decision();
    }
    Ok(())
}

impl SalesOrderCommandProcess {
    /// 提交销售单并冻结提交快照。
    ///
    /// `GoodsService` 与 `Voucher` 均直接调用统一 `start_approval`，以
    /// `submission_no` 冻结 `subject_version` 与快照；禁止经 `sales_review`
    /// 准入、`CARD_SALES_APPROVAL` 或第二条启动路径。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 提交请求（含期望草稿版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交快照视图；已提交过的草稿幂等返回既有提交。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或有效工作副本不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    #[tracing::instrument(
        name = "sales_order.submit",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "submit")
    )]
    pub async fn submit_sales_order(
        &self,
        id: &str,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmissionView> {
        req.validate()?;
        let identity = SubmissionIdentity::new(id, &req, actor)?;
        let process = self.clone();
        let command = identity.clone();
        let request_id = id.to_owned();
        let request_actor = actor.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.submit_command(&request_id, req, &request_actor, command, executor).await
                })
            })
            .await;
        match result {
            Err(error) if error.command_may_have_committed() => {
                self.recover_sales_submission(id, actor, &identity, error).await
            },
            result => result,
        }
    }

    /// 先授权读取原单和回执，再在同一事务准备并保存提交。
    async fn submit_command(
        &self,
        id: &str,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
        identity: SubmissionIdentity,
        executor: &mut dyn Executor,
    ) -> Result<SubmissionView> {
        let access = self.command_access(actor, sales_order_permissions::SUBMIT);
        let order = access.current(id, executor).await?;
        if let Some(view) = replay_submission_with_executor(
            &self.db,
            &identity.audit_id,
            (&identity.fingerprint, &identity.key_hash),
            id,
            actor.id(),
            &access,
            executor,
        )
        .await?
        {
            return Ok(view);
        }
        let draft = self
            .prepare_sales_edit(
                &access,
                order,
                (req.contract_id.clone(), req.customer_id.clone(), req.draft.clone()),
                actor,
                true,
                executor,
            )
            .await?;
        self.submit_prepared_draft(id, req, actor, draft, identity, executor).await
    }

    /// 原读取事务确认关系后，完成目录预检、冻结副本及现有提交写入流程。
    async fn submit_prepared_draft(
        &self,
        id: &str,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
        draft: (SalesOrder, SalesOrderDraftRequest),
        identity: SubmissionIdentity,
        executor: &mut dyn Executor,
    ) -> Result<SubmissionView> {
        let resubmission_reason = req.resubmission_reason.clone();
        let PreparedSubmissionCopy { order, mut working_copy, copy_lines, working_copy_plan } =
            self.prepare_submission_copy(req, actor, draft, executor).await?;
        if let Some(existing) = self.submission_for_copy(&working_copy, executor).await? {
            return Ok(existing);
        }
        let sellable = SalesOrderService::sellable_working_copy_refs(&copy_lines)?;
        self.sales().ensure_sellable_refs(&sellable, &self.catalog(), executor).await?;
        self.ensure_procurement_responsibility_before_submit(&order, &copy_lines, executor).await?;
        let submission_no = self.next_submission_no(&order, executor).await?;
        let submission = build_submission(&working_copy, &copy_lines, submission_no, actor)?;
        let submission_lines = build_submission_lines(&submission, &copy_lines)?;
        working_copy.submit()?;
        self.start_approval_submission(
            ApprovalSubmissionStart {
                resubmission_reason,
                id,
                order,
                working_copy,
                submission,
                submission_lines,
                copy_lines,
                working_copy_plan,
            },
            actor,
            &identity.idempotency_key,
            identity.audit_id,
            identity.fingerprint,
            executor,
        )
        .await
    }

    /// 从最新冻结提交递增序号，保留原仓储读取及溢出校验。
    async fn next_submission_no(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<u32> {
        let order_id = SalesOrderId::new(order.base.id.clone());
        let latest = self
            .db
            .sales_order_submissions()
            .find_latest_by_order(&order_id, executor)
            .await?
            .map(|submission| submission.submission_no)
            .unwrap_or(0);
        SalesOrderSubmission::next_submission_no(latest).map_err(Error::from)
    }

    /// 按原读取顺序加载并准备首次提交工作副本，不执行写入。
    async fn prepare_submission_copy(
        &self,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
        draft: (SalesOrder, SalesOrderDraftRequest),
        executor: &mut dyn Executor,
    ) -> Result<PreparedSubmissionCopy> {
        let (order, draft) = draft;
        let refs = draft
            .lines
            .iter()
            .filter_map(|line| line.goods.as_ref())
            .map(|goods| (goods.sku_id.to_string(), goods.sku_revision_id.to_string()))
            .collect::<Vec<_>>();
        self.sales().ensure_sellable_refs(&refs, &self.catalog(), executor).await?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let active_working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, executor)
            .await?;
        let stable = self.sales().collect_stable_lines_for_draft(&order_id, &draft.lines, executor).await?;
        let (working_copy, copy_lines, working_copy_plan) = self
            .sales()
            .prepare_submission_copy(
                &order,
                active_working_copy,
                stable,
                &draft,
                req.version,
                actor,
                executor,
            )
            .await?;
        Ok(PreparedSubmissionCopy { order, working_copy, copy_lines, working_copy_plan })
    }

    /// 按原读取顺序回读工作副本已冻结的提交头与原行。
    async fn submission_for_copy(
        &self,
        working_copy: &SalesOrderWorkingCopy,
        executor: &mut dyn Executor,
    ) -> Result<Option<SubmissionView>> {
        let existing = self
            .db
            .sales_order_submissions()
            .find_by_working_copy(&SalesOrderWorkingCopyId::new(working_copy.base.id.clone()), executor)
            .await?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        let existing_id = SalesOrderSubmissionId::new(existing.base.id.clone());
        let existing_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&[existing_id], executor)
            .await?;
        Ok(Some(submission_view(existing, existing_lines)))
    }

    /// 销售单提交并启动统一审批。
    ///
    /// # 用途
    /// 冻结提交快照并启动统一审批。
    ///
    /// # 参数
    /// * `start` - 销售单、工作副本与提交快照
    /// * `actor` - 审计操作人
    /// * `idempotency_key` - 客户端幂等键
    /// * `audit_id` - 幂等审计主键
    /// * `fingerprint` - 请求摘要
    ///
    /// # 返回
    /// 返回提交快照视图。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    async fn start_approval_submission(
        &self,
        start: ApprovalSubmissionStart<'_>,
        actor: &AuditActor,
        idempotency_key: &str,
        audit_id: String,
        fingerprint: String,
        executor: &mut dyn Executor,
    ) -> Result<SubmissionView> {
        let PreparedSalesSubmission { access, expected_order_version, input } = self
            .prepare_sales_submission(start, actor, idempotency_key, &audit_id, &fingerprint, executor)
            .await?;
        persist_sales_order_start(&self.db, access, expected_order_version, input, executor).await
    }

    /// 按既有顺序准备领域状态、审批输入、审计和商品引用，不在此开启事务。
    async fn prepare_sales_submission(
        &self,
        mut start: ApprovalSubmissionStart<'_>,
        actor: &AuditActor,
        idempotency_key: &str,
        audit_id: &str,
        fingerprint: &str,
        executor: &mut dyn Executor,
    ) -> Result<PreparedSalesSubmission> {
        let context = self.prepare_submission_domain(&mut start, actor, idempotency_key, executor).await?;
        let prepared =
            self.prepare_submission_runtime(&start, &context, actor, idempotency_key, executor).await?;
        let workflow_action = submission_workflow_action(start.id, actor, start.resubmission_reason.clone())?;
        let audit = submission_event(
            audit_id.to_string(),
            actor,
            idempotency_key,
            fingerprint.to_string(),
            &start.order,
            &start.submission,
        )?;
        let sellable_refs = SalesOrderService::sellable_working_copy_refs(&start.copy_lines)?;
        SalesOrderService::new(self.db.clone())
            .ensure_sellable_refs(
                &sellable_refs,
                &CatalogQualificationAdapter::new(self.services.catalog.clone()),
                executor,
            )
            .await?;
        Ok(PreparedSalesSubmission {
            access: context.access,
            expected_order_version: context.expected_order_version,
            input: SalesOrderStartPersistInput {
                identity: self.authorization.clone(),
                resubmission_reason: start.resubmission_reason,
                order: start.order,
                working_copy: start.working_copy,
                submission: start.submission,
                submission_lines: start.submission_lines,
                workflow_action,
                document_type: context.ports.document_type,
                snapshot_payload: context.snapshot,
                prepared,
                owner_role: context.ports.owner_role,
                organization_id: context.organization_id,
                now: context.now,
                audit,
                working_copy_plan: start.working_copy_plan,
                sellable_refs,
            },
        })
    }

    /// 保持授权、绑定、状态迁移、快照和责任组织的首错顺序。
    async fn prepare_submission_domain(
        &self,
        start: &mut ApprovalSubmissionStart<'_>,
        actor: &AuditActor,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSubmissionApprovalContext> {
        let access = self.command_access(actor, sales_order_permissions::SUBMIT);
        let expected_order_version = start.order.base.version;
        let ports = sales_approval_ports(start.order.business_type)?;
        let subject = subject_ref_for_sales_business(start.order.business_type, start.id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let binding = find_approval_binding(&self.db, start.id, executor).await.map_err(Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        execute_sales_order_domain_action(&mut start.order, ports.on_approval_start, actor.id())?;
        let now = Instant::now();
        let snapshot = build_sales_order_snapshot(
            &start.order,
            &start.submission,
            &start.submission_lines,
            actor.id(),
            now,
        )?;
        let command = sales_order_start_command(
            ports.document_type,
            start.id,
            start.submission.submission_no,
            actor.id(),
            idempotency_key,
        );
        ensure_unified_start_command(&command)?;

        let organization_id = sales_order_responsible_org_id(&start.order)?;
        Ok(SalesSubmissionApprovalContext {
            access,
            expected_order_version,
            ports,
            subject,
            binding,
            now,
            snapshot,
            organization_id,
        })
    }

    /// 从绑定定义和原收据解析人员后生成启动计划；重放语义继续由审批引擎负责。
    async fn prepare_submission_runtime(
        &self,
        start: &ApprovalSubmissionStart<'_>,
        context: &SalesSubmissionApprovalContext,
        actor: &AuditActor,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<PreparedExecution> {
        let graph = load_bound_definition_graph_with_executor(&self.db, &context.binding, executor).await?;
        let receipt = load_start_receipt(
            &self.db,
            context.ports.document_type,
            &context.subject,
            start.submission.submission_no,
            idempotency_key,
            executor,
        )
        .await?;
        let graph = resolve_start_approvers(
            &self.db,
            &self.authorization,
            graph,
            actor.id(),
            receipt.as_ref(),
            executor,
        )
        .await?;
        let input = build_sales_order_start_input(SalesOrderStartInput {
            resubmission_reason: start.resubmission_reason.clone(),
            graph,
            binding: &context.binding,
            document_type: context.ports.document_type,
            subject: context.subject.clone(),
            subject_version: start.submission.submission_no,
            actor_id: actor.id(),
            organization_id: &context.organization_id,
            idempotency_key,
            receipt,
            now: context.now,
        })?;
        prepare_start(input).map_err(Error::from)
    }

    /// 从独立回执恢复原提交快照及行，再重验当前提交访问资格。
    async fn replay_sales_submission(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        expected_key_hash: &CommandFingerprint,
        sales_order_id: &str,
        actor: &AuditActor,
    ) -> Result<Option<SubmissionView>> {
        let db = self.db.clone();
        let command_id = audit_id.to_string();
        let fingerprint = expected_fingerprint.to_string();
        let expected_key_hash = expected_key_hash.clone();
        let sales_order_id = sales_order_id.to_string();
        let actor_id = actor.id().to_string();
        let access = self.command_access(actor, sales_order_permissions::SUBMIT);
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replay_submission_with_executor(
                        &db,
                        &command_id,
                        (&fingerprint, &expected_key_hash),
                        &sales_order_id,
                        &actor_id,
                        &access,
                        executor,
                    )
                    .await
                })
            })
            .await
    }

    /// 未知提交只查证同一完整命令，不重跑提交准备或写入。
    async fn recover_sales_submission(
        &self,
        id: &str,
        actor: &AuditActor,
        identity: &SubmissionIdentity,
        original_error: Error,
    ) -> Result<SubmissionView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            match self
                .replay_sales_submission(
                    &identity.audit_id,
                    &identity.fingerprint,
                    &identity.key_hash,
                    id,
                    actor,
                )
                .await
            {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(error) => return finish_receipt_recovery(original_error, Err(error)),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
}

/// 构造原销售单与精确提交快照关联的单个成功事件。
fn submission_event(
    command_id: String,
    actor: &AuditActor,
    idempotency_key: &str,
    fingerprint: String,
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
) -> Result<SalesCommandEvent> {
    SalesCommandEvent::new(
        command_id,
        actor,
        idempotency_key,
        fingerprint,
        SalesCommandResult::Submitted {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            submission_id: SalesOrderSubmissionId::new(submission.base.id.clone()),
        },
        order.order_no.clone(),
    )
}

/// 创建与本次提交同事务保存的状态变更记录。
fn submission_workflow_action(
    id: &str,
    actor: &AuditActor,
    resubmission_reason: Option<String>,
) -> Result<WorkflowAction> {
    Ok(WorkflowAction::new(
        WorkflowActionId::new(next_id()),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(id.to_string()),
            action_type: WorkflowActionType::Submit,
            from_status: "DRAFT".to_string(),
            to_status: "PENDING_REVIEW".to_string(),
            actor_id: actor.id().to_string(),
            actor_role: "role-sales".to_string(),
            comment: resubmission_reason,
        },
    )?)
}

#[cfg(test)]
mod identity_tests {
    use erp_core::AccountKind;
    use serde_json::json;

    use super::*;

    #[test]
    fn submission_identity_keeps_the_original_payload_for_replay_after_preflight() {
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let mut request: SubmitSalesOrderRequest = serde_json::from_value(json!({
            "version": 1, "idempotency_key": " request-key ",
            "draft": {"editor_user_id": "actor", "lines": []}
        }))
        .unwrap();
        let identity = SubmissionIdentity::new("sales-1", &request, &actor).unwrap();
        assert_eq!(identity.idempotency_key, "request-key");
        assert_eq!(request.idempotency_key, " request-key ");
        assert_eq!(
            identity.fingerprint,
            sales_submission_fingerprint(actor.id(), "sales-1", &request).unwrap()
        );
        let prepared = request.clone();
        assert_eq!(
            identity.fingerprint,
            sales_submission_fingerprint(actor.id(), "sales-1", &prepared).unwrap()
        );
        request.idempotency_key = " ".into();
        assert!(matches!(
            SubmissionIdentity::new("sales-1", &request, &actor),
            Err(Error::ValidationError(_))
        ));
    }
}
