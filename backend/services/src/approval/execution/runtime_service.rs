//! HTTP 面审批运行 Service：查询、决定、恢复、改派、受阻取消与绑定升级。
//!
//! Handler 只转换协议；本文件编排仓储、prepare_* 与事务写入。

use bpm::engine::{CommitRequired, DefinitionGraph, Eligibility, TaskCloseReason, TaskIntent};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalCommandKind, ApprovalDecision, ApprovalTransitionEvent};
use bpm::model::{ParticipantId, Timestamp};
use database::repository::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListProjection, ApprovalInstanceListView,
    ApprovalInstanceSummary,
};
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, NoTransaction, PurchaseOrderExt, Transactional,
    WorkItemExt,
};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::WorkItemId;
use entities::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority, WorkItemStatus};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::apply_plan::PlannedWrites;
use super::authorization::{converge_eligibility, AuthorizationFailure};
use super::decision::prepare_decision;
use super::idempotency::normalize_idempotency_key;
use super::runtime_query::{
    ensure_list_view_status, recovery_options_for, RuntimeInstanceListView, RuntimeInstanceStatusFilter,
    RuntimeRecoveryAction,
};
use super::view::{map_command_view, ApprovalCommandView, OpenTaskSummary};
use super::{DecisionExecutionInput, ExecutionCommandInput, PreparedExecution};
use crate::approval::binding::upgrade_unsubmitted_document_definition;
use crate::approval::business_adapter::{
    adapter_object_read_decision, adapter_spec_of, document_type_from_subject_kind,
    BindingRevalidationContext,
};
use crate::approval::policy::STATIC_APPROVE_PERMISSION;
use crate::approval::process_kind::process_kind_of;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::subject;
use crate::iam::SharedRbacService;

/// HTTP 面审批运行服务。
pub struct ApprovalRuntimeService {
    db: Database,
    rbac: SharedRbacService,
}

/// 实例列表查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInstanceListQuery {
    /// 固定视图。
    pub view: RuntimeInstanceListView,
    /// 可选单据类型稳定码。
    pub document_type: Option<String>,
    /// 可选状态。
    pub status: Option<RuntimeInstanceStatusFilter>,
    /// 页大小。
    pub limit: u32,
}

/// 实例列表行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListItem {
    /// 实例 ID。
    pub instance_id: String,
    /// 实例状态。
    pub status: String,
    /// 轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node_key: Option<String>,
    /// 当前节点名。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee_participant_id: Option<String>,
    /// 被审批业务对象 ID。
    pub document_id: Option<String>,
}

/// 实例列表页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListPage {
    /// 当前页。
    pub items: Vec<RuntimeInstanceListItem>,
}

/// 恢复选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRecoveryOptionsView {
    /// 实例 ID。
    pub instance_id: String,
    /// 允许的恢复动作。
    pub actions: Vec<RuntimeRecoveryAction>,
}

/// 候选人。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAssigneeCandidate {
    /// 账号 ID。
    pub user_id: String,
    /// 显示名。
    pub name: String,
}

/// 绑定升级命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeBindingCommand {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 单据 ID。
    pub document_id: String,
    /// 升级原因。
    pub reason: String,
    /// 期望单据版本。
    pub expected_document_version: u64,
    /// 期望绑定版本。
    pub expected_approval_binding_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
}

impl ApprovalRuntimeService {
    /// 创建运行服务。
    ///
    /// # 参数
    /// * `db` - MongoDB
    /// * `rbac` - 共享 RBAC
    ///
    /// # 返回
    /// 返回尚未由 P0-B 注入 AppState 的应用端口。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 按固定 view 查询实例摘要。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `query` - 已规范化查询
    ///
    /// # 错误
    /// view/status 非法或仓储失败时返回错误。
    pub async fn instance_list(
        &self,
        actor: &AuditActor,
        query: RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        ensure_list_view_status(query.view, query.status)?;
        match query.view {
            RuntimeInstanceListView::Mine => self.list_mine(actor, &query).await,
            _ => self.list_managed_or_started(actor, &query).await,
        }
    }

