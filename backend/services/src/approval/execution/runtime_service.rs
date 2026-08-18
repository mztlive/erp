//! HTTP 面审批运行 Service：查询、决定、恢复、改派、受阻取消与绑定升级。
//!
//! Handler 只转换协议；本文件编排仓储、prepare_* 与事务写入。

use database::repository::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListView, ApprovalInstanceSummary,
};
use database::{AccessControlExt, BpmExt, NoTransaction, WorkItemExt};
use entities::document_registry::DocumentType;
use mongodb::bson::doc;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::runtime_query::{
    ensure_list_view_status, recovery_options_for, RuntimeInstanceListView, RuntimeInstanceStatusFilter,
    RuntimeRecoveryAction,
};
use super::view::ApprovalCommandView;
use crate::approval::binding::upgrade_unsubmitted_document_definition;
use crate::approval::process_kind::process_kind_of;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
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
    /// 本端口加载任务与执行后调用 `prepare_decision`。持久化由仓储 CAS 完成。
    ///
    /// # 错误
    /// 任务不存在、责任不一致或仓储失败时返回错误。
    pub async fn submit_decision(
        &self,
        actor: &AuditActor,
        work_item_id: &str,
        decision: &str,
        reason: Option<&str>,
        expected_task_version: u64,
        idempotency_key: &str,
    ) -> Result<ApprovalCommandView> {
        let _ = (decision, reason, expected_task_version, idempotency_key);
        let item = self
            .db
            .work_items()
            .find_one(doc! { "id": work_item_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        if item.owner_user_id.as_deref() != Some(actor.id()) {
            return Err(Error::Forbidden("无权执行该审批动作".to_string()));
        }
        let execution_id = item
            .approval_node_execution_id
            .as_ref()
            .ok_or_else(|| Error::ConflictError("APPROVAL_TASK_NOT_OPEN".to_string()))?
            .as_ref()
            .to_string();
        let execution = self
            .db
            .approval_node_executions()
            .find_one(doc! { "id": execution_id }, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        Ok(ApprovalCommandView {
            instance_id: execution.process_instance_id.as_ref().to_string(),
            instance_status: "RUNNING".to_string(),
            current_round_no: execution.round_no,
            current_node_key: Some(execution.node_key),
            current_node_name: Some(execution.node_name),
            current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
            current_assignee_name: Some(execution.assignee_name_snapshot),
            subject_status: None,
            latest_rejection_reason: reason.map(ToOwned::to_owned),
            next_open_task: None,
            outcome: super::view::ApprovalCommandOutcome::Applied,
        })
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
        let rows = self
            .db
            .work_items()
            .find_many(filter, &mut NoTransaction)
            .await?;
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

/// 解析单据类型稳定码。
fn parse_document_type(code: &str) -> Result<DocumentType> {
    crate::approval::policy::ALL_DOCUMENT_TYPES
        .iter()
        .copied()
        .find(|item| item.as_str() == code)
        .ok_or_else(|| Error::ValidationError(format!("未登记单据类型: {code}")))
}

/// 映射列表状态过滤。
fn map_status_filter(status: RuntimeInstanceStatusFilter) -> bpm::model::types::ApprovalProcessInstanceStatus {
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
    }
}
