use std::sync::Arc;

use application_core::AuditActor;
use bpm::SubjectRef;
use erp_approval_runtime::approver_directory::resolve_approvers;
use erp_core::common::time::Instant;
use erp_core::ids::StockAdjustmentId;
use erp_identity::AuthorizationService;
use erp_inventory::authorization::require_submit;
use erp_inventory::{
    InventoryExt, StockAdjustment, StockAdjustmentApprovalSnapshot, StockAdjustmentDetailView,
    StockAdjustmentLine, StockAdjustmentSubmitResultQuery, StockAdjustmentUpdate,
    SubmitStockAdjustmentRequest,
};
use erp_read_models::inventory_center::load_approval_binding;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::{PlannedWrites, PreparedExecution};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::adapter::{
    StockAdjustmentAdapter, execute_stock_adjustment_domain_action, require_frozen_binding,
    stock_adjustment_adapter, stock_adjustment_subject_ref, workflow_snapshot_from_inventory,
};
use super::mapping::build_adjustment_line_updates;
use super::{InventoryAdjustmentProcess, approval_prepare as start_approval, persist as start_persist};
use crate::workflow::FulfillmentWorkflow;
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
        identity: Arc<AuthorizationService>,
        workflow: Arc<dyn FulfillmentWorkflow>,
        writes: PlannedWrites,
    ) -> start_persist::StockAdjustmentStartPersistInput {
        start_persist::StockAdjustmentStartPersistInput {
            identity,
            resubmission_reason: req.resubmission_reason.clone(),
            workflow,
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

impl InventoryAdjustmentProcess {
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
        let process = Self::new(
            self.db.clone(),
            self.identity.clone(),
            self.workflow.clone(),
            self.inventory_service.clone(),
        );
        let (id_owned, request, writer) = (id.to_owned(), req.clone(), actor.clone());
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { process.submit_adjustment(&id_owned, &request, &writer, executor).await },
                )
            })
            .await;
        let result_instance_id = match result {
            Ok(instance_id) => instance_id,
            Err(error) => {
                if let Some(view) = recover_stock_adjustment_start_replay(self, id, &req, actor).await? {
                    return Ok(view);
                }
                return Err(error);
            },
        };
        self.read_model()
            .stock_adjustment_detail_for_instance(
                id,
                actor,
                Some((&result_instance_id, req.expected_subject_version)),
            )
            .await
            .map_err(Error::from)
    }

    /// 在原快照中完成提交授权、草稿读取、审批规划及持久化。
    async fn submit_adjustment(
        &self,
        id: &str,
        req: &SubmitStockAdjustmentRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let context = self.prepare_adjustment_submission(id, req, actor, executor).await?;
        let writes = self.prepare_adjustment_runtime(&context, req, actor, executor).await?;
        let instance_id = writes.instance.base.id.clone();
        let input =
            context.into_persist_input(id, req, actor, self.identity.clone(), self.workflow.clone(), writes);
        start_persist::persist_stock_adjustment_start(&self.db, input, executor).await?;
        Ok(instance_id)
    }

    /// 先加载并校验草稿，再冻结提交展示事实；不执行持久化。
    async fn prepare_adjustment_submission(
        &self,
        id: &str,
        req: &SubmitStockAdjustmentRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustmentSubmission> {
        let adapter = stock_adjustment_adapter()?;
        let subject = stock_adjustment_subject_ref(id)?;
        let (mut adjustment, binding, lines) =
            load_adjustment_submission_draft(&self.db, &self.identity, id, req, actor, executor).await?;
        execute_stock_adjustment_domain_action(&mut adjustment, adapter.on_approval_start)?;
        let now = Instant::now();
        let snapshot = workflow_snapshot_from_inventory(StockAdjustmentApprovalSnapshot::build(
            &adjustment,
            &lines,
            actor.id(),
            now,
        )?);
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

    /// 用冻结绑定和人员解析结果准备审批运行态，拒绝新提交进入回放分支。
    async fn prepare_adjustment_runtime(
        &self,
        context: &StockAdjustmentSubmission,
        req: &SubmitStockAdjustmentRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PlannedWrites> {
        let graph =
            start_approval::load_bound_definition_graph_with_executor(&self.db, &context.binding, executor)
                .await?;
        let graph = resolve_approvers(&self.db, &self.identity, graph, actor.id(), executor).await?;
        let input = start_approval::build_stock_adjustment_start_input(
            &self.db,
            self.workflow.as_ref(),
            start_approval::StockAdjustmentStartInput {
                resubmission_reason: req.resubmission_reason.clone(),
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
            executor,
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
        let workflow = self.workflow.clone();
        let id = id.to_string();
        let actor = actor.clone();
        let query_for_read = query.clone();
        let client = db.client().clone();
        let result_ref = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    start_approval::find_stock_adjustment_start_result(
                        &db,
                        workflow.as_ref(),
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

/// 在原快照先授权，再读取并校验本次提交草稿、绑定和明细。
async fn load_adjustment_submission_draft(
    db: &Database,
    identity: &AuthorizationService,
    id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(StockAdjustment, ApprovalDefinitionBinding, Vec<StockAdjustmentLine>)> {
    require_submit(identity, actor, id, executor).await?;
    let mut adjustment = db
        .inventory()
        .stock_adjustment(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".into()))?;
    if !adjustment.matches_version(req.expected_version) {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".into()));
    }
    let target_subject_version = adjustment
        .approval_subject_version
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("库存调整审批主题版本已达上限".into()))?;
    if req.expected_subject_version != target_subject_version {
        return Err(Error::ConflictError("库存调整审批主题版本已变化，请刷新后重试".into()));
    }
    adjustment.update(StockAdjustmentUpdate {
        reason_type: Some(req.reason_type),
        reviewed_by: None,
        finance_reviewed_by: None,
        note: Some(req.note.clone()),
        occurred_at: Some(Instant::from_unix_secs(req.occurred_at)),
    })?;
    let binding = load_approval_binding(db, id, executor).await?;
    let binding = require_frozen_binding(binding.as_ref())?.clone();
    let mut lines =
        db.inventory().adjustment_lines_by_adjustment_ids(&[StockAdjustmentId::new(id)], executor).await?;
    let updates = build_adjustment_line_updates(&req.lines)?;
    adjustment.apply_line_updates(&mut lines, &updates, true)?;
    Ok((adjustment, binding, lines))
}

const START_REPLAY_RECOVERY_ATTEMPTS: usize = 32;

/// 使用独立只读事务先解析启动收据，再返回当前可读详情。
async fn committed_stock_adjustment_start_replay(
    service: &InventoryAdjustmentProcess,
    id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentDetailView>> {
    let db = service.db.clone();
    let workflow = service.workflow.clone();
    let id_owned = id.to_string();
    let req_owned = req.clone();
    let actor_owned = actor.clone();
    let client = db.client().clone();
    let result_ref = client
        .with_transaction(move |executor| {
            Box::pin(async move {
                start_approval::reconcile_stock_adjustment_start_receipt(
                    &db,
                    workflow.as_ref(),
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
    service: &InventoryAdjustmentProcess,
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_inventory::AdjustmentReasonType;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 提交草稿必须使用快照，且资格失败先于表头、版本、绑定与明细读取。
    #[tokio::test]
    async fn submission_draft_authorizes_in_the_original_snapshot_before_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("submission_draft_unit");
        let identity = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let request = SubmitStockAdjustmentRequest {
            expected_version: 7,
            expected_subject_version: 2,
            reason_type: AdjustmentReasonType::StockGain,
            resubmission_reason: None,
            lines: Vec::new(),
            balances: Vec::new(),
            note: String::new(),
            occurred_at: 42,
            idempotency_key: "submit-1".into(),
        };
        let error = load_adjustment_submission_draft(
            &db,
            &identity,
            "adjustment",
            &request,
            &actor,
            &mut ReadExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务"));
        let error = load_adjustment_submission_draft(
            &db,
            &identity,
            "adjustment",
            &request,
            &actor,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