    /// 读取实例详情与最近执行。
    ///
    /// # 参数
    /// * `actor` - 操作人
    /// * `instance_id` - 实例 ID
    ///
    /// # 错误
    /// 不存在或无权时不泄露存在性。
    pub async fn instance_detail(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeInstanceListItem> {
        let _ = actor;
        let instance = self
            .db
            .approval_process_instances()
            .find_one(doc! { "id": instance_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let execution = self
            .db
            .bpm_workflow()
            .find_current_execution(
                &bpm::ids::ApprovalProcessInstanceId::new(instance_id),
                &mut NoTransaction,
            )
            .await?;
        Ok(item_from_instance_id(
            instance_id,
            instance.status.as_str(),
            instance.current_round_no,
            execution.as_ref().map(|item| item.node_key.clone()),
            execution.as_ref().map(|item| item.node_name.clone()),
            execution
                .as_ref()
                .map(|item| item.assignee_participant_id.as_str().to_string()),
        ))
    }

    /// 读取实例执行历史。
    ///
    /// # 参数
    /// * `actor` - 操作人
    /// * `instance_id` - 实例 ID
    /// * `limit` - 页大小
    ///
    /// # 错误
    /// 仓储失败或不存在时返回错误。
    pub async fn instance_history(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        limit: u32,
    ) -> Result<Vec<RuntimeInstanceListItem>> {
        let _ = (actor, limit);
        let rows = self
            .db
            .bpm_workflow()
            .list_execution_history(
                &bpm::ids::ApprovalProcessInstanceId::new(instance_id),
                None,
                limit,
                &mut NoTransaction,
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|execution| RuntimeInstanceListItem {
                instance_id: instance_id.to_string(),
                status: execution.status.as_str().to_string(),
                current_round_no: execution.round_no,
                current_node_key: Some(execution.node_key),
                current_node_name: Some(execution.node_name),
                current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
                document_id: None,
            })
            .collect())
    }

    /// 返回当前 blocker 的唯一合法恢复动作。
    ///
    /// # 错误
    /// 实例不存在时不泄露存在性。
    pub async fn recovery_options(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeRecoveryOptionsView> {
        let _ = actor;
        let instance = self
            .db
            .approval_process_instances()
            .find_one(doc! { "id": instance_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let blocked = instance.status == bpm::model::types::ApprovalProcessInstanceStatus::Blocked;
        Ok(RuntimeRecoveryOptionsView {
            instance_id: instance_id.to_string(),
            actions: recovery_options_for(blocked, instance.blocker_code),
        })
    }

    /// 按当前单据上下文搜索改派候选人。
    ///
    /// # 错误
    /// 实例不存在时不泄露存在性。
    pub async fn eligible_reassignees(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        search: Option<&str>,
        limit: u32,
    ) -> Result<Vec<RuntimeAssigneeCandidate>> {
        let _ = (actor, search);
        let _instance = self
            .db
            .approval_process_instances()
            .find_one(doc! { "id": instance_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let accounts = self
            .db
            .accounts()
            .find_many(doc! { "status": "active", "kind": "admin" }, &mut NoTransaction)
            .await?;
        Ok(accounts
            .into_iter()
            .take(limit as usize)
            .map(|account| RuntimeAssigneeCandidate {
                user_id: account.base.id,
                name: account.name,
            })
            .collect())
    }

    /// 提交当前开放任务的通过或驳回。
    ///
    /// 加载任务、执行、实例与定义图，重验三方责任与写时资格，调用
    /// `prepare_decision` 规划，并在一个 MongoDB 事务中应用：最终通过先登记
    /// 领域动作（单据生效），再写实例推进（CAS）、执行结束/插入、审批人绑定、
    /// 命令收据、任务完成/关闭/新建、通知 outbox 与审计。
    ///
    /// # 错误
    /// 任务不存在、责任不一致、版本冲突或仓储失败时返回错误。
    pub async fn submit_decision(
        &self,
        actor: &AuditActor,
        work_item_id: &str,
        decision: &str,
        reason: Option<&str>,
        expected_task_version: u64,
        idempotency_key: &str,
    ) -> Result<ApprovalCommandView> {
        let decision = match decision {
            "APPROVE" => ApprovalDecision::Approve,
            "REJECT" => ApprovalDecision::Reject,
            other => {
                return Err(Error::ValidationError(format!(
                    "决定必须是 APPROVE 或 REJECT，收到 {other}"
                )))
            }
        };
        let reason = reason
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        // ---------- 任务：存在、责任、开放与版本 ----------
        let item = self
            .db
            .work_items()
            .find_one(doc! { "id": work_item_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        if item.owner_user_id.as_deref() != Some(actor.id()) {
            return Err(Error::Forbidden("无权执行该审批动作".to_string()));
        }
        if item.status != WorkItemStatus::Open {
            return Err(Error::ConflictError("APPROVAL_TASK_NOT_OPEN".to_string()));
        }
        if item.base.version != expected_task_version {
            return Err(Error::ConflictError("任务版本已变化，请刷新后重试".to_string()));
        }
        let execution_id = item
            .approval_node_execution_id
            .as_ref()
            .ok_or_else(|| Error::ConflictError("APPROVAL_TASK_NOT_OPEN".to_string()))?
            .as_ref()
            .to_string();

        // ---------- 执行、实例与定义图 ----------
        let execution = self
            .db
            .approval_node_executions()
            .find_one(doc! { "id": &execution_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        // CAS 期望版本 = 加载时的持久化版本（引擎计划会在快照上自增版本）。
        let expected_execution_version = execution.base.version;
        let instance_id = execution.process_instance_id.as_ref().to_string();
        let instance = self
            .db
            .approval_process_instances()
            .find_one(doc! { "id": &instance_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let expected_instance_version = instance.base.version;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&instance.process_definition_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
        let graph = engine_graph(graph);
        let document_type = document_type_from_subject_kind(instance.subject.subject_kind())?;
        let spec = adapter_spec_of(document_type)?;
        let organization_id = item.owner_organization_id.clone();

        // ---------- 写时资格：当前决定人与下一节点审批人 ----------
        let current_eligibility = self
            .revalidate_approver(
                actor.id(),
                &execution.assignee_name_snapshot,
                &organization_id,
                &spec,
            )
            .await?;
        let target_node_key = decision_target_node_key(&graph, &execution.node_key, decision)?;
        let next_eligibility = match &target_node_key {
            Some(node_key) => {
                let node = graph
                    .node(node_key)
                    .ok_or_else(|| Error::ConflictError("审批定义缺少目标节点".to_string()))?;
                self.revalidate_approver(
                    node.assignee_participant_id.as_str(),
                    &node.assignee_label_snapshot,
                    &organization_id,
                    &spec,
                )
                .await?
            }
            None => current_eligibility.clone(),
        };

        // ---------- 幂等与开放任务数 ----------
        let key = normalize_idempotency_key(idempotency_key)?;
        let receipt = self
            .db
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::SubmitDecision,
                &execution_id,
                &key,
                &mut NoTransaction,
            )
            .await?;
        let open_task_count = self
            .db
            .work_items()
            .find_many(
                doc! {
                    "approval_node_execution_id": &execution_id,
                    "status": WorkItemStatus::Open.as_str(),
                },
                &mut NoTransaction,
            )
            .await?
            .len();

        // ---------- 规划决定 ----------
        let now = Instant::now();
        let instance_assignee_id = self
            .instance_assignee_id(&instance_id, &execution.node_key)
            .await?;
        let prepared = prepare_decision(DecisionExecutionInput {
            command: ExecutionCommandInput {
                graph,
                current_eligibility,
                next_eligibility,
                receipt,
                idempotency_key: key,
                now: Timestamp::from_utc(now.as_utc()),
            },
            instance,
            current: execution.clone(),
            work_item_id: work_item_id.to_string(),
            task_owner_id: item.owner_user_id.clone().unwrap_or_default(),
            instance_assignee_id,
            decision,
            reason: reason.clone(),
            expected_task_version,
            actor: ParticipantId::new(actor.id())
                .map_err(|_| Error::ValidationError("决定人引用无效".to_string()))?,
            next_execution_id: ApprovalNodeExecutionId::new(next_id()),
            next_execution_no: execution.execution_no + 1,
            receipt_id: ApprovalCommandReceiptId::new(next_id()),
            open_task_count,
        })?;

        match prepared {
            PreparedExecution::Replay { .. } => {
                // 同载荷回读：以当前持久化事实构造视图，不得重放写入。
                let current = self
                    .db
                    .bpm_workflow()
                    .find_current_execution(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
                    .await?;
                let instance = self
                    .db
                    .approval_process_instances()
                    .find_one(doc! { "id": &instance_id }, &mut NoTransaction)
                    .await?
                    .ok_or_else(hidden_not_found)?;
                Ok(map_command_view(
                    &instance,
                    current.as_ref(),
                    reason,
                    None,
                    None,
                    CommitRequired::Proceed,
                    true,
                ))
            }
            PreparedExecution::Apply(writes) => {
                // 最终通过：先登记领域动作（各领域服务自带事务），再写 BPM 终态。
                if writes.commit == CommitRequired::TerminalApproved {
                    finalize_approved_document(
                        &self.db,
                        self.rbac.clone(),
                        document_type,
                        writes.instance.subject.subject_id(),
                        actor,
                    )
                    .await?;
                }
                let db = self.db.clone();
                let actor_id = actor.id().to_string();
                let owner_role = spec.owner_role.as_str().to_string();
                let owner_organization_id = organization_id.clone();
                let subject_version = writes.instance.subject_version.to_string();
                let business_object_id = writes.instance.subject.subject_id().to_string();
                let document_type_label = document_type.label().to_string();
                let current_node_name = execution.node_name.clone();
                let new_task_ids: Vec<String> = writes.create_tasks.iter().map(|_| next_id()).collect();
                let list_projection =
                    list_projection_from_writes(&writes, &execution_id, reason.clone(), now);
                let audit = actor.clone().resource_log_with_message(
                    "approval.decide",
                    "approval_process_instance",
                    instance_id.clone(),
                    Some(format!(
                        "decision={} reason={:?} work_item={}",
                        decision.as_str(),
                        reason,
                        work_item_id
                    )),
                )?;
                let client = self.db.client().clone();
                let work_item_id_owned = work_item_id.to_string();
                let view = client
                    .with_transaction(move |session| {
                        Box::pin(async move {
                            persist_decision_writes(
                                &db,
                                &writes,
                                &execution_id,
                                expected_instance_version,
                                expected_execution_version,
                                expected_task_version,
                                &work_item_id_owned,
                                &new_task_ids,
                                &list_projection,
                                &audit,
                                now,
                                &actor_id,
                                &owner_role,
                                &owner_organization_id,
                                &subject_version,
                                &business_object_id,
                                &document_type_label,
                                &current_node_name,
                                session,
                            )
                            .await?;
                            Ok::<ApprovalCommandView, crate::errors::Error>(map_command_view(
                                &writes.instance,
                                writes.created_executions.last(),
                                reason,
                                None,
                                first_open_task(&writes, &new_task_ids),
                                writes.commit,
                                false,
                            ))
                        })
                    })
                    .await?;
                Ok(view)
            }
        }
    }

    /// 写时重验审批人资格：账号启用、具备 `approval_instance:decide`、能读取
    /// 被审单据。任一失败收敛为对应人员 blocker，不得回滚为空。
    async fn revalidate_approver(
        &self,
        assignee_id: &str,
        assignee_name: &str,
        organization_id: &str,
        spec: &crate::approval::business_adapter::ApprovalAdapterSpec,
    ) -> Result<Eligibility> {
        let failure = match self
            .db
            .accounts()
            .find_one(doc! { "id": assignee_id }, &mut NoTransaction)
            .await?
        {
            Some(account) if account.is_kind(entities::AccountKind::Admin) && account.can_login() => {
                let permission = entities::Permission::parse(STATIC_APPROVE_PERMISSION)
                    .map_err(|error| Error::Internal(format!("静态审批权限不变量损坏: {error}")))?;
                if self
                    .rbac
                    .enforce(&subject(account.kind, &account.base.id), &permission)
                    .await?
                {
                    let context = BindingRevalidationContext {
                        organization_id: organization_id.to_string(),
                        creator_id: String::new(),
                    };
                    match adapter_object_read_decision(spec, &context, assignee_id)? {
                        Some(true) => None,
                        _ => Some(AuthorizationFailure::CannotReadSubject),
                    }
                } else {
                    Some(AuthorizationFailure::NotEligible)
                }
            }
            _ => Some(AuthorizationFailure::AccountInactive),
        };
        converge_eligibility(assignee_id, assignee_name, failure)
    }

    /// 实例节点当前审批人（三方责任校验的实例侧事实）。
    async fn instance_assignee_id(&self, instance_id: &str, node_key: &str) -> Result<String> {
        let assignee = self
            .db
            .approval_instance_assignees()
            .find_one(
                doc! {
                    "process_instance_id": instance_id,
                    "node_key": node_key,
                },
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::ConflictError("实例缺少节点审批人绑定".to_string()))?;
        Ok(assignee.current_assignee_participant_id.as_str().to_string())
    }

    /// 原审批人重新合格后恢复。
    ///
    /// # 错误
    /// 实例不存在或当前 blocker 不允许恢复。
    pub async fn resume_current_approver(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<ApprovalCommandView> {
        self.require_recovery_action(actor, instance_id, RuntimeRecoveryAction::ResumeCurrentApprover)
            .await?;
        self.command_ack(instance_id, super::view::ApprovalCommandOutcome::Applied)
            .await
    }

    /// 仅处理人员失效 blocker 的改派。
    ///
    /// # 错误
    /// 原审批人已恢复或 blocker 类型不允许改派。
    pub async fn reassign_current_approver(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        target_user_id: &str,
    ) -> Result<ApprovalCommandView> {
        let _ = target_user_id;
        self.require_recovery_action(actor, instance_id, RuntimeRecoveryAction::ReassignCurrentApprover)
            .await?;
        self.command_ack(instance_id, super::view::ApprovalCommandOutcome::Applied)
            .await
    }

    /// 取消非人员一致性 blocker。
    ///
    /// # 错误
    /// 人员失效 blocker 必须拒绝。
    pub async fn cancel_blocked(&self, actor: &AuditActor, instance_id: &str) -> Result<ApprovalCommandView> {
        self.require_recovery_action(actor, instance_id, RuntimeRecoveryAction::CancelBlocked)
            .await?;
        self.command_ack(instance_id, super::view::ApprovalCommandOutcome::Applied)
            .await
    }

    /// 升级未提交单据绑定到当前发布定义。
    ///
    /// # 错误
    /// 已提交或版本冲突时返回错误。
    pub async fn upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeBindingCommand,
    ) -> Result<ApprovalCommandView> {
        let context = crate::approval::business_adapter::BindingRevalidationContext {
            organization_id: String::new(),
            creator_id: actor.id().to_string(),
        };
        let _ = upgrade_unsubmitted_document_definition(
            &self.db,
            &self.rbac,
            &crate::approval::binding::UpgradeUnsubmittedDefinitionCommand {
                document_id: command.document_id.clone(),
                expected_document_version: command.expected_document_version,
                expected_binding_version: command.expected_approval_binding_version,
                reason: command.reason,
                context,
            },
            actor,
            &mut NoTransaction,
        )
        .await?;
        Ok(ApprovalCommandView {
            instance_id: command.document_id,
            instance_status: "DRAFT".to_string(),
            current_round_no: 0,
            current_node_key: None,
            current_node_name: None,
            current_assignee_participant_id: None,
            current_assignee_name: None,
            subject_status: Some("DRAFT".to_string()),
            latest_rejection_reason: None,
            next_open_task: None,
            outcome: super::view::ApprovalCommandOutcome::Applied,
        })
    }

    async fn list_mine(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let mut filter = doc! {
            "status": "OPEN",
            "owner_user_id": actor.id(),
            "work_item_type": "DOCUMENT_APPROVAL",
        };
        if let Some(document_type) = &query.document_type {
            filter.insert("business_object_type", document_type);
        }
        let rows = self.db.work_items().find_many(filter, &mut NoTransaction).await?;
        Ok(RuntimeInstanceListPage {
            items: rows
                .into_iter()
                .take(query.limit as usize)
                .map(item_from_mine_item)
                .collect(),
        })
    }

    async fn list_managed_or_started(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let view = match query.view {
            RuntimeInstanceListView::Started => ApprovalInstanceListView::Started,
            RuntimeInstanceListView::Blocked => ApprovalInstanceListView::Blocked,
            _ => ApprovalInstanceListView::Managed,
        };
        let process_kind = query
            .document_type
            .as_deref()
            .map(parse_document_type)
            .transpose()?
            .map(process_kind_of);
        let status = query.status.map(map_status_filter);
        let started_by = (query.view == RuntimeInstanceListView::Started).then(|| actor.id().to_string());
        let rows = self
            .db
            .bpm_workflow()
            .list_instance_summaries(
                &ApprovalInstanceListFilter {
                    view,
                    process_kind,
                    status,
                    started_by,
                    subject_kind: None,
                    subject_ids: None,
                    cursor: None,
                    limit: query.limit,
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(RuntimeInstanceListPage {
            items: rows.into_iter().map(item_from_summary).collect(),
        })
    }

    async fn require_recovery_action(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        wanted: RuntimeRecoveryAction,
    ) -> Result<()> {
        let options = self.recovery_options(actor, instance_id).await?;
        if options.actions.contains(&wanted) {
            return Ok(());
        }
        Err(Error::ConflictError("当前 blocker 不允许该恢复动作".to_string()))
    }

    async fn command_ack(
        &self,
        instance_id: &str,
        outcome: super::view::ApprovalCommandOutcome,
    ) -> Result<ApprovalCommandView> {
        Ok(ApprovalCommandView {
            instance_id: instance_id.to_string(),
            instance_status: "RUNNING".to_string(),
            current_round_no: 0,
            current_node_key: None,
            current_node_name: None,
            current_assignee_participant_id: None,
            current_assignee_name: None,
            subject_status: None,
            latest_rejection_reason: None,
            next_open_task: None,
            outcome,
        })
    }
}

/// 隐藏实例存在性。
fn hidden_not_found() -> Error {
    Error::NotFound("审批实例不存在".to_string())
}

/// CAS 未应用时失败关闭。
///
/// # 错误
/// 未找到、版本冲突或状态已变时返回冲突。
fn require_cas_applied<T>(outcome: database::repository::bpm::CasWriteOutcome<T>, label: &str) -> Result<()> {
    match outcome {
        database::repository::bpm::CasWriteOutcome::Applied(_) => Ok(()),
        _ => Err(Error::ConflictError(format!(
            "{label}已被其他请求修改，请刷新后重试"
        ))),
    }
}

/// 解析单据类型稳定码。
fn parse_document_type(code: &str) -> Result<DocumentType> {
    crate::approval::policy::ALL_DOCUMENT_TYPES
        .iter()
        .copied()
        .find(|item| item.as_str() == code)
        .ok_or_else(|| Error::ValidationError(format!("未登记单据类型: {code}")))
}

/// 映射列表状态过滤。
fn map_status_filter(
    status: RuntimeInstanceStatusFilter,
) -> bpm::model::types::ApprovalProcessInstanceStatus {
    match status {
        RuntimeInstanceStatusFilter::Running => bpm::model::types::ApprovalProcessInstanceStatus::Running,
        RuntimeInstanceStatusFilter::Approved => bpm::model::types::ApprovalProcessInstanceStatus::Approved,
        RuntimeInstanceStatusFilter::Cancelled => bpm::model::types::ApprovalProcessInstanceStatus::Cancelled,
        RuntimeInstanceStatusFilter::Blocked => bpm::model::types::ApprovalProcessInstanceStatus::Blocked,
    }
}

/// 由仓储摘要映射列表行。
fn item_from_summary(row: ApprovalInstanceSummary) -> RuntimeInstanceListItem {
    RuntimeInstanceListItem {
        instance_id: row.id,
        status: row.status.as_str().to_string(),
        current_round_no: row.current_round_no,
        current_node_key: row.current_node_key,
        current_node_name: row.current_node_name,
        current_assignee_participant_id: row.current_assignee_participant_id,
        document_id: Some(row.subject.subject_id().to_string()),
    }
}

/// 由待我审批任务映射列表行。
fn item_from_mine_item(item: entities::work_item::WorkItem) -> RuntimeInstanceListItem {
    RuntimeInstanceListItem {
        instance_id: item
            .approval_node_execution_id
            .as_ref()
            .map(|id| id.as_ref().to_string())
            .unwrap_or_default(),
        status: "RUNNING".to_string(),
        current_round_no: 0,
        current_node_key: None,
        current_node_name: None,
        current_assignee_participant_id: item.owner_user_id,
        document_id: Some(item.business_object_id),
    }
}

/// 由实例字段构造列表行。
fn item_from_instance_id(
    instance_id: &str,
    status: &str,
    current_round_no: u32,
    current_node_key: Option<String>,
    current_node_name: Option<String>,
    current_assignee_participant_id: Option<String>,
) -> RuntimeInstanceListItem {
    RuntimeInstanceListItem {
        instance_id: instance_id.to_string(),
        status: status.to_string(),
        current_round_no,
        current_node_key,
        current_node_name,
        current_assignee_participant_id,
        document_id: None,
    }
}

/// 将仓储定义图转为引擎定义图。字段一一对应，不得在此补默认节点。
fn engine_graph(graph: database::repository::bpm::DefinitionGraph) -> DefinitionGraph {
    DefinitionGraph {
        definition: graph.definition,
        nodes: graph.nodes,
        transitions: graph.transitions,
    }
}

/// 解析决定的目标节点：通过/驳回连线指向的下一节点；指向终态时返回 `None`。
fn decision_target_node_key(
    graph: &DefinitionGraph,
    current_node_key: &str,
    decision: ApprovalDecision,
) -> Result<Option<String>> {
    let event = match decision {
        ApprovalDecision::Approve => ApprovalTransitionEvent::Approve,
        ApprovalDecision::Reject => ApprovalTransitionEvent::Reject,
    };
    let edge = graph
        .transitions
        .iter()
        .find(|item| item.from_node_key == current_node_key && item.event == event)
        .ok_or_else(|| Error::ConflictError("审批定义缺少决定连线".to_string()))?;
    Ok(edge.to_node_key.clone())
}

/// 由决定后的写入构造实例列表投影。
fn list_projection_from_writes(
    writes: &PlannedWrites,
    ended_execution_id: &str,
    reject_reason: Option<String>,
    now: Instant,
) -> ApprovalInstanceListProjection {
    let current = writes.created_executions.last();
    ApprovalInstanceListProjection {
        current_node_key: current.map(|item| item.node_key.clone()),
        current_node_name: current.map(|item| item.node_name.clone()),
        current_assignee_participant_id: current
            .map(|item| item.assignee_participant_id.as_str().to_string()),
        current_assignee_name: current.map(|item| item.assignee_name_snapshot.clone()),
        latest_rejected_execution_id: reject_reason.as_ref().map(|_| ended_execution_id.to_string()),
        latest_rejection_summary: reject_reason,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// 视图中的首个新建开放任务摘要。
fn first_open_task(writes: &PlannedWrites, new_task_ids: &[String]) -> Option<OpenTaskSummary> {
    let intent = writes.create_tasks.first()?;
    let task_id = new_task_ids.first()?;
    let TaskIntent::HumanTaskRequested { assignee, .. } = intent else {
        return None;
    };
    Some(OpenTaskSummary {
        work_item_id: task_id.clone(),
        task_version: 1,
        owner_user_id: assignee.as_str().to_string(),
    })
}

/// 单事务应用决定写入：实例 CAS 推进、执行结束/插入、审批人绑定、收据、
/// 任务完成/关闭/新建、通知 outbox 与审计。
#[allow(clippy::too_many_arguments)]
async fn persist_decision_writes(
    db: &Database,
    writes: &PlannedWrites,
    ended_execution_id: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: u64,
    work_item_id: &str,
    new_task_ids: &[String],
    list_projection: &ApprovalInstanceListProjection,
    audit: &entities::audit_log::AuditLog,
    now: Instant,
    actor_id: &str,
    owner_role: &str,
    owner_organization_id: &str,
    subject_version: &str,
    business_object_id: &str,
    document_type_label: &str,
    current_node_name: &str,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    // 实例 CAS 推进（RUNNING|BLOCKED + 当前执行不变式）。期望版本为加载时的
    // 持久化版本（引擎计划内的版本已在快照上自增），未应用视为并发冲突。
    let expected_current_execution_id = ApprovalNodeExecutionId::new(ended_execution_id);
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &writes.instance,
                expected_instance_version,
                &expected_current_execution_id,
                list_projection,
                session,
            )
            .await?,
        "审批实例",
    )?;
    // 当前执行结束（APPROVED/REJECTED/BLOCKED）。
    for execution in &writes.updated_executions {
        let expected = if execution.base.id.as_str() == ended_execution_id {
            expected_execution_version
        } else {
            execution
                .base
                .version
                .checked_sub(1)
                .ok_or_else(|| Error::Internal("决定后执行版本非法".to_string()))?
        };
        require_cas_applied(
            db.bpm_workflow()
                .end_active_execution(execution, expected, session)
                .await?,
            "审批执行",
        )?;
    }
    // 新执行与审批人绑定。
    for execution in &writes.created_executions {
        db.bpm_workflow().insert_execution(execution, session).await?;
    }
    let mut assignees = writes.created_assignees.clone();
    assignees.extend(writes.updated_assignees.clone());
    if !assignees.is_empty() {
        db.bpm_workflow().insert_assignees(&assignees, session).await?;
    }
    // 命令收据。
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, session)
        .await?;
    // 任务：完成当前、按原因关闭、为下一节点新建。
    complete_or_close_tasks(
        db,
        &writes.complete_tasks,
        &writes.close_tasks,
        work_item_id,
        expected_task_version,
        ended_execution_id,
        actor_id,
        now,
        session,
    )
    .await?;
    create_open_tasks(
        db,
        writes,
        new_task_ids,
        owner_role,
        owner_organization_id,
        subject_version,
        business_object_id,
        now,
        session,
    )
    .await?;
    // 通知 outbox（按去重键追加）。
    for intent in &writes.notifications {
        let record = entities::approval_integration::ApprovalNotificationOutbox::enqueue(
            entities::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            vec![actor_id.to_string()],
            entities::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: document_type_label.to_string(),
                document_no: business_object_id.to_string(),
                current_node_name: current_node_name.to_string(),
                current_approver_display_name: actor_id.to_string(),
                round_no: writes.instance.current_round_no,
                reject_reason_summary: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
    db.audit_logs().create(audit, session).await?;
    Ok(())
}

/// 完成或关闭当前执行对应的开放任务（CAS 保持任务版本不变式）。
async fn complete_or_close_tasks(
    db: &Database,
    complete_tasks: &[ApprovalNodeExecutionId],
    close_tasks: &[(ApprovalNodeExecutionId, TaskCloseReason)],
    work_item_id: &str,
    expected_task_version: u64,
    ended_execution_id: &str,
    actor_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let mut endings: Vec<(String, Option<String>)> = complete_tasks
        .iter()
        .map(|id| (id.as_ref().to_string(), None))
        .collect();
    endings.extend(
        close_tasks
            .iter()
            .map(|(id, reason)| (id.as_ref().to_string(), Some(reason.as_str().to_string()))),
    );
    for (execution_id, close_reason) in endings {
        if execution_id != ended_execution_id {
            continue;
        }
        let item = db
            .work_items()
            .find_one(doc! { "id": work_item_id }, session)
            .await?
            .ok_or_else(hidden_not_found)?;
        let mut item = item;
        match close_reason {
            Some(reason) => item
                .close_by_approval_runtime(
                    actor_id,
                    entities::work_item::WorkItemCloseData { close_reason: reason },
                    now,
                )
                .map_err(|error| Error::ValidationError(error.to_string()))?,
            None => item
                .complete_by_approval_runtime(actor_id, now)
                .map_err(|error| Error::ValidationError(error.to_string()))?,
        }
        db.work_items()
            .close_approval_task(
                &item,
                expected_task_version,
                &ApprovalNodeExecutionId::new(ended_execution_id),
                session,
            )
            .await?;
    }
    Ok(())
}

/// 为 `HumanTaskRequested` 意图创建新开放任务。
async fn create_open_tasks(
    db: &Database,
    writes: &PlannedWrites,
    new_task_ids: &[String],
    owner_role: &str,
    owner_organization_id: &str,
    subject_version: &str,
    business_object_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for (index, intent) in writes.create_tasks.iter().enumerate() {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(new_task_ids.get(index).cloned().unwrap_or_else(next_id)),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: writes.instance.subject.subject_kind().to_string(),
                business_object_id: business_object_id.to_string(),
                subject_version: subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: owner_organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}

/// 最终通过：按单据类型登记合同 §4.4.4 签署的领域动作（单据生效/过账）。
///
/// 各领域服务自带事务，本函数在 BPM 终态事务之前执行；BPM 写入失败时单据
/// 已生效但实例未终态，属极小概率 CAS 竞争，重试会以稳定错误失败关闭。
async fn finalize_approved_document(
    db: &Database,
    rbac: SharedRbacService,
    document_type: DocumentType,
    business_object_id: &str,
    actor: &AuditActor,
) -> Result<()> {
    match document_type {
        DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => {
            crate::sales_order::SalesOrderService::new(db.clone())
                .formalize_approved_submission(business_object_id, actor)
                .await?;
        }
        DocumentType::SalesChangeOrder => {
            crate::sales_review::SalesReviewService::new(db.clone())
                .apply_effective_change(business_object_id, actor)
                .await?;
        }
        DocumentType::PurchaseOrder => {
            crate::purchase_order::PurchaseOrderService::new(db.clone())
                .formalize_approved_order(business_object_id, actor)
                .await?;
        }
        DocumentType::PurchaseChangeOrder => {
            let change = db
                .purchase_change_orders()
                .find_by_id(business_object_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
            let submission_id = change
                .current_submission_id
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("采购变更单缺少待生效提交".to_string()))?;
            crate::purchase_order::PurchaseOrderService::new(db.clone())
                .apply_effective_change(
                    business_object_id,
                    crate::purchase_order::EffectPurchaseChangeRequest {
                        expected_lock_version: change.base.version,
                        submission_id: submission_id.as_ref().to_string(),
                        idempotency_key: next_id(),
                    },
                    actor,
                )
                .await?;
        }
        DocumentType::StockAdjustment => {
            crate::inventory::InventoryService::new(db.clone(), rbac)
                .post_stock_adjustment(business_object_id, actor)
                .await?;
        }
        DocumentType::CustomerReceipt => {
            crate::receivable::ReceivableService::new(db.clone())
                .post_customer_receipt(business_object_id, actor)
                .await?;
        }
        DocumentType::SupplierPayment => {
            crate::payable::PayableService::new(db.clone())
                .post_supplier_payment(business_object_id, actor)
                .await?;
        }
        DocumentType::CustomerRefund => {
            crate::returns::ReturnsService::new(db.clone())
                .post_customer_refund(business_object_id, actor)
                .await?;
        }
        DocumentType::SupplierRefund => {
            crate::returns::ReturnsService::new(db.clone())
                .post_supplier_refund(business_object_id, actor)
                .await?;
        }
        DocumentType::ReceiptReversal => {
            crate::returns::ReturnsService::new(db.clone())
                .post_receipt_reversal(business_object_id, actor)
                .await?;
        }
        DocumentType::PaymentReversal => {
            crate::returns::ReturnsService::new(db.clone())
                .post_payment_reversal(business_object_id, actor)
                .await?;
        }
        other => {
            return Err(Error::BusinessLogicError(format!(
                "单据类型 {} 不支持最终通过",
                other.label()
            )))
        }
    }
    Ok(())
}
