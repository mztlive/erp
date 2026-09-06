//! 决定、恢复与受阻取消通知 outbox 写入。

use std::collections::HashSet;

use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::repository::ApprovalIntegrationExt;
use bpm::model::ApprovalNodeExecution;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use super::super::apply_plan::PlannedWrites;
use super::read_auth::runtime_object_readable;
use crate::error::{Error, Result};
use crate::service::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use crate::service::approval::{
    approval_document_read_scope_with_executor, definition_management_visibility_with_executor,
};
use application_core::AuditActor;

/// 决定通知只消费冻结快照、实际执行与当前权限事实。
pub(super) struct DecisionNotificationFacts<'a> {
    pub(super) document_type_label: &'a str,
    pub(super) document_no: &'a str,
    pub(super) submitted_by: &'a str,
    pub(super) ended_execution: &'a ApprovalNodeExecution,
    pub(super) reject_reason: Option<&'a str>,
    pub(super) runtime_admin_ids: &'a [String],
}

/// 受阻取消通知所需的模板与收件人事实。
///
/// # 用途
/// 打包 [`persist_cancel_notifications`] 的通知上下文。
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
/// 收件人固定为提交人与实际取消的运行管理员。
pub(super) struct CancelNotificationFacts<'a> {
    pub(super) submitted_by: &'a str,
    pub(super) actor_id: &'a str,
    pub(super) document_type_label: &'a str,
    pub(super) document_no: &'a str,
    pub(super) current_node_name: &'a str,
    pub(super) current_approver_display_name: &'a str,
}

