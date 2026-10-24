use application_core::AuditActor;
use bpm::SubjectRef;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_inventory::{
    StockAdjustment, StockAdjustmentApprovalSnapshot, StockAdjustmentDetailView, StockAdjustmentLine,
    StockAdjustmentSubmitResultQuery, StockAdjustmentUpdate, SubmitStockAdjustmentRequest,
};
use erp_read_models::inventory_center::{RECENT_HISTORY_LIMIT, load_approval_binding};
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::{PlannedWrites, PreparedExecution};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::adapter::{
    StockAdjustmentAdapter, execute_stock_adjustment_domain_action, require_frozen_binding,
    start_approval_command_kind, stock_adjustment_adapter, stock_adjustment_start_command,
    stock_adjustment_subject_ref, workflow_snapshot_from_inventory,
};
use super::mapping::build_adjustment_line_updates;
use super::{InventoryAdjustmentService, approval_prepare as start_approval, persist as start_persist};
use crate::adapters::workflow::approver_directory::resolve_approvers;
use crate::{Error, Result};

/// 领域草稿与冻结提交事实；审批计划准备完成前不写入数据库。
struct StockAdjustmentSubmission {
    adjustment: StockAdjustment,
    lines: Vec<StockAdjustmentLine>,
    binding: ApprovalDefinitionBinding,
    adapter: StockAdjustmentAdapter,
    subject: SubjectRef,
    snapshot: ApprovalSubjectSnapshotPayload,
    organization_id: String,
    now: Instant,
}

impl StockAdjustmentSubmission {
    fn into_persist_input(
        self,
        id: &str,
        req: &SubmitStockAdjustmentRequest,
        actor: &AuditActor,
        rbac: SharedRbacService,
        writes: PlannedWrites,
    ) -> start_persist::StockAdjustmentStartPersistInput {
        start_persist::StockAdjustmentStartPersistInput {
            rbac,
            adjustment: self.adjustment,
            actor: actor.clone(),
            id: id.to_string(),
            snapshot_payload: self.snapshot,
            writes,
            binding: self.binding,
            owner_role: self.adapter.owner_role,
            organization_id: self.organization_id,
            now: self.now,
            lines: self.lines,
            balances: req.balances.clone(),
            expected_document_version: req.expected_version,
            expected_subject_version: req.expected_subject_version,
        }
    }
}

