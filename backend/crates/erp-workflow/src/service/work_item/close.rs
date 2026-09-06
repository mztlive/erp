//! W29 异常任务受控关闭。

use std::sync::Arc;

use crate::entity::work_item::{WorkItem, WorkItemCloseData};
use crate::ports::W29CloseFact;
use crate::repository::WorkItemExt;
use application_core::CommandReceipt;

use erp_core::common::time::Instant;
use persistence_core::Transactional;
use validator::Validate;

use crate::error::{Error, Result};
use crate::ports::PreparedWorkflowAudit;
use application_core::AuditActor;

use super::access::{ensure_generic_work_item_mutation, ensure_item_in_managed_scope, ActorAccess};
use super::dto;
use super::write::{
    expected_task_version, required_text, work_item_update_error, WorkItemWriteError, WorkItemWriteOutcome,
    IDEMPOTENCY_AUDIT_PREFIX,
};
use super::{CloseWorkItemRequest, WorkItemConflictKind, WorkItemMutationOutcome, WorkItemService};

/// W29 关闭命令的领域证据与审计输入。
///
/// # 用途
/// 将关闭事务所需字段打包，供 [`WorkItemService::close_with_domain_evidence`] 使用。
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
/// 关闭必须在同一事务内写入领域证据与任务终态。
struct CloseDomainEvidenceInput<'a> {
    /// 待关闭任务。
    item: WorkItem,
    /// 操作人。
    actor: &'a AuditActor,
    /// 已规范化的 W29 关闭决策。
    decision: W29CloseFact,
    /// 强类型幂等命令收据。
    receipt: CommandReceipt,
}

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 关闭重复、误派或已有有效替代任务。
    ///
    /// # 错误
    /// 缺少管理权限、任务类型禁止通用关闭、原因非法或版本陈旧时返回错误。
    pub async fn close(
        self,
        id: String,
        req: CloseWorkItemRequest,
        actor: AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let managed_access = self.managed_access(&actor).await?;
        let item = self.load(id.clone()).await?;
        ensure_generic_work_item_mutation(&item)?;
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.close";
        let reason_code = required_text(&req.reason_code, "关闭原因代码不能为空")?;
        let replacement_id = req
            .replacement_work_item_id
            .as_deref()
            .map(|value| required_text(value, "替代任务ID不能为空"))
            .transpose()?;
        let decision =
            self.facts
                .prepare_w29_close(&reason_code, req.comment.as_deref(), replacement_id.as_deref())?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            action,
            "work_item",
            &id,
            &idempotency_key,
            [
                version,
                decision.close_reason.clone(),
                decision.replacement_work_item_id.clone().unwrap_or_default(),
            ],
        )?;
        if let Some(replayed) = self.idempotent_replay(&receipt, &id).await? {
            ensure_generic_work_item_mutation(&replayed)?;
            return self.applied_outcome(replayed, &actor).await;
        }
        if item.base.version != expected_task_version {
            return self
                .conflict_outcome(&id, WorkItemConflictKind::Version, &actor)
                .await;
        }
        ensure_item_in_managed_scope(&item, &managed_access)?;
        self.ensure_object_participation(&actor, &item).await?;
        if !item.is_w29_closable() {
            return Err(Error::BusinessLogicError(
                "只有 W29 登记的异常任务允许受控关闭".to_string(),
            ));
        }
        if let Some(replacement_id) = decision.replacement_work_item_id.as_deref() {
            self.ensure_w29_replacement(&item, replacement_id, &actor, &managed_access)
                .await?;
        }
        let updated = self
            .close_with_domain_evidence(CloseDomainEvidenceInput {
                item,
                actor: &actor,
                decision,
                receipt,
            })
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, &actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(&id, WorkItemConflictKind::Version, &actor)
                    .await
            }
        }
    }

    /// 校验重复关闭所引用的替代任务仍是同类、开放且位于当前管理范围。
    async fn ensure_w29_replacement(
        &self,
        current: &WorkItem,
        replacement_id: &str,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<()> {
        if replacement_id == current.base.id {
            return Err(Error::ValidationError("替代任务不能引用自身".to_string()));
        }
        let replacement = self.load(replacement_id.to_string()).await?;
        if !replacement.is_w29_replacement_for(current) {
            return Err(Error::ConflictError(
                "替代任务必须是同一 W29 对象类别的开放正式任务".to_string(),
            ));
        }
        ensure_item_in_managed_scope(&replacement, access)?;
        self.ensure_object_participation(actor, &replacement).await
    }

    /// 在同一事务内写入 W29 领域证据、关闭任务并登记审计。
    ///
    /// # 用途
    /// 将任务关闭与领域对象证据写入同一事务。
    ///
    /// # 参数
    /// * `input` - 任务、关闭原因与审计字段
    ///
    /// # 返回
    /// 返回写入结果或版本冲突。
    ///
    /// # 错误
    /// 替代任务非法、领域对象不存在或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅 W29 可关闭任务允许走此路径；替代任务必须是同类开放正式任务。
    async fn close_with_domain_evidence(
        &self,
        input: CloseDomainEvidenceInput<'_>,
    ) -> Result<WorkItemWriteOutcome> {
        let CloseDomainEvidenceInput {
            mut item,
            actor,
            decision,
            receipt,
        } = input;
        let closed_at = Instant::now();
        item.close(
            actor.id(),
            WorkItemCloseData {
                close_reason: decision.close_reason.clone(),
            },
            closed_at,
        )?;
        let evidence_reference = decision.evidence_reference(&item.base.id, receipt.id());
        let replay_receipt = receipt.clone();
        let replay_item_id = item.base.id.clone();
        let audit = PreparedWorkflowAudit::from_receipt(&receipt, actor.clone(), item.base.id.clone())?;
        let actor_id = actor.id().to_string();
        let receipt_id = receipt.id().to_string();
        let db = self.db.clone();
        let facts = Arc::clone(&self.facts);
        let audit_port = Arc::clone(&self.audit);
        let client = db.client().clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    facts
                        .persist_w29_close(
                            &item,
                            &decision,
                            &evidence_reference,
                            &actor_id,
                            &receipt_id,
                            closed_at,
                            session,
                        )
                        .await?;
                    db.work_items()
                        .update(&mut item, session)
                        .await
                        .map_err(work_item_update_error)?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<WorkItem, WorkItemWriteError>(item)
                })
            })
            .await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(WorkItemWriteError::VersionConflict) => Ok(WorkItemWriteOutcome::VersionConflict),
            Err(WorkItemWriteError::Service(error)) => {
                match self.idempotent_replay(&replay_receipt, &replay_item_id).await? {
                    Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                    None => Err(error),
                }
            }
        }
    }
}

/// 判断工作项投影是否属于 W29 可受控关闭关系。
///
/// # 参数
/// * `item` - 已授权的工作项投影字段
///
/// # 返回
/// 非审批的集成异常或对账差异任务返回 `true`。
///
/// # 错误
/// 无。
pub(super) fn is_w29_fields_closable(item: &dto::WorkItemFields) -> bool {
    item.work_item_type.is_w29_closable(
        &item.business_object_type,
        item.approval_node_execution_id.is_some(),
    )
}
