use bpm::engine::{DefinitionGraph, TaskIntent};
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::types::{
    ApprovalCommandKind, ApprovalExecutionAssignmentSource, ApprovalNodeExecutionStatus,
    ApprovalProcessInstanceStatus,
};
use bpm::model::ApprovalNodeExecution;
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalNotificationOutboxId, ApprovalSubjectSnapshotId, WorkItemId};
use erp_inventory::InventoryExt;
use erp_inventory::{StockAdjustment, StockAdjustmentLine, StockAdjustmentState};
use erp_workflow::entity::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
    ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload,
};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::DocumentApprovalWorkItemData;
use erp_workflow::entity::work_item::{WorkItem, WorkItemPriority};
use erp_workflow::ApprovalIntegrationExt;
use erp_workflow::BpmExt;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::WorkItemExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::adapter::require_frozen_binding;
use super::approval_prepare::{
    ensure_stock_adjustment_submit_authorized_with_executor, load_bound_definition_graph_with_executor,
    revalidate_stock_adjustment_start_candidates,
};
use super::approval_query::load_approval_binding;
use super::mapping::{list_projection_from_execution, stock_adjustment_start_scopes};
use crate::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_inventory::{ExpectedStockBalanceVersion, StockAdjustmentView};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::apply_plan::PlannedWrites;
use erp_workflow::service::approval::execution::map_receipt_first_write_error;
use erp_workflow::service::approval::process_kind::process_kind_of;

/// 库存调整启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的调整单、快照、启动计划与审计身份。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub struct StockAdjustmentStartPersistInput {
    /// 授权服务；事务内重验提交人与节点候选人权限。
    pub rbac: SharedRbacService,
    /// 已进入 `IN_APPROVAL` 的调整单。
    pub adjustment: StockAdjustment,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 调整单主键。
    pub id: String,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 的 Apply 写入计划。
    pub writes: PlannedWrites,
    /// 创建时冻结的定义绑定。
    pub binding: ApprovalDefinitionBinding,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
    /// 已完成服务端校验、用于冻结快照的最终明细。
    pub lines: Vec<StockAdjustmentLine>,
    /// 提交时必须仍匹配的余额版本。
    pub balances: Vec<ExpectedStockBalanceVersion>,
    /// 提交命令读取到的单据乐观锁版本。
    pub expected_document_version: u64,
    /// 本次启动必须冻结的审批主题版本。
    pub expected_subject_version: u32,
}

/// 在同一事务中写入单据迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入调整单、快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 调整单、快照与启动计划
///
/// # 返回
/// 返回提交后的调整单视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub async fn persist_stock_adjustment_start(
    db: &Database,
    input: StockAdjustmentStartPersistInput,
) -> Result<StockAdjustmentView> {
    let StockAdjustmentStartPersistInput {
        rbac,
        adjustment,
        actor,
        id,
        snapshot_payload,
        writes,
        binding,
        owner_role,
        organization_id,
        now,
        lines,
        balances,
        expected_document_version,
        expected_subject_version,
    } = input;
    let audit = actor
        .clone()
        .resource_log("stock_adjustment.submit", "stock_adjustment", id.clone())?;
    let db = db.clone();
    let client = db.client().clone();
    let updated = client
        .with_transaction(move |session| {
            Box::pin(async move {
                let current = db
                    .inventory()
                    .stock_adjustment(&id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                ensure_fresh_start_document(
                    &current,
                    &adjustment,
                    expected_document_version,
                    expected_subject_version,
                )?;
                ensure_stock_adjustment_submit_authorized_with_executor(
                    &db, &rbac, &current, &actor, session,
                )
                .await?;
                let persisted_binding = load_approval_binding(&db, &id, session).await?;
                let persisted_binding = require_frozen_binding(persisted_binding.as_ref())?;
                if persisted_binding != &binding {
                    return Err(Error::ConflictError(
                        "库存调整审批定义绑定已变化，请刷新后重试".to_string(),
                    ));
                }
                let graph =
                    load_bound_definition_graph_with_executor(&db, persisted_binding, session).await?;
                revalidate_stock_adjustment_start_candidates(
                    &db,
                    &rbac,
                    &graph,
                    actor.id(),
                    &organization_id,
                    session,
                )
                .await?;
                revalidate_start_lines(&db, &current, &lines, session).await?;
                validate_balance_versions(&db, &adjustment, &lines, &balances, session).await?;
                validate_start_writes(
                    &writes,
                    &graph,
                    &binding,
                    &id,
                    actor.id(),
                    expected_subject_version,
                )?;
                // 命令收据是事务内第一笔写入。并发 loser 退出失败事务后只允许
                // 使用新会话回读 winner，不得先留下任何业务或 BPM 写入。
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await
                    .map_err(map_receipt_first_write_error)?;
                let guarded = db
                    .business_documents()
                    .mark_approval_started(
                        &id,
                        DocumentType::StockAdjustment,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError(
                        "库存调整单审批启动守卫冲突，请刷新后重试".to_string(),
                    ));
                }
                // 同一事务先写本次提交的数量与状态，随后冻结完整展示。
                for line in &lines {
                    if !db
                        .inventory()
                        .update_adjustment_line(&line.base.id, line.quantity, Some(line.direction), session)
                        .await?
                    {
                        return Err(Error::NotFound("调整明细不存在".to_string()));
                    }
                }
                let mut adjustment = adjustment;
                db.stock_adjustments().update(&mut adjustment, session).await?;
                persist_runtime_writes(
                    &db,
                    &writes,
                    &snapshot_payload,
                    StartRuntimeContext {
                        owner_role,
                        organization_id: &organization_id,
                        document_no: &adjustment.adjustment_no,
                        submitted_by: actor.id(),
                        now,
                    },
                    session,
                )
                .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<StockAdjustment, crate::Error>(adjustment)
            })
        })
        .await?;
    Ok(updated.into())
}

