use application_core::{AuditActor, CommandFingerprint};
use bpm::model::SubjectRef;
use erp_core::common::time::Instant;
use erp_core::ids::{
    BusinessDocumentId, SalesOrderId, SalesOrderSubmissionId, SalesOrderWorkingCopyId, WorkflowActionId,
};
use erp_sales::dto::sales_order::{SubmissionView, SubmitSalesOrderRequest};
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
use erp_workflow::entity::document_registry::{
    DocumentType, WorkflowAction, WorkflowActionData, WorkflowActionType,
};
use erp_workflow::service::approval::execution::{PreparedExecution, command_recovery_delay, prepare_start};
use erp_workflow::service::document_registry::find_approval_binding;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::adapter::{
    RECENT_HISTORY_LIMIT, SalesApprovalPorts, SalesOrderStartCommand, build_sales_order_snapshot,
    execute_sales_order_domain_action, reject_legacy_card_sales_decision, reject_legacy_card_sales_work_item,
    require_frozen_binding, sales_approval_ports, sales_order_object_readable,
    sales_order_responsible_org_id, sales_order_start_command, start_approval_command_kind,
};
use super::super::adapters::catalog::CatalogQualificationAdapter;
use super::super::authorization::SalesCommandAccess;
use super::super::start_approval::{
    ReplaySalesOrderStartInput, SalesOrderStartInput, SalesOrderStartPersistInput,
    SalesOrderWorkingCopyPersistPlan, build_sales_order_start_input, load_bound_definition_graph,
    load_start_receipt, persist_sales_order_start, replay_sales_order_start_with_executor,
};
use crate::adapters::workflow::approver_directory::resolve_start_approvers;
use crate::order_to_cash::command_event::{
    SalesCommandEvent, finish_receipt_recovery, replay_submission_with_executor,
};
use crate::order_to_cash::subject_ref_for_sales_business;
use crate::{Error, Result};

/// 销售提交启动恢复入参。
struct RecoverSalesSubmissionStartInput<'a> {
    /// 销售单主键。
    sales_order_id: &'a str,
    /// 提交时冻结的单据类型。
    document_type: DocumentType,
    /// 提交时冻结的审批主题版本。
    subject_version: u32,
    /// 提交幂等键。
    idempotency_key: &'a str,
    /// 提交人。
    actor: &'a AuditActor,
    /// 提交审计收据 ID。
    audit_id: &'a str,
    /// 命令载荷指纹。
    fingerprint: &'a str,
    /// 触发恢复的原始错误。
    original_error: Error,
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

