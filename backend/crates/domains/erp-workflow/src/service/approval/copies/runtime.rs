//! 从原审批事务中的中性事件生成个人抄送，外部通知不参与此事务。
use std::collections::BTreeSet;

use bpm::engine::{BpmEvent, BpmEventKind};
use bpm::graph::DefinitionGraph;
use bpm::model::ApprovalProcessInstance;
use chrono::Utc;
use entity_core::BaseModel;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::approval_copy::{ApprovalCopyConfig, ApprovalCopyReceipt, CopyTrigger};
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::approval_copy::ApprovalCopyExt;
use crate::repository::{ApprovalIntegrationExt, ApprovalSubjectSnapshotRepositoryExt, BpmExt};

/// 启动事务生成一次流程开始抄送；旧定义无配置时零写入。
/// # 参数
/// 数据库、当前授权端口、已冻结快照和提交事务。
/// # 返回
/// 无。
/// # 错误
/// 实例不匹配、配置损坏或数据库失败时中止提交。
pub async fn persist_started_copy(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    snapshot: &ApprovalSubjectSnapshot,
    ex: &mut dyn Executor,
) -> Result<()> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&snapshot.approval_process_instance_id, ex)
        .await?
        .ok_or_else(|| Error::Internal("抄送缺少审批实例".into()))?;
    let event = BpmEvent::new(
        BpmEventKind::InstanceStarted,
        snapshot.approval_process_instance_id.clone(),
        instance.current_round_no,
    );
    persist_events(db, auth, &instance, &[event], snapshot, ex).await
}

/// 决定或取消原事务中按绑定版本追加抄送。
/// # 参数
/// 数据库、授权端口、实例、已接受事件和原事务。
/// # 返回
/// 无；恢复与受阻事件不产生记录。
/// # 错误
/// 快照不匹配、数据库或账号读取失败时回滚。
pub async fn persist_copies(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    instance: &ApprovalProcessInstance,
    events: &[BpmEvent],
    ex: &mut dyn Executor,
) -> Result<()> {
    if !events.iter().any(|e| CopyTrigger::from_event(e).is_some()) {
        return Ok(());
    }
    if db
        .approval_copies()
        .config(instance.process_definition_id.as_ref(), ex)
        .await?
        .is_none_or(|config| config.rules.is_empty())
    {
        return Ok(());
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, ex)
        .await?
        .ok_or_else(|| Error::Internal("抄送缺少提交快照".into()))?;
    persist_events(db, auth, instance, events, &snapshot, ex).await
}

async fn persist_events(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    instance: &ApprovalProcessInstance,
    events: &[BpmEvent],
    snapshot: &ApprovalSubjectSnapshot,
    ex: &mut dyn Executor,
) -> Result<()> {
    let repository = db.approval_copies();
    let Some(config) = repository.config(instance.process_definition_id.as_ref(), ex).await? else {
        return Ok(());
    };
    if config.rules.is_empty() {
        return Ok(());
    }
    snapshot.ensure_matches_runtime_subject(
        snapshot.document_type,
        instance.subject.subject_id(),
        instance.subject_version,
    )?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&instance.process_definition_id, ex)
        .await?
        .ok_or_else(|| Error::Internal("抄送定义不存在".into()))?;
    if graph.definition.definition_version != instance.definition_version
        || snapshot.approval_process_instance_id.as_ref() != instance.base.id
    {
        return Err(Error::Internal("抄送版本不匹配".into()));
    }
    for event in events {
        persist_event(db, auth, Dispatch { instance, snapshot, graph: &graph, config: &config, event }, ex)
            .await?;
    }
    Ok(())
}
struct Dispatch<'a> {
    instance: &'a ApprovalProcessInstance,
    snapshot: &'a ApprovalSubjectSnapshot,
    graph: &'a DefinitionGraph,
    config: &'a ApprovalCopyConfig,
    event: &'a BpmEvent,
}
async fn persist_event(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    context: Dispatch<'_>,
    ex: &mut dyn Executor,
) -> Result<()> {
    let Dispatch { instance, snapshot, graph, config, event } = context;
    let now = Utc::now().timestamp();
    let Some(trigger) = CopyTrigger::from_event(event) else {
        return Ok(());
    };
    let node = event.node_key.as_deref().and_then(|key| graph.node(key));
    let ids = config
        .rules
        .iter()
        .filter(|r| r.matches(event, node.map(|n| n.display_order)))
        .flat_map(|r| r.recipient_user_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let accounts = auth.load_accounts(&ids.iter().cloned().collect::<Vec<_>>(), ex).await?;
    let mut rows = Vec::new();
    for id in ids {
        let account = accounts.iter().find(|a| a.id == id);
        let delivered = account.is_some_and(|a| a.is_active_backoffice());
        rows.push(ApprovalCopyReceipt {
            base: BaseModel::new(next_id()),
            instance_id: instance.base.id.clone(),
            snapshot_id: snapshot.base.id.clone(),
            subject_version: snapshot.subject_version,
            document_type: snapshot.document_type,
            document_no: snapshot.payload.document_no.clone(),
            submitter_name: snapshot.display.as_ref().and_then(|d| d.source.submitter_name.clone()),
            event_key: event_key(event),
            trigger,
            node_name: node.map(|n| n.node_name.clone()),
            round_no: event.round_no,
            recipient_name: account.map_or_else(
                || config.recipient_names.get(&id).cloned().unwrap_or_else(|| "账号不可用".into()),
                |a| a.display_name.clone(),
            ),
            recipient_user_id: id,
            delivered,
            undelivered_reason: (!delivered).then(|| "账号不存在、已停用或不是内部账号".into()),
            copied_at: now,
            read_at: None,
        });
    }
    db.approval_copies().insert(&rows, ex).await?;
    Ok(())
}

fn event_key(event: &BpmEvent) -> String {
    format!(
        "{}:{}:{}",
        event.kind.as_str(),
        event.round_no,
        event.execution_id.as_ref().map_or("", |id| id.as_ref())
    )
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};

    use super::*;
    #[test]
    fn identity_is_stable_for_retries_and_separates_rounds_and_executions() {
        let first = BpmEvent::new(BpmEventKind::NodeRejected, ApprovalProcessInstanceId::new("i"), 1)
            .with_execution(ApprovalNodeExecutionId::new("e1"));
        assert_eq!(event_key(&first), event_key(&first.clone()));
        let mut next = first.clone();
        next.round_no = 2;
        assert_ne!(event_key(&first), event_key(&next));
        next = first.clone();
        next.execution_id = Some(ApprovalNodeExecutionId::new("e2"));
        assert_ne!(event_key(&first), event_key(&next));
    }
}