/// 校验事务内草稿与预先构造的启动后单据仍为同一条原子迁移。
fn ensure_fresh_start_document(
    current: &StockAdjustment,
    target: &StockAdjustment,
    expected_document_version: u64,
    expected_subject_version: u32,
) -> Result<()> {
    let expected_next_subject = current
        .approval_subject_version
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("库存调整审批主题版本已达上限".to_string()))?;
    if current.base.id != target.base.id
        || current.base.version != expected_document_version
        || target.base.version != expected_document_version
        || current.status != StockAdjustmentState::Draft
        || target.status != StockAdjustmentState::InApproval
        || expected_next_subject != expected_subject_version
        || target.approval_subject_version != expected_subject_version
        || current.warehouse_id != target.warehouse_id
        || current.prepared_by != target.prepared_by
    {
        return Err(Error::ConflictError(
            "库存调整单事务内版本、状态或审批主题已变化".to_string(),
        ));
    }
    Ok(())
}

/// 在事务快照内重读明细身份与版本；数量和方向可由本次提交命令修改。
async fn revalidate_start_lines(
    db: &Database,
    current: &StockAdjustment,
    target_lines: &[StockAdjustmentLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let persisted = db
        .inventory()
        .adjustment_lines_by_adjustment_ids(
            &[erp_core::ids::StockAdjustmentId::new(current.base.id.clone())],
            executor,
        )
        .await?;
    if persisted.len() != target_lines.len()
        || persisted.iter().any(|line| {
            !target_lines.iter().any(|target| {
                target.base.id == line.base.id
                    && target.base.version == line.base.version
                    && target.stock_adjustment_id == line.stock_adjustment_id
                    && target.sku_id == line.sku_id
            })
        })
    {
        return Err(Error::ConflictError(
            "库存调整明细身份或版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 校验引擎启动计划精确绑定本次业务命令。
fn validate_start_writes(
    writes: &PlannedWrites,
    graph: &DefinitionGraph,
    binding: &ApprovalDefinitionBinding,
    adjustment_id: &str,
    actor_id: &str,
    expected_subject_version: u32,
) -> Result<()> {
    let expected_scope = stock_adjustment_start_scopes(adjustment_id, expected_subject_version)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Internal("库存调整启动命令缺少 V3 scope".to_string()))?;
    if writes.receipt.command_kind != ApprovalCommandKind::StartApproval
        || writes.receipt.scope_id != expected_scope
        || writes.receipt.result_ref != writes.instance.base.id
        || writes.instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || writes.instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || writes.instance.subject.subject_id() != adjustment_id
        || writes.instance.subject_version != expected_subject_version
        || writes.instance.process_definition_id != binding.approval_process_definition_id
        || writes.instance.definition_version != binding.approval_definition_version
        || writes.instance.started_by.as_str() != actor_id
        || writes.instance.status != ApprovalProcessInstanceStatus::Running
        || writes.instance.current_round_no != 1
        || writes.instance.blocker_code.is_some()
        || writes.instance.ended_at.is_some()
    {
        return Err(Error::Internal(
            "库存调整启动计划与签署命令身份不一致".to_string(),
        ));
    }
    let [first] = writes.created_executions.as_slice() else {
        return Err(Error::Internal(
            "库存调整启动计划必须且只能创建一个入口执行".to_string(),
        ));
    };
    if first.process_instance_id.as_ref() != writes.instance.base.id
        || first.round_no != writes.instance.current_round_no
        || first.status != ApprovalNodeExecutionStatus::Active
        || first.assignment_source != ApprovalExecutionAssignmentSource::Definition
        || first.replaces_execution_id.is_some()
        || writes
            .instance
            .current_node_execution_id
            .as_ref()
            .map(|id| id.as_ref())
            != Some(first.base.id.as_str())
    {
        return Err(Error::Internal(
            "库存调整启动计划的入口执行身份不一致".to_string(),
        ));
    }
    let entry = graph
        .entry_node()
        .map_err(|_| Error::Internal("库存调整事务内定义缺少入口节点".to_string()))?;
    if first.node_key != entry.node_key
        || first.node_name != entry.node_name
        || first.assignee_participant_id != entry.assignee_participant_id
        || first.assignee_name_snapshot != entry.assignee_label_snapshot
    {
        return Err(Error::ConflictError(
            "库存调整启动入口执行与事务内定义事实不一致".to_string(),
        ));
    }
    let [TaskIntent::HumanTaskRequested {
        execution_id,
        assignee,
        node_key,
        node_name,
        round_no,
    }] = writes.create_tasks.as_slice()
    else {
        return Err(Error::Internal(
            "库存调整启动计划必须且只能创建一个入口任务".to_string(),
        ));
    };
    if execution_id.as_ref() != first.base.id
        || assignee != &first.assignee_participant_id
        || node_key != &first.node_key
        || node_name != &first.node_name
        || *round_no != first.round_no
    {
        return Err(Error::Internal(
            "库存调整启动计划的入口任务身份不一致".to_string(),
        ));
    }
    if writes.created_assignees.len() != graph.nodes.len()
        || graph.nodes.iter().any(|node| {
            !writes.created_assignees.iter().any(|assignee| {
                assignee.process_instance_id.as_ref() == writes.instance.base.id
                    && assignee.node_key == node.node_key
                    && assignee.definition_assignee_participant_id == node.assignee_participant_id
                    && assignee.current_assignee_participant_id == node.assignee_participant_id
            })
        })
        || !writes.created_assignees.iter().any(|assignee| {
            assignee.node_key == first.node_key
                && assignee.current_assignee_participant_id == first.assignee_participant_id
        })
        || !writes.updated_executions.is_empty()
        || !writes.complete_tasks.is_empty()
        || !writes.close_tasks.is_empty()
    {
        return Err(Error::Internal(
            "库存调整启动计划的审批人绑定或写入集合不一致".to_string(),
        ));
    }
    Ok(())
}

/// 在提交事务内校验余额身份、维度与乐观锁版本。
///
/// # 参数
/// * `db` - 数据库
/// * `adjustment` - 待提交调整单
/// * `lines` - 最终调整明细
/// * `expected` - 客户端编辑时冻结的余额版本
/// * `session` - 当前事务
///
/// # 错误
/// 余额缺失、版本冲突、重复或不能完整覆盖明细维度时返回错误。
async fn validate_balance_versions(
    db: &Database,
    adjustment: &StockAdjustment,
    lines: &[StockAdjustmentLine],
    expected: &[ExpectedStockBalanceVersion],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let mut balance_ids = std::collections::HashSet::with_capacity(expected.len());
    let mut covered_dimensions = std::collections::HashSet::with_capacity(expected.len());
    for item in expected {
        if !balance_ids.insert(item.balance_id.as_str()) {
            return Err(Error::ValidationError("库存余额版本行不得重复".to_string()));
        }
        let balance = db
            .stock_balances()
            .find_by_id(&item.balance_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("库存余额不存在".to_string()))?;
        if balance.base.version != item.expected_version {
            return Err(Error::ConflictError("库存余额已变化，请刷新后重试".to_string()));
        }
        if balance.warehouse_id != adjustment.warehouse_id
            || !lines.iter().any(|line| line.sku_id == balance.sku_id)
        {
            return Err(Error::ValidationError("库存余额与调整单维度不一致".to_string()));
        }
        covered_dimensions.insert((balance.warehouse_id.to_string(), balance.sku_id.to_string()));
    }
    if lines.iter().any(|line| {
        !covered_dimensions.contains(&(adjustment.warehouse_id.to_string(), line.sku_id.to_string()))
    }) {
        return Err(Error::ValidationError(
            "提交缺少调整明细对应的库存余额版本".to_string(),
        ));
    }
    Ok(())
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 参数
/// * `db` - 数据库
/// * `writes` - 启动写入集合
/// * `snapshot_payload` - 快照载荷
/// * `owner_role` - 责任角色
/// * `organization_id` - 责任组织
/// * `now` - 调用方时间
/// * `session` - 当前事务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
struct StartRuntimeContext<'a> {
    owner_role: &'a str,
    organization_id: &'a str,
    document_no: &'a str,
    submitted_by: &'a str,
    now: Instant,
}

async fn persist_runtime_writes(
    db: &Database,
    writes: &PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    context: StartRuntimeContext<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交库存调整".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, context.now),
            session,
        )
        .await?;
    let mut snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::StockAdjustment,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    snapshot.display = Some(
        erp_read_models::workbench::capture_approval_display(
            db,
            snapshot.document_type,
            &snapshot.business_object_id,
            session,
        )
        .await?,
    );
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, session)
        .await?;
    persist_open_tasks(
        db,
        writes,
        context.owner_role,
        context.organization_id,
        context.now,
        session,
    )
    .await?;
    persist_start_notifications(
        db,
        writes,
        first,
        context.document_no,
        context.submitted_by,
        context.now,
        session,
    )
    .await
}

/// 严格消费启动计划的 `Started`/`Entered` 通知意图并同事务追加 outbox。
async fn persist_start_notifications(
    db: &Database,
    writes: &PlannedWrites,
    first: &ApprovalNodeExecution,
    document_no: &str,
    submitted_by: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let notification_identities = writes
        .notifications
        .iter()
        .map(|intent| (intent.event_kind, intent.dedup_key.clone()))
        .collect::<Vec<_>>();
    validate_start_notification_identities(
        &notification_identities,
        &writes.instance.base.id,
        &first.base.id,
    )?;
    for intent in &writes.notifications {
        let mut recipients = vec![first.assignee_participant_id.as_str().to_string()];
        if intent.event_kind == ApprovalNotificationEventKind::Started {
            recipients.push(submitted_by.to_string());
        }
        recipients.sort();
        recipients.dedup();
        let record = ApprovalNotificationOutbox::enqueue(
            ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            recipients,
            ApprovalNotificationTemplateParams {
                document_type_label: DocumentType::StockAdjustment.label().to_string(),
                document_no: document_no.to_string(),
                current_node_name: first.node_name.clone(),
                current_approver_display_name: first.assignee_name_snapshot.clone(),
                round_no: writes.instance.current_round_no,
                reject_reason_summary: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox()
            .create(&record, executor)
            .await?;
    }
    Ok(())
}

/// 校验启动通知恰为一条 Started 与一条 Entered，且去重键绑定运行身份。
pub(super) fn validate_start_notification_identities(
    notifications: &[(ApprovalNotificationEventKind, String)],
    instance_id: &str,
    execution_id: &str,
) -> Result<()> {
    let expected_started = format!("started:{instance_id}");
    let expected_entered = format!("entered:{execution_id}");
    if notifications.len() != 2
        || notifications
            .iter()
            .filter(|(kind, key)| *kind == ApprovalNotificationEventKind::Started && key == &expected_started)
            .count()
            != 1
        || notifications
            .iter()
            .filter(|(kind, key)| *kind == ApprovalNotificationEventKind::Entered && key == &expected_entered)
            .count()
            != 1
    {
        return Err(Error::Internal(
            "库存调整启动计划必须包含唯一且身份一致的 Started/Entered 通知意图".to_string(),
        ));
    }
    Ok(())
}

/// 将 `HumanTaskRequested` 映射为 `DOCUMENT_APPROVAL` 任务并写入。
///
/// # 参数
/// * `db` - 数据库
/// * `writes` - 启动写入
/// * `owner_role` - 责任角色
/// * `organization_id` - 责任组织
/// * `now` - 创建时间
/// * `session` - 当前事务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_open_tasks(
    db: &Database,
    writes: &PlannedWrites,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::StockAdjustment.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}