/// 读取销售单最新提交号，作为撤回查找实例的 `subject_version`。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `sales_order_id` - 销售单主键。
///
/// # 返回
/// 返回最新提交的 `submission_no`。
///
/// # 错误
/// 没有提交时返回冲突；仓储读取失败时返回对应错误。
pub(super) async fn latest_submission_no(db: &mongodb::Database, sales_order_id: &str) -> Result<u32> {
    db.sales_order_submissions()
        .find_latest_by_order(&SalesOrderId::new(sales_order_id), &mut NoTransaction)
        .await?
        .map(|submission| submission.submission_no)
        .ok_or_else(|| Error::ConflictError("销售单没有可撤回的提交版本".to_string()))
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
        let access = self.command_access(actor, "submit")?;
        let authorized_order = access.current(id, &mut NoTransaction).await?;
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        let audit_id = sales_submission_audit_id(actor.id(), id, &idempotency_key);
        let fingerprint = sales_submission_fingerprint(actor.id(), id, &req)?;
        let expected_key_hash = CommandFingerprint::from_parts([idempotency_key.clone()]);
        if let Some(existing) =
            self.replay_sales_submission(&audit_id, &fingerprint, &expected_key_hash, id, actor).await?
        {
            return Ok(existing);
        }
        let PreparedSubmissionCopy { order, mut working_copy, copy_lines, working_copy_plan } =
            self.prepare_submission_copy(req, actor, &access, authorized_order).await?;
        if let Some(existing) = self.submission_for_copy(&working_copy).await? {
            return Ok(existing);
        }
        self.sales().ensure_sellable_working_copy_lines(&copy_lines, &self.catalog()).await?;
        self.ensure_procurement_responsibility_before_submit(&order, &copy_lines).await?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let latest_submission_no = self
            .db
            .sales_order_submissions()
            .find_latest_by_order(&order_id, &mut NoTransaction)
            .await?
            .map(|submission| submission.submission_no)
            .unwrap_or(0);
        let submission_no = SalesOrderSubmission::next_submission_no(latest_submission_no)?;
        let submission = build_submission(&working_copy, &copy_lines, submission_no, actor)?;
        let submission_lines = build_submission_lines(&submission, &copy_lines)?;
        working_copy.submit()?;
        self.start_approval_submission(
            ApprovalSubmissionStart {
                id,
                order,
                working_copy,
                submission,
                submission_lines,
                copy_lines,
                working_copy_plan,
            },
            actor,
            &idempotency_key,
            audit_id,
            fingerprint,
        )
        .await
    }

    /// 按原读取顺序加载并准备首次提交工作副本，不执行写入。
    async fn prepare_submission_copy(
        &self,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
        access: &SalesCommandAccess,
        authorized_order: SalesOrder,
    ) -> Result<PreparedSubmissionCopy> {
        let (order, draft) = self
            .prepare_sales_edit(
                access,
                authorized_order,
                (req.contract_id, req.customer_id),
                req.draft,
                actor,
                true,
            )
            .await?;
        self.sales().ensure_sellable_draft_lines(&draft.lines, &self.catalog()).await?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let active_working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?;
        let stable = self.sales().collect_stable_lines_for_draft(&order_id, &draft.lines).await?;
        let (working_copy, copy_lines, working_copy_plan) = self
            .sales()
            .prepare_submission_copy(&order, active_working_copy, stable, &draft, req.version, actor)
            .await?;
        Ok(PreparedSubmissionCopy { order, working_copy, copy_lines, working_copy_plan })
    }

    /// 按原读取顺序回读工作副本已冻结的提交头与原行。
    async fn submission_for_copy(
        &self,
        working_copy: &SalesOrderWorkingCopy,
    ) -> Result<Option<SubmissionView>> {
        let existing = self
            .db
            .sales_order_submissions()
            .find_by_working_copy(
                &SalesOrderWorkingCopyId::new(working_copy.base.id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        let existing_id = SalesOrderSubmissionId::new(existing.base.id.clone());
        let existing_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&[existing_id], &mut NoTransaction)
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
    ) -> Result<SubmissionView> {
        let id = start.id;
        let PreparedSalesSubmission { access, expected_order_version, input } =
            self.prepare_sales_submission(start, actor, idempotency_key, &audit_id, &fingerprint).await?;
        let document_type = input.document_type;
        let subject_version = input.submission.submission_no;
        match persist_sales_order_start(&self.db, access, expected_order_version, input).await {
            Ok(view) => Ok(view),
            Err(error) if error.command_may_have_committed() => {
                self.recover_sales_submission_start(RecoverSalesSubmissionStartInput {
                    sales_order_id: id,
                    document_type,
                    subject_version,
                    idempotency_key,
                    actor,
                    audit_id: &audit_id,
                    fingerprint: &fingerprint,
                    original_error: error,
                })
                .await
            },
            Err(error) => Err(error),
        }
    }

    /// 按既有顺序准备领域状态、审批输入、审计和商品引用，不在此开启事务。
    async fn prepare_sales_submission(
        &self,
        mut start: ApprovalSubmissionStart<'_>,
        actor: &AuditActor,
        idempotency_key: &str,
        audit_id: &str,
        fingerprint: &str,
    ) -> Result<PreparedSalesSubmission> {
        let context = self.prepare_submission_domain(&mut start, actor, idempotency_key).await?;
        let prepared = self.prepare_submission_runtime(&start, &context, actor, idempotency_key).await?;
        let workflow_action = submission_workflow_action(start.id, actor)?;
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
                &CatalogQualificationAdapter::new(self.db.clone()),
                &mut NoTransaction,
            )
            .await?;
        Ok(PreparedSalesSubmission {
            access: context.access,
            expected_order_version: context.expected_order_version,
            input: SalesOrderStartPersistInput {
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
    ) -> Result<SalesSubmissionApprovalContext> {
        let access = self.command_access(actor, "submit")?;
        let expected_order_version = start.order.base.version;
        let ports = sales_approval_ports(start.order.business_type)?;
        let subject = subject_ref_for_sales_business(start.order.business_type, start.id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let binding =
            find_approval_binding(&self.db, start.id, &mut NoTransaction).await.map_err(Error::from)?;
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
        let _ = (start_approval_command_kind(&command), RECENT_HISTORY_LIMIT);
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
    ) -> Result<PreparedExecution> {
        let graph = load_bound_definition_graph(&self.db, &context.binding).await?;
        let receipt = load_start_receipt(
            &self.db,
            context.ports.document_type,
            &context.subject,
            start.submission.submission_no,
            idempotency_key,
        )
        .await?;
        let graph =
            resolve_start_approvers(&self.db, graph, actor.id(), receipt.as_ref(), &mut NoTransaction)
                .await?;
        let input = build_sales_order_start_input(SalesOrderStartInput {
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
        let access = self.command_access(actor, "submit")?;
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

    /// 每次恢复使用独立只读事务，先验证启动收据，再回读精确提交回执。
    async fn recover_submission_attempt(
        &self,
        input: &RecoverSalesSubmissionStartInput<'_>,
    ) -> Result<Option<SubmissionView>> {
        let db = self.db.clone();
        let recovery = SalesStartRecovery {
            order_id: input.sales_order_id.to_string(),
            idempotency_key: input.idempotency_key.to_string(),
            actor_id: input.actor.id().to_string(),
            document_type: input.document_type,
            subject_version: input.subject_version,
        };
        let access = self.command_access(input.actor, "submit")?;
        let started = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { recovery.replay(&db, &access, executor).await })
            })
            .await?;
        if started.is_none() {
            return Ok(None);
        }
        self.replay_sales_submission(
            input.audit_id,
            input.fingerprint,
            &CommandFingerprint::from_parts([input.idempotency_key.trim().to_string()]),
            input.sales_order_id,
            input.actor,
        )
        .await
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    ///
    /// # 参数
    /// * `input` - 销售提交恢复所需的单据类型、主题版本与审计身份
    ///
    /// # 返回
    /// 回读到已提交结果时返回提交视图；否则返回原始错误。
    ///
    /// # 错误
    /// 有界回读仍无法确认提交结果时返回原始错误，或传播不可恢复的冲突。
    ///
    /// # 关键业务约束
    /// 仅在 `command_may_have_committed` 场景进入；不得在确认未提交时吞掉错误。
    async fn recover_sales_submission_start(
        &self,
        input: RecoverSalesSubmissionStartInput<'_>,
    ) -> Result<SubmissionView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let recovered = self.recover_submission_attempt(&input).await;
            match recovered {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(error) => return finish_receipt_recovery(input.original_error, Err(error)),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(input.original_error)
    }
}

/// 在原恢复读取位置加载并验证冻结审批绑定。
async fn load_frozen_binding(
    db: &Database,
    order_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    let binding = find_approval_binding(db, order_id, executor).await.map_err(Error::from)?;
    Ok(require_frozen_binding(binding.as_ref())?.clone())
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
fn submission_workflow_action(id: &str, actor: &AuditActor) -> Result<WorkflowAction> {
    Ok(WorkflowAction::new(
        WorkflowActionId::new(next_id()),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(id.to_string()),
            action_type: WorkflowActionType::Submit,
            from_status: "DRAFT".to_string(),
            to_status: "PENDING_REVIEW".to_string(),
            actor_id: actor.id().to_string(),
            actor_role: "role-sales".to_string(),
            comment: None,
        },
    )?)
}

/// 单次恢复所查证的冻结启动身份。
struct SalesStartRecovery {
    order_id: String,
    idempotency_key: String,
    actor_id: String,
    document_type: DocumentType,
    subject_version: u32,
}

impl SalesStartRecovery {
    async fn replay(
        &self,
        db: &Database,
        access: &SalesCommandAccess,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let order = access.current(&self.order_id, executor).await?;
        let current_ports = sales_approval_ports(order.business_type)?;
        if current_ports.document_type != self.document_type {
            return Err(Error::ConflictError("销售单业务类型在提交恢复期间已变化".to_string()));
        }
        let organization_id = sales_order_responsible_org_id(&order)?;
        let _ = sales_order_object_readable(&organization_id, &self.actor_id)?;
        let binding = load_frozen_binding(db, &self.order_id, executor).await?;
        let subject = subject_ref_for_sales_business(order.business_type, &self.order_id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        replay_sales_order_start_with_executor(
            db,
            ReplaySalesOrderStartInput {
                document_type: self.document_type,
                subject: &subject,
                subject_version: self.subject_version,
                idempotency_key: &self.idempotency_key,
                binding: &binding,
                actor_id: &self.actor_id,
            },
            executor,
        )
        .await
    }
}
