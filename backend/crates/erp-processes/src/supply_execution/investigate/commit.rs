//! 调查结果在同一执行器内应用、登记证据并更新任务与回执。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_supply::dto::supplier_fulfillment::SupplierOrderInvestigationAction;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_fulfillment::investigate::{
    InvestigationEvidenceRecord, PreparedInvestigation, apply_prepared_investigation, ensure_replay_safe,
    finish_evidence, parse_prepared_investigation, persist_action, persist_order,
    validate_investigation_intent,
};
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;

use super::prepare::{load_investigation_facts, load_investigation_task};
use super::{
    InvestigationCommandContext, InvestigationCommitResult, InvestigationIdentity, InvestigationWrite,
    ensure_no_active_w26_task, persist_investigation_result, serialize_investigation_record,
};
use crate::adapters::identity::shared_rbac_service;
use crate::{Error, Result};

pub(super) struct InvestigationCommit {
    context: InvestigationCommandContext,
    identity: InvestigationIdentity,
    prepared: PreparedInvestigation,
    actor: AuditActor,
    rbac: SharedRbacService,
}

impl InvestigationCommit {
    /// 准备最终事务的不可变输入；构造过程不读取或写入数据库。
    ///
    /// # 参数
    /// * `db` - 组合根数据库，仅用于装配授权服务。
    /// * `context` - 原命令及可选正式任务身份。
    /// * `identity` - 原命令与调查证据的稳定身份。
    /// * `prepared` - 已在事务外取得并冻结的结果。
    /// * `actor` - 当前操作人。
    /// # 返回
    /// 返回最终事务步骤的输入。
    /// # 错误
    /// 不返回错误。
    pub(super) fn new(
        db: &Database,
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
            rbac: shared_rbac_service(db.clone()),
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
        let (mut order, mut target_action) = load_investigation_facts(db, &self.context, executor).await?;
        let investigated_order_version = order.base.version;
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
        let task_version = self.update_task(db, &order, investigated_order_version, executor).await?;
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
            return Err(Error::ConflictError("供应商调查结果已由同一命令的另一执行确定".to_string()));
        }
        finish_evidence(&mut evidence, record.outcome(), response_summary)?;
        persist_action(db, &mut evidence, executor).await?;
        Ok(evidence)
    }

    async fn update_task(
        &self,
        db: &Database,
        order: &SupplierFulfillmentOrder,
        investigated_order_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<Option<u64>> {
        let Some(task_context) = &self.context.task else {
            return Ok(None);
        };
        let mut work_item = load_investigation_task(
            db,
            task_context,
            &order.base.id,
            investigated_order_version,
            &self.actor,
            Some(&self.rbac),
            executor,
        )
        .await?;
        work_item.subject_version = order.base.version.to_string();
        work_item.record_activity(self.actor.id(), Instant::now())?;
        db.work_items().update(&mut work_item, executor).await?;
        Ok(Some(work_item.base.version))
    }
}