/// 在受阻取消事务内追加通知 outbox。
///
/// # 用途
/// 校验受阻取消计划仅含一条 BlockedCancelled 意图并写入 outbox。
///
/// # 参数
/// * `db` - 数据库
/// * `writes` - 受阻取消计划
/// * `facts` - 模板与收件人事实
/// * `now` - 入队时间
/// * `session` - 事务会话
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 计划含任务变更、通知意图不匹配或入队失败时返回错误。
///
/// # 关键业务约束
/// 受阻取消不得创建、完成或关闭审批任务。
pub(super) async fn persist_cancel_notifications(
    db: &Database,
    writes: &PlannedWrites,
    facts: CancelNotificationFacts<'_>,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    if !writes.create_tasks.is_empty() || !writes.complete_tasks.is_empty() || !writes.close_tasks.is_empty()
    {
        return Err(Error::Internal(
            "受阻取消计划不得创建、完成或关闭审批任务".to_string(),
        ));
    }
    let [intent] = writes.notifications.as_slice() else {
        return Err(Error::Internal("受阻取消必须且只能产生一条通知意图".to_string()));
    };
    let expected_dedup = format!("blocked_cancelled:{}", writes.instance.base.id);
    if intent.event_kind
        != crate::entity::approval_integration::ApprovalNotificationEventKind::BlockedCancelled
        || intent.dedup_key != expected_dedup
    {
        return Err(Error::Internal("受阻取消通知意图不匹配".to_string()));
    }
    let record = crate::entity::approval_integration::ApprovalNotificationOutbox::enqueue(
        erp_core::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
        intent.dedup_key.clone(),
        intent.event_kind,
        blocked_cancel_notification_recipients(facts.submitted_by, facts.actor_id),
        crate::entity::approval_integration::ApprovalNotificationTemplateParams {
            document_type_label: facts.document_type_label.to_string(),
            document_no: facts.document_no.to_string(),
            current_node_name: facts.current_node_name.to_string(),
            current_approver_display_name: facts.current_approver_display_name.to_string(),
            round_no: writes.instance.current_round_no,
            reject_reason_summary: None,
        },
        now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_notification_outbox().create(&record, session).await?;
    Ok(())
}

/// 受阻取消固定通知提交人和实际执行取消的运行管理员；同人时只保留一次。
pub(super) fn blocked_cancel_notification_recipients(submitted_by: &str, actor_id: &str) -> Vec<String> {
    if submitted_by == actor_id {
        return vec![submitted_by.to_string()];
    }
    vec![submitted_by.to_string(), actor_id.to_string()]
}

/// 读取当前有效且真正具备该单据类型运行管理权限的通知收件人。
pub(super) async fn runtime_admin_notification_recipients(
    _db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    document_type: DocumentType,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let spec = adapter_spec_of(document_type)?;
    let accounts = rbac
        .list_accounts_by_kind(erp_core::AccountKind::Admin, executor)
        .await?;
    let mut recipients = Vec::new();
    for account in accounts {
        if !account.is_active_backoffice() {
            continue;
        }
        let actor = AuditActor::new(account.id.clone(), account.id.clone(), account.kind);
        let visibility = definition_management_visibility_with_executor(rbac, &actor, executor).await?;
        let read_scope =
            approval_document_read_scope_with_executor(rbac, &actor, document_type, executor).await?;
        let context = BindingRevalidationContext {
            organization_id: snapshot.payload.responsible_org_id.clone(),
            creator_id: snapshot.payload.submitted_by.clone(),
        };
        let read_scope_covers =
            !read_scope.is_empty() && read_scope.covers(&snapshot.payload.responsible_org_id);
        let object_readable = runtime_object_readable(&spec, &context, &account.id, read_scope_covers)?;
        if visibility.runtime_admin_types().contains(&document_type) && read_scope_covers && object_readable {
            recipients.push(account.id);
        }
    }
    recipients.sort();
    recipients.dedup();
    Ok(recipients)
}

/// 按 §16.5 消费决定计划中的每个通知意图并写入同一事务 outbox。
pub(super) async fn persist_decision_notifications(
    db: &Database,
    writes: &PlannedWrites,
    facts: DecisionNotificationFacts<'_>,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    use crate::entity::approval_integration::ApprovalNotificationEventKind as EventKind;

    if writes.notifications.is_empty() {
        return Err(Error::Internal("审批决定计划缺少通知意图".to_string()));
    }
    let mut seen = HashSet::with_capacity(writes.notifications.len());
    for intent in &writes.notifications {
        if !seen.insert(intent.dedup_key.as_str()) {
            return Err(Error::Internal("审批决定计划包含重复通知意图".to_string()));
        }
        let event_execution = match intent.event_kind {
            EventKind::Entered | EventKind::NodeApproved | EventKind::NodeRejected | EventKind::Blocked => {
                let execution_id = intent
                    .dedup_key
                    .split_once(':')
                    .map(|(_, id)| id)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| Error::Internal("审批决定通知缺少执行引用".to_string()))?;
                writes
                    .created_executions
                    .iter()
                    .chain(writes.updated_executions.iter())
                    .find(|execution| execution.base.id == execution_id)
                    .or_else(|| {
                        (facts.ended_execution.base.id == execution_id).then_some(facts.ended_execution)
                    })
                    .ok_or_else(|| Error::Internal("审批决定通知执行引用不存在".to_string()))?
            }
            EventKind::Completed => facts.ended_execution,
            _ => return Err(Error::Internal("审批决定计划包含非决定通知事件".to_string())),
        };
        let expected_dedup = match intent.event_kind {
            EventKind::Entered => format!("entered:{}", event_execution.base.id),
            EventKind::NodeApproved => format!("approved:{}", event_execution.base.id),
            EventKind::NodeRejected => format!("rejected:{}", event_execution.base.id),
            EventKind::Blocked => format!("blocked:{}", event_execution.base.id),
            EventKind::Completed => format!("completed:{}", writes.instance.base.id),
            _ => unreachable!("unsupported decision event was rejected above"),
        };
        if intent.dedup_key != expected_dedup {
            return Err(Error::Internal("审批决定通知去重键不匹配".to_string()));
        }
        let recipients = match intent.event_kind {
            EventKind::Entered => vec![event_execution.assignee_participant_id.as_str().to_string()],
            EventKind::NodeApproved | EventKind::NodeRejected | EventKind::Completed => {
                vec![facts.submitted_by.to_string()]
            }
            EventKind::Blocked => notification_recipients(
                facts.submitted_by,
                facts.runtime_admin_ids.iter().map(String::as_str),
            ),
            _ => unreachable!("unsupported decision event was rejected above"),
        };
        let record = crate::entity::approval_integration::ApprovalNotificationOutbox::enqueue(
            erp_core::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            recipients,
            crate::entity::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: facts.document_type_label.to_string(),
                document_no: facts.document_no.to_string(),
                current_node_name: event_execution.node_name.clone(),
                current_approver_display_name: event_execution.assignee_name_snapshot.clone(),
                round_no: event_execution.round_no,
                reject_reason_summary: (intent.event_kind == EventKind::NodeRejected)
                    .then(|| facts.reject_reason.map(ToOwned::to_owned))
                    .flatten(),
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
    Ok(())
}

/// 以主收件人开头追加其它收件人并稳定去重。
pub(super) fn notification_recipients<'a>(
    primary: &str,
    additional: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut recipients = vec![primary.to_string()];
    for recipient in additional {
        if !recipients.iter().any(|existing| existing == recipient) {
            recipients.push(recipient.to_string());
        }
    }
    recipients
}

/// 原审批人恢复通知所需的执行与模板事实。
///
/// # 用途
/// 打包 [`persist_resume_notifications`] 的通知上下文。
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
/// 必须同时产生 Entered 与 Resumed 两条意图。
pub(super) struct ResumeNotificationFacts<'a> {
    pub(super) new_execution: &'a ApprovalNodeExecution,
    pub(super) submitted_by: &'a str,
    pub(super) document_type_label: &'a str,
    pub(super) document_no: &'a str,
}

