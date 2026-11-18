//! 原提交人结束驳回处理责任；不删除或作废业务单据。
use application_core::{AuditActor, CommandReceipt};
use erp_core::common::time::Instant;
use erp_core::validation::normalize_required_text;
use erp_permissions::work_item;
use persistence_core::{Executor, Transactional};
use serde::Deserialize;

use super::write::expected_task_version;
use super::{WorkItemMutationOutcome, WorkItemService};
use crate::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use crate::entity::work_item_command::{WorkItemCommandReceipt, WorkItemCommandResult};
use crate::ports::{PreparedWorkflowAudit, WorkflowAuditOperation, WorkflowAuthorizationPort};
use crate::repository::prelude::*;
use crate::repository::{WorkItemCommandExt, WorkItemCommandRepositoryExt, WorkItemExt};
use crate::{Error, Result};

/// 原提交人放弃本次驳回处理的强类型请求。
#[derive(Debug, Clone, Deserialize)]
pub struct AbandonApprovalReworkRequest {
    pub expected_task_version: String,
    pub idempotency_key: String,
    pub reason: String,
}

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 原提交人关闭自己的驳回待处理任务。
    /// # 参数
    /// 任务ID、原版本与幂等键和放弃说明、已认证操作人。
    /// # 返回
    /// 写入或同载荷回放时返回任务ID；单据仍保留已驳回及编辑重提资格。
    /// # 错误
    /// 账号或权限失效、责任不符、任务版本冲突、异载荷重放或持久化失败时拒绝。
    pub async fn abandon_rework(
        self,
        id: String,
        req: AbandonApprovalReworkRequest,
        actor: AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let version = expected_task_version(&req.expected_task_version)?;
        let reason = normalize_required_text(req.reason, "放弃说明不能为空", 512, "放弃说明过长")?;
        let key = normalize_required_text(
            req.idempotency_key,
            "缺少本次操作的提交编号，请重新发起操作",
            128,
            "本次操作的提交编号无效，请重新发起操作",
        )?;
        let receipt = CommandReceipt::from_resource_parts(
            "work-item-command-",
            actor.id(),
            "work_item.abandon_rework",
            "work_item",
            &id,
            &key,
            [version.to_string(), reason.clone()],
        )?;
        let audit = PreparedWorkflowAudit::resource_with_message(
            actor.clone(),
            receipt.action(),
            "work_item",
            id.clone(),
            Some("原提交人已放弃本次驳回处理".into()),
        )?
        .for_command(receipt.id().to_owned(), WorkflowAuditOperation::ApprovalReworkAbandoned)?;
        self.audit.validate(&audit)?;

        let client = self.db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    self.apply_abandon_rework(&id, version, &reason, &receipt, &audit, &actor, executor).await
                })
            })
            .await
    }

    /// 同一事务内重新授权、仲裁幂等并关闭任务。
    #[allow(clippy::too_many_arguments)]
    async fn apply_abandon_rework(
        &self,
        id: &str,
        version: u64,
        reason: &str,
        receipt: &CommandReceipt,
        audit: &PreparedWorkflowAudit,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemMutationOutcome> {
        self.authorization.require(actor, &work_item::MANAGE, id, executor).await?;
        let mut item = self
            .db
            .work_items()
            .find_work_item(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".into()))?;
        ensure_rework_owner(&item, actor.id())?;

        if let Some(committed) =
            self.db.work_item_command_receipts().find_command(receipt.id(), executor).await?
        {
            verify_abandon_replay(&item, &committed, receipt, actor.id())?;
            return Ok(WorkItemMutationOutcome::Applied {
                work_item_id: id.to_owned(),
                task_version: item.base.version,
            });
        }

        if item.base.version != version {
            return Err(Error::ConflictError("任务版本已变化，请刷新后重试".into()));
        }
        item.abandon_approval_rework(actor.id(), reason, Instant::now())?;
        self.db.work_items().update(&mut item, executor).await?;
        self.audit.persist(audit, executor).await?;
        let committed = WorkItemCommandReceipt::new(
            receipt,
            WorkItemCommandResult::ReworkAbandoned {
                work_item_id: id.to_owned(),
                task_version: item.base.version,
                reason: reason.to_owned(),
            },
            audit.id.clone(),
        )?;
        self.db.work_item_command_receipts().create(&committed, executor).await?;
        Ok(WorkItemMutationOutcome::Applied { work_item_id: id.to_owned(), task_version: item.base.version })
    }
}

fn ensure_rework_owner(item: &WorkItem, actor: &str) -> Result<()> {
    if item.work_item_type != WorkItemType::ApprovalRework || item.owner_user_id.as_deref() != Some(actor) {
        return Err(Error::Forbidden("仅原提交人可以放弃驳回待处理任务".into()));
    }
    Ok(())
}

/// 回放必须证明原提交人已经关闭同一任务，禁止把其它终态或损坏结果当作已放弃。
fn verify_abandon_replay(
    item: &WorkItem,
    committed: &WorkItemCommandReceipt,
    command: &CommandReceipt,
    actor: &str,
) -> Result<()> {
    let (id, version) = committed.committed_task(command)?;
    let WorkItemCommandResult::ReworkAbandoned { reason, .. } = &committed.result else {
        return Err(Error::Internal("驳回处理回执类型不一致".into()));
    };
    ensure_rework_owner(item, actor)?;
    if item.base.id != id
        || item.base.version != version
        || item.status != WorkItemStatus::Closed
        || item.closed_by.as_deref() != Some(actor)
        || item.close_reason.as_deref() != Some(reason.as_str())
    {
        return Err(Error::Internal("驳回处理回执与任务不一致".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bpm::ApprovalProcessInstanceId;
    use erp_core::AccountKind;
    use erp_core::ids::WorkItemId;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::work_item::{
        AssignmentSource, WorkItemData, WorkItemPriority, WorkItemSubjectVersions,
    };
    use crate::ports::{ObjectFact, ObjectFactMap, ObjectKind};
    use crate::service::work_item::access::task_facts_current;
    use crate::service::work_item::testing::unavailable_service;

    #[tokio::test]
    async fn abandon_rework_stops_before_task_reads_when_authorization_fails() {
        let service = unavailable_service();
        let actor = AuditActor::new("submitter".into(), "submitter".into(), AccountKind::Admin);
        let receipt = CommandReceipt::from_resource_parts(
            "work-item-command-",
            actor.id(),
            "work_item.abandon_rework",
            "work_item",
            "unread-task",
            "key",
            ["7".to_owned(), "不再办理".to_owned()],
        )
        .unwrap();
        let audit = PreparedWorkflowAudit::resource_with_id(
            "audit".into(),
            actor.clone(),
            receipt.action(),
            "work_item",
            "unread-task".into(),
            None,
        )
        .unwrap();
        let result = service
            .apply_abandon_rework(
                "unread-task",
                7,
                "不再办理",
                &receipt,
                &audit,
                &actor,
                &mut SnapshotExecutor::default(),
            )
            .await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }

    #[test]
    fn abandoned_v1_replays_after_v2_only_with_exact_history_and_matching_command() {
        let id = "approval-rework:instance-v1";
        let mut item = WorkItem::new_with_responsibility_key(
            WorkItemId::new(id),
            WorkItemData {
                work_item_type: WorkItemType::ApprovalRework,
                business_object_type: "purchase_payment_request".into(),
                business_object_id: "request".into(),
                subject_version: "1".into(),
                owner_role: "approval_submitter".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "submitter".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "approval_rework:instance-v1",
        )
        .unwrap();
        item.abandon_approval_rework("submitter", "不再办理", Instant::from_unix_secs(1)).unwrap();
        item.base.version = 2;
        let command = |reason: &str| {
            CommandReceipt::from_resource_parts(
                "work-item-command-",
                "submitter",
                "work_item.abandon_rework",
                "work_item",
                id,
                "key",
                ["1".to_owned(), reason.to_owned()],
            )
            .unwrap()
        };
        let receipt = WorkItemCommandReceipt::new(
            &command("不再办理"),
            WorkItemCommandResult::ReworkAbandoned {
                work_item_id: id.into(),
                task_version: 2,
                reason: "不再办理".into(),
            },
            "audit".into(),
        )
        .unwrap();

        let mut fact = ObjectFact::new("request", "付款申请", "submitter");
        fact.subject_versions = WorkItemSubjectVersions::constrained(vec!["2".into()]).unwrap();
        let key = (ObjectKind::PurchasePaymentRequest, "request".into());
        assert!(!task_facts_current(&item, &ObjectFactMap::from([(key.clone(), fact.clone())])));
        fact.subject_versions.register_rework_history(&ApprovalProcessInstanceId::new("instance-v1"), 1);
        let facts = ObjectFactMap::from([(key, fact)]);
        assert!(task_facts_current(&item, &facts));

        verify_abandon_replay(&item, &receipt, &command("不再办理"), "submitter").unwrap();
        assert!(verify_abandon_replay(&item, &receipt, &command("另一说明"), "submitter").is_err());
        assert!(verify_abandon_replay(&item, &receipt, &command("不再办理"), "other").is_err());

        item.status = WorkItemStatus::Open;
        assert!(!task_facts_current(&item, &facts));
        assert!(verify_abandon_replay(&item, &receipt, &command("不再办理"), "submitter").is_err());
    }
}