impl InventoryAdjustmentService {
    /// 提交库存调整并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `req` - 最终草稿、余额版本与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的完整详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    #[tracing::instrument(
        name = "inventory.stock_adjustment_submit",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_submit")
    )]
    pub async fn submit_stock_adjustment(
        &self,
        id: &str,
        req: SubmitStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        req.validate()?;
        if let Some(view) = committed_stock_adjustment_start_replay(self, id, &req, actor).await? {
            return Ok(view);
        }
        let context = self.prepare_adjustment_submission(id, &req, actor).await?;
        let writes = self.prepare_adjustment_runtime(&context, &req, actor).await?;
        let result_instance_id = writes.instance.base.id.clone();
        let input = context.into_persist_input(id, &req, actor, self.rbac.clone(), writes);
        let persist_result = start_persist::persist_stock_adjustment_start(&self.db, input).await;
        if let Err(error) = persist_result {
            if let Some(view) = recover_stock_adjustment_start_replay(self, id, &req, actor).await? {
                return Ok(view);
            }
            return Err(error);
        }
        self.read_model()
            .stock_adjustment_detail_for_instance(
                id,
                actor,
                Some((&result_instance_id, req.expected_subject_version)),
            )
            .await
            .map_err(Error::from)
    }

    /// 先加载并校验草稿，再冻结提交展示事实；不执行持久化。
    async fn prepare_adjustment_submission(
        &self,
        id: &str,
        req: &SubmitStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentSubmission> {
        let adapter = stock_adjustment_adapter()?;
        let subject = stock_adjustment_subject_ref(id)?;
        let (mut adjustment, binding, lines) = self.load_adjustment_submission_draft(id, req, actor).await?;
        execute_stock_adjustment_domain_action(&mut adjustment, adapter.on_approval_start)?;
        let now = Instant::now();
        let snapshot = workflow_snapshot_from_inventory(StockAdjustmentApprovalSnapshot::build(
            &adjustment,
            &lines,
            actor.id(),
            now,
        )?);
        let start = stock_adjustment_start_command(
            id,
            adjustment.approval_subject_version,
            actor.id(),
            &req.idempotency_key,
        );
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = adjustment.warehouse_id.to_string();
        Ok(StockAdjustmentSubmission {
            adjustment,
            lines,
            binding,
            adapter,
            subject,
            snapshot,
            organization_id,
            now,
        })
    }

    /// 保持授权、文档版本、主题版本、字段更新、绑定和明细的原校验顺序。
    async fn load_adjustment_submission_draft(
        &self,
        id: &str,
        req: &SubmitStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<(StockAdjustment, ApprovalDefinitionBinding, Vec<StockAdjustmentLine>)> {
        let mut adjustment = self.inventory().load_stock_adjustment(id).await?;
        start_approval::ensure_stock_adjustment_submit_authorized_with_executor(
            &self.db,
            &self.rbac,
            &adjustment,
            actor,
            &mut NoTransaction,
        )
        .await?;
        if !adjustment.matches_version(req.expected_version) {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        let target_subject_version = adjustment
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("库存调整审批主题版本已达上限".to_string()))?;
        if req.expected_subject_version != target_subject_version {
            return Err(Error::ConflictError("库存调整审批主题版本已变化，请刷新后重试".to_string()));
        }
        adjustment.update(StockAdjustmentUpdate {
            reason_type: Some(req.reason_type),
            reviewed_by: None,
            finance_reviewed_by: None,
            note: Some(req.note.clone()),
            occurred_at: Some(Instant::from_unix_secs(req.occurred_at)),
        })?;
        let binding = load_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let mut lines = self.inventory().load_adjustment_lines(id, &mut NoTransaction).await?;
        let line_updates = build_adjustment_line_updates(&req.lines)?;
        adjustment.apply_line_updates(&mut lines, &line_updates, true)?;
        Ok((adjustment, binding, lines))
    }

    /// 用冻结绑定和人员解析结果准备审批运行态，拒绝新提交进入回放分支。
    async fn prepare_adjustment_runtime(
        &self,
        context: &StockAdjustmentSubmission,
        req: &SubmitStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<PlannedWrites> {
        let graph = start_approval::load_bound_definition_graph(&self.db, &context.binding).await?;
        let graph = resolve_approvers(&self.db, graph, actor.id(), &mut NoTransaction).await?;
        let input = start_approval::build_stock_adjustment_start_input(
            &self.db,
            &self.rbac,
            start_approval::StockAdjustmentStartInput {
                graph,
                binding: &context.binding,
                subject: context.subject.clone(),
                subject_version: context.adjustment.approval_subject_version,
                actor_id: actor.id(),
                organization_id: &context.organization_id,
                idempotency_key: &req.idempotency_key,
                receipt: None,
                now: context.now,
            },
            &mut NoTransaction,
        )
        .await?;
        let PreparedExecution::Apply(writes) = start_approval::prepare_stock_adjustment_start(input, req)?
        else {
            return Err(Error::Internal("新库存调整提交不得进入回放分支".to_string()));
        };
        Ok(*writes)
    }

    /// 按原 StartApproval 作用域和幂等键查询已提交的库存调整结果。
    ///
    /// 本端口只承认精确存在且能通过 receipt → instance → snapshot → frozen
    /// binding → actor/current scope 验证的收据；不存在或错 key 返回 `NotFound`，
    /// 不得依据当前单据状态推断原命令成功。
    ///
    /// # 参数
    /// * `id` - 调整单主键。
    /// * `query` - 期望主题版本与幂等键。
    /// * `actor` - 当前认证操作人。
    ///
    /// # 返回
    /// 返回原命令实例对应的当前可读详情。
    ///
    /// # 错误
    /// 查询参数非法、事务内解析失败、结果不存在，或详情读取失败时返回对应错误。
    pub async fn stock_adjustment_submit_result(
        &self,
        id: &str,
        query: StockAdjustmentSubmitResultQuery,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        query.validate()?;
        let detail_id = id.to_string();
        let detail_actor = actor.clone();
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_string();
        let actor = actor.clone();
        let query_for_read = query.clone();
        let client = db.client().clone();
        let result_ref = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    start_approval::find_stock_adjustment_start_result(
                        &db,
                        &rbac,
                        &id,
                        query_for_read.expected_subject_version,
                        &query_for_read.idempotency_key,
                        &actor,
                        executor,
                    )
                    .await
                })
            })
            .await?;
        let result_ref = result_ref.ok_or_else(|| Error::NotFound("库存调整提交结果不存在".to_string()))?;
        self.read_model()
            .stock_adjustment_detail_for_instance(
                &detail_id,
                &detail_actor,
                Some((&result_ref, query.expected_subject_version)),
            )
            .await
            .map_err(Error::from)
    }
}

const START_REPLAY_RECOVERY_ATTEMPTS: usize = 32;

/// 使用独立只读事务先解析启动收据，再返回当前可读详情。
async fn committed_stock_adjustment_start_replay(
    service: &InventoryAdjustmentService,
    id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentDetailView>> {
    let db = service.db.clone();
    let rbac = service.rbac.clone();
    let id_owned = id.to_string();
    let req_owned = req.clone();
    let actor_owned = actor.clone();
    let client = db.client().clone();
    let result_ref = client
        .with_transaction(move |executor| {
            Box::pin(async move {
                start_approval::reconcile_stock_adjustment_start_receipt(
                    &db,
                    &rbac,
                    &id_owned,
                    &req_owned,
                    &actor_owned,
                    executor,
                )
                .await
            })
        })
        .await?;
    let Some(result_ref) = result_ref else {
        return Ok(None);
    };
    Ok(Some(
        service
            .read_model()
            .stock_adjustment_detail_for_instance(
                id,
                actor,
                Some((&result_ref, req.expected_subject_version)),
            )
            .await?,
    ))
}

/// 事务失败或结果未知后，以有限次新会话等待并发 winner 的收据可见。
async fn recover_stock_adjustment_start_replay(
    service: &InventoryAdjustmentService,
    id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentDetailView>> {
    for _ in 0..START_REPLAY_RECOVERY_ATTEMPTS {
        if let Some(view) = committed_stock_adjustment_start_replay(service, id, req, actor).await? {
            return Ok(Some(view));
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Ok(None)
}