/// 在恢复事务内按新执行事实追加进入节点与原审批人恢复通知。
///
/// # 用途
/// 校验恢复计划通知意图并写入同一事务 outbox。
///
/// # 参数
/// * `db` - 数据库
/// * `writes` - 恢复计划
/// * `facts` - 新执行与模板事实
/// * `now` - 入队时间
/// * `session` - 事务会话
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 意图数量/类型不匹配、去重键错误或入队失败时返回错误。
///
/// # 关键业务约束
/// 恢复必须且只能产生 Entered 与 Resumed 两条通知。
pub(super) async fn persist_resume_notifications(
    db: &Database,
    writes: &PlannedWrites,
    facts: ResumeNotificationFacts<'_>,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    use crate::entity::approval_integration::ApprovalNotificationEventKind as EventKind;

    if writes.notifications.len() != 2 {
        return Err(Error::Internal(
            "原审批人恢复必须产生进入节点和恢复两条通知意图".to_string(),
        ));
    }
    let mut seen = HashSet::with_capacity(2);
    for intent in &writes.notifications {
        if !seen.insert(intent.event_kind) {
            return Err(Error::Internal("原审批人恢复包含重复通知意图".to_string()));
        }
        let expected_dedup = match intent.event_kind {
            EventKind::Entered => format!("entered:{}", facts.new_execution.base.id),
            EventKind::Resumed => format!("resumed:{}", facts.new_execution.base.id),
            _ => {
                return Err(Error::Internal(
                    "原审批人恢复包含非进入节点或恢复通知".to_string(),
                ));
            }
        };
        if intent.dedup_key != expected_dedup {
            return Err(Error::Internal("原审批人恢复通知去重键不匹配".to_string()));
        }
        let primary = facts.new_execution.assignee_participant_id.as_str();
        let recipients = match intent.event_kind {
            EventKind::Entered => vec![primary.to_string()],
            EventKind::Resumed => notification_recipients(primary, [facts.submitted_by]),
            _ => unreachable!("unsupported resume event was rejected above"),
        };
        let record = crate::entity::approval_integration::ApprovalNotificationOutbox::enqueue(
            erp_core::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            recipients,
            crate::entity::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: facts.document_type_label.to_string(),
                document_no: facts.document_no.to_string(),
                current_node_name: facts.new_execution.node_name.clone(),
                current_approver_display_name: facts.new_execution.assignee_name_snapshot.clone(),
                round_no: facts.new_execution.round_no,
                reject_reason_summary: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
    if !seen.contains(&EventKind::Entered) || !seen.contains(&EventKind::Resumed) {
        return Err(Error::Internal(
            "原审批人恢复缺少进入节点或恢复通知意图".to_string(),
        ));
    }
    Ok(())
}
