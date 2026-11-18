//! 调查结果在同一执行器内应用、登记证据并更新任务与回执。

use std::future::Future;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::AuthorizationService;
use erp_supply::FulfillmentOrderAccess;
use erp_supply::dto::supplier_fulfillment::SupplierOrderInvestigationAction;
use erp_supply::entity::supplier_fulfillment::SupplierOrderAction;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_fulfillment::investigate::{
    InvestigationEvidenceRecord, PreparedInvestigation, apply_prepared_investigation, ensure_replay_safe,
    finish_evidence, parse_prepared_investigation, persist_action, persist_order,
    validate_investigation_intent,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use super::prepare::{authorize_investigation, load_investigation_facts};
use super::{
    InvestigationCommandContext, InvestigationCommitResult, InvestigationIdentity, InvestigationWrite,
    ensure_no_active_w26_task, persist_investigation_result, serialize_investigation_record,
};
use crate::workflow::FulfillmentWorkflow;
use crate::{Error, Result};

pub(super) struct InvestigationCommit {
    context: InvestigationCommandContext,
    identity: InvestigationIdentity,
    prepared: PreparedInvestigation,
    actor: AuditActor,
    workflow: Arc<dyn FulfillmentWorkflow>,
    authorization: Arc<AuthorizationService>,
    access: FulfillmentOrderAccess,
}

impl InvestigationCommit {
    /// 准备最终事务的不可变输入；构造过程不读取或写入数据库。
    ///
    /// # 参数
    /// * `workflow` - 原应用注入的任务事实及办理能力。
    /// * `authorization` - 原应用共享授权服务。
    /// * `access` - 同一授权服务绑定的订单范围能力。
    /// * `context` - 原命令及可选正式任务身份。
    /// * `identity` - 原命令与调查证据的稳定身份。
    /// * `prepared` - 已在事务外取得并冻结的结果。
    /// * `actor` - 当前操作人。
    /// # 返回
    /// 返回最终事务步骤的输入。
    /// # 错误
    /// 不返回错误。
    pub(super) fn new(
        workflow: Arc<dyn FulfillmentWorkflow>,
        authorization: Arc<AuthorizationService>,
        access: FulfillmentOrderAccess,
        context: &InvestigationCommandContext,
        identity: &InvestigationIdentity,
        prepared: PreparedInvestigation,
        actor: &AuditActor,
    ) -> Self {
        Self {
            context: context.clone(),
            identity: identity.clone(),
            prepared,
            actor: actor.clone(),
            workflow,
            authorization,
            access,
        }
    }

    /// 在入口事务的同一执行器上提交事实、调查证据、任务与回执。
    ///
    /// # 参数
    /// * `db` - 当前数据库。
    /// * `executor` - 入口传入的事务执行器。
    /// # 返回
    /// 返回提交后的订单、证据、结果正文及可选任务版本。
    /// # 错误
    /// 任一步失败立即停止并返回首个错误；不重新派发供应商动作。
    pub(super) async fn apply(
        self,
        db: &Database,
        executor: &mut dyn Executor,
    ) -> Result<InvestigationCommitResult> {
        let task = self.authorize(db, executor).await?;
        let (mut order, mut target_action) = load_investigation_facts(db, &self.context, executor).await?;
        if self.context.task.is_none() {
            ensure_no_active_w26_task(db, &order.base.id, executor).await?;
        }
        if self.context.action == SupplierOrderInvestigationAction::Replay {
            ensure_replay_safe(db, &order, &target_action, executor).await?;
        }

        let original_order = order.clone();
        let original_target = target_action.clone();
        let finding = apply_prepared_investigation(
            &self.context.subject(),
            &self.prepared,
            &mut order,
            &mut target_action,
        )?;

        if order != original_order {
            persist_order(db, &mut order, executor).await?;
        }
        if target_action != original_target {
            persist_action(db, &mut target_action, executor).await?;
        }

        let (record, response_summary) =
            serialize_investigation_record(&self.context.subject(), &target_action, &finding)?;
        let evidence = self.finish_evidence(db, &record, response_summary, executor).await?;

        let task_executor = &mut *executor;
        let task_version = update_task(task, order.base.version, |mut item| async move {
            db.work_items().update(&mut item, task_executor).await?;
            Ok(item)
        })
        .await?;

        persist_investigation_result(
            InvestigationWrite {
                db,
                order: &order,
                evidence: &evidence,
                context: &self.context,
                actor: &self.actor,
                command_id: &self.identity.command_id,
                fingerprint: &self.identity.fingerprint,
                task_version,
            },
            executor,
        )
        .await?;
        Ok((order, evidence, record, task_version))
    }

    /// 原任务或订单资格成立后，沿同一事务拒绝 API 自动执行对象。
    async fn authorize(&self, db: &Database, executor: &mut dyn Executor) -> Result<Option<WorkItem>> {
        authorize_investigation(
            db,
            &self.authorization,
            &self.access,
            self.workflow.as_ref(),
            &self.context,
            &self.actor,
            executor,
        )
        .await
    }

    async fn finish_evidence(
        &self,
        db: &Database,
        record: &InvestigationEvidenceRecord,
        response_summary: String,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOrderAction> {
        let mut evidence = db
            .supplier_order_actions()
            .find_by_id(&self.identity.evidence_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("供应商调查意图记录不存在".to_string()))?;
        validate_investigation_intent(
            &evidence,
            &self.context.subject(),
            &self.identity.evidence_idempotency_key,
        )?;
        let durable = parse_prepared_investigation(&evidence, &self.context.subject())?;
        if durable != self.prepared {
            return Err(Error::ConflictError("本次供应商调查已有处理结果，请刷新查看".to_string()));
        }

        finish_evidence(&mut evidence, record.outcome(), response_summary)?;
        persist_action(db, &mut evidence, executor).await?;
        Ok(evidence)
    }
}

/// 原快照核验的任务沿原 CAS 版本同步到新订单版本，不再以旧主题重验已更新订单。
async fn update_task<F, Fut>(task: Option<WorkItem>, order_version: u64, write: F) -> Result<Option<u64>>
where
    F: FnOnce(WorkItem) -> Fut,
    Fut: Future<Output = Result<WorkItem>>,
{
    let Some(mut task) = task else { return Ok(None) };
    task.subject_version = order_version.to_string();
    task.record_activity(Instant::now())?;
    let updated = write(task).await?;
    Ok(Some(updated.base.version))
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use erp_approval_runtime::ApprovalRuntime;
    use erp_core::AccountKind;
    use erp_core::ids::WorkItemId;
    use erp_identity::testing::unavailable_authorization;
    use erp_identity::{IdentityAuditPort, PreparedResourceAudit, RbacService, Result as IdentityResult};
    use erp_supply::dto::supplier_fulfillment::SupplierOrderResolution;
    use erp_workflow::WorkItemService;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority, WorkItemType};
    use erp_workflow::ports::{FailClosedAuditPort, WorkflowAuthorizationPort};
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::supply_execution::investigate::InvestigationTaskContext;
    use crate::workflow::FulfillmentWorkflowServices;

    struct NoAudit;

    #[derive(Clone)]
    struct NoWorkflowAuthorization;

    impl WorkflowAuthorizationPort for NoWorkflowAuthorization {}

    #[async_trait]
    impl IdentityAuditPort for NoAudit {
        fn resource_log(
            &self,
            _: AuditActor,
            _: &str,
            _: &str,
            _: String,
        ) -> IdentityResult<PreparedResourceAudit> {
            panic!("授权失败不得准备审计")
        }

        async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> IdentityResult<()> {
            panic!("授权失败不得持久化审计")
        }
    }

    fn authorized_task() -> WorkItem {
        let mut task = WorkItem::new(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::IntegrationResultUnknown,
                business_object_type: "SUPPLIER_FULFILLMENT_ORDER".into(),
                business_object_id: "order".into(),
                subject_version: "2".into(),
                owner_role: "operations".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "actor".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap();
        task.base.version = 3;
        task
    }

    #[tokio::test]
    async fn changed_order_advances_the_authorized_task_using_its_original_cas_version() {
        let original = authorized_task();
        let next = update_task(Some(original.clone()), 3, |mut task| async move {
            assert_eq!(task.base.id, "task");
            assert_eq!(task.base.version, 3);
            assert_eq!(task.subject_version, "3");
            assert_eq!(task.owner_user_id.as_deref(), Some("actor"));
            assert!(task.last_activity_at.is_some());
            task.base.version = 4;
            Ok(task)
        })
        .await
        .unwrap();
        assert_eq!(next, Some(4));
        assert_eq!(original.subject_version, "2");
        assert_eq!(update_task(None, 3, |_| async { panic!("对象入口不得更新任务") }).await.unwrap(), None);
        assert!(matches!(
            update_task(Some(original), 3, |_| async {
                Err(Error::ConflictError("任务版本已变化".into()))
            })
            .await,
            Err(Error::ConflictError(message)) if message == "任务版本已变化"
        ));
    }

    #[tokio::test]
    async fn investigation_stages_authorize_before_task_versions_and_order_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("investigation_commit");
        let authorization = unavailable_authorization();
        let rbac = Arc::new(RbacService::new(db.clone(), Arc::new(NoAudit)));
        let service =
            WorkItemService::new(db.clone(), rbac.clone(), authorization.clone(), NoWorkflowAuthorization);
        let runtime = Arc::new(ApprovalRuntime::new(
            db.clone(),
            authorization.clone(),
            rbac,
            NoWorkflowAuthorization,
            Arc::new(FailClosedAuditPort),
        ));

        let mut commit = InvestigationCommit::new(
            Arc::new(FulfillmentWorkflowServices::new(service, runtime)),
            authorization.clone(),
            FulfillmentOrderAccess::new(db.clone(), authorization),
            &InvestigationCommandContext {
                order_id: "order".into(),
                expected_order_version: 2,
                action: SupplierOrderInvestigationAction::QueryResult,
                operation_id: "operation".into(),
                target_action_id: "action".into(),
                task: Some(InvestigationTaskContext {
                    work_item_id: "task".into(),
                    expected_task_version: 3,
                    expected_subject_version: "2".into(),
                }),
                idempotency_key: "key".into(),
            },
            &InvestigationIdentity {
                command_id: "command".into(),
                fingerprint: "fingerprint".into(),
                evidence_id: "evidence".into(),
                evidence_idempotency_key: "evidence-key".into(),
            },
            PreparedInvestigation::PersistedTerminal(SupplierOrderResolution::OrderAccepted),
            &AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
        );

        for task in [true, false] {
            if !task {
                commit.context.task = None;
            }
            for action in
                [SupplierOrderInvestigationAction::QueryResult, SupplierOrderInvestigationAction::Replay]
            {
                commit.context.action = action;
                assert!(matches!(
                    commit.authorize(&db, &mut SnapshotExecutor::default()).await,
                    Err(Error::Internal(message)) if message == "测试授权设施不可用"
                ));
                assert!(matches!(
                    commit.authorize(&db, &mut ReadExecutor::default()).await,
                    Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
                ));
            }
        }
    }
}
