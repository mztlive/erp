//! 开放任务转交、候选人资格与岗位分离。

use std::collections::HashSet;
use std::future::Future;
use std::result::Result as StdResult;
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::AccountKind;
use erp_identity::AccessTarget;
use erp_permissions::{internal, work_item};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::candidate_facts_current;
use super::order_access::task_read_error;
use super::reassign_apply::reassignment_audits;
use super::write::{
    IDEMPOTENCY_AUDIT_PREFIX, WorkItemWriteError, WorkItemWriteOutcome, expected_task_version, required_text,
};
use super::{
    ReassignWorkItemRequest, WorkItemMutationOutcome, WorkItemReassignCandidateView, WorkItemService,
};
use crate::entity::work_item::{FulfillmentResponsibilityKey, WorkItem, WorkItemAssignmentSeparationPolicy};
use crate::error::{Error, Result};
use crate::ports::{PreparedWorkflowAudit, WorkflowAuthorizationPort};

/// 服务端规范化的转交输入，不保存事务外任务或授权结果。
struct ReassignCommand {
    id: String,
    actor: AuditActor,
    expected_version: u64,
    target_user_id: String,
    reason: String,
    receipt: CommandReceipt,
    audit: PreparedWorkflowAudit,
}

/// 转交命令的任务、目标责任人与审计输入。
///
/// # 用途
/// 将转交事务所需字段打包，供 [`WorkItemService::reassign_with_assignment_policy_audit`] 使用。
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
/// 管理人、目标责任人与业务事实必须沿原事务核验。
pub(super) struct AssignmentPolicyAuditInput<'a> {
    /// 待转交任务。
    pub(super) item: WorkItem,
    /// 期望任务版本。
    pub(super) expected_task_version: u64,
    /// 目标责任人。
    pub(super) target_user_id: String,
    /// 操作人。
    pub(super) actor: &'a AuditActor,
    /// 强类型幂等命令收据。
    pub(super) receipt: CommandReceipt,
    /// 权限安全的转交说明。
    pub(super) audit_detail: String,
    /// 原事务内读取的目标账号类型。
    pub(super) assignee_kind: AccountKind,
}

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 管理端选择新的责任人时，列出当前开放非审批任务可转交的有效账号。采购单候选人还须能处理该单全部开放履约任务。
    ///
    /// 列表只供选择，不能作为写入资格；转交时必须在写事务重验任务、账号和目标业务资格。任务缺失、不属于开放的非审批任务、超出管理范围或取证失败时拒绝。
    pub async fn reassign_candidates(
        self,
        id: String,
        actor: AuditActor,
    ) -> Result<Vec<WorkItemReassignCandidateView>> {
        let db = self.db.clone();
        db.client()
            .with_transaction(move |executor| {
                Box::pin(async move { self.candidate_list(&id, &actor, executor).await })
            })
            .await
    }

    // 在原管理快照读取任务及级联履约范围，按完整执行资格筛出候选人并排除当前负责人。候选筛选可能跨有效期，交付前重新检查精确任务管理权。
    async fn candidate_list(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemReassignCandidateView>> {
        let item = self.managed_item(id, actor, executor).await?;
        let tasks = self.cascade_tasks(&item, executor).await?;

        let accounts = self.identity.directory().accounts_by_kind(AccountKind::Admin, executor).await?;
        let mut candidates = Vec::new();
        for account in accounts {
            if item.owner_user_id.as_deref() == Some(account.id.as_str()) {
                continue;
            }
            match self.candidate_allowed(&account.id, &item, tasks.as_deref(), executor).await {
                Ok(()) => {},
                Err(Error::Forbidden(_)) => continue,
                Err(error) => return Err(error),
            }
            candidates.push(WorkItemReassignCandidateView {
                user_id: account.id,
                display_name: account.display_name,
                account: account.login_account,
            });
        }

        candidates.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.account.cmp(&right.account))
                .then_with(|| left.user_id.cmp(&right.user_id))
        });

        // 候选逐项检查可能跨越授权到期边界，交付前重验管理目标。
        self.authorization.require(actor, &work_item::MANAGE, id, executor).await?;
        Ok(candidates)
    }

    // 检查候选人是否可承接当前任务；存在采购级联范围时，沿原执行器一并验证关联的全部开放履约任务。
    async fn candidate_allowed(
        &self,
        id: &str,
        item: &WorkItem,
        tasks: Option<&[WorkItem]>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.assignment_kind(id, item, executor).await?;
        if let Some(tasks) = tasks {
            ensure_fulfillment_tasks_candidate(self, tasks, id, executor).await?;
        }
        Ok(())
    }

    // 发现采购责任转交时读取该采购单全部开放履约任务，供候选选择和实际转交使用同一范围；其他任务不扩大为级联。
    async fn cascade_tasks(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<WorkItem>>> {
        let Some(order_id) = purchase_order_fulfillment_responsibility_id(item)? else { return Ok(None) };
        let (_, tasks) = self.facts.purchase_order_fulfillment_scope(item, &order_id, executor).await?;
        Ok(Some(tasks))
    }

    /// 按期望版本将开放非审批任务交给具备完整资格的目标责任人。写事务内重新确认管理范围、任务状态及目标资格，并按幂等命令处理回放。
    ///
    /// 返回可跨线程调度的 `Send` future；管理或目标资格不足、任务状态不支持或版本冲突时拒绝。
    #[expect(
        clippy::manual_async_fn,
        reason = "显式 Send 返回边界用于验证嵌套异步调用，避免调用方的高阶生命周期推导失败"
    )]
    pub fn reassign(
        self,
        id: String,
        req: ReassignWorkItemRequest,
        actor: AuditActor,
    ) -> impl Future<Output = Result<WorkItemMutationOutcome>> + Send {
        async move {
            let command = Arc::new(self.reassign_command(id, req, actor)?);
            let transaction_command = command.clone();
            let service = self.clone();
            let result = self
                .db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move { service.apply_reassign(&transaction_command, executor).await })
                })
                .await;
            self.finish_command(&command.receipt, &command.actor, &command.audit, result).await
        }
    }

    // 开启转交事务前规范化请求的版本、目标、幂等身份和安全审计；决定能否转交的事实留到写事务读取。
    fn reassign_command(
        &self,
        id: String,
        req: ReassignWorkItemRequest,
        actor: AuditActor,
    ) -> Result<ReassignCommand> {
        req.validate()?;
        let key = required_text(&req.idempotency_key, "缺少本次操作的提交编号，请重新发起操作")?;
        let target_user_id = required_text(&req.target_user_id, "目标用户不能为空")?;
        let reason = required_text(&req.reason, "转交原因不能为空")?;
        let expected_version = expected_task_version(&req.expected_task_version)?;

        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            "work_item.reassign",
            "work_item",
            &id,
            &key,
            [expected_version.to_string(), target_user_id.clone(), reason.clone()],
        )?;
        let (audit, _) = reassignment_audits(&actor, &receipt, &id, None)?;
        self.audit.validate(&audit)?;
        Ok(ReassignCommand { id, actor, expected_version, target_user_id, reason, receipt, audit })
    }

    // 在原转交写事务内核验精确管理目标、命令回放、任务版本和接收人资格，不能以候选列表先前的判断代替本次取证。
    async fn apply_reassign(
        &self,
        command: &ReassignCommand,
        executor: &mut dyn Executor,
    ) -> StdResult<WorkItemWriteOutcome, WorkItemWriteError> {
        let item = self.managed_item(&command.id, &command.actor, executor).await?;
        if let Some(replayed) = self.idempotent_replay(&command.receipt, &item, executor).await? {
            return Ok(WorkItemWriteOutcome::Updated(Box::new(replayed)));
        }

        if item.base.version != command.expected_version {
            return Ok(WorkItemWriteOutcome::VersionConflict);
        }

        let assignee_kind = self.assignment_kind(&command.target_user_id, &item, executor).await?;
        let item = self
            .reassign_with_assignment_policy_audit(
                AssignmentPolicyAuditInput {
                    item,
                    expected_task_version: command.expected_version,
                    target_user_id: command.target_user_id.clone(),
                    actor: &command.actor,
                    receipt: command.receipt.clone(),
                    audit_detail: command.reason.clone(),
                    assignee_kind,
                },
                executor,
            )
            .await?;
        Ok(WorkItemWriteOutcome::Updated(Box::new(item)))
    }

    // 在转交原事务读取目标账号的当前状态，同时核验该任务类型的完整办理资格；不使用事务外候选缓存。
    async fn assignment_kind(
        &self,
        assignee_id: &str,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<AccountKind> {
        let assignee = self
            .identity
            .directory()
            .account(assignee_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".into()))?;
        self.ensure_assignment_candidate(
            assignee_id,
            assignee.kind,
            item,
            item.owner_user_id.as_deref() == Some(assignee_id),
            executor,
        )
        .await?;
        Ok(assignee.kind)
    }

    // 在调用方事务快照中重验目标账号资格、对象访问权与岗位分离。
    // 目标账号仍有效、具备任务所需权限且满足岗位分离时返回 `Ok(())`。
    //
    // 账号失效、权限撤销、对象版本变化或岗位分离不满足时返回错误。
    //
    // `user_id`：待接收任务的具体账号 ID；`expected_kind`：原事务中读取的账号类型。
    // `allow_current_owner`：是否允许目标账号保持为当前负责人；`executor`：调用方事务执行器。
    pub(super) async fn ensure_assignment_candidate(
        &self,
        user_id: &str,
        expected_kind: AccountKind,
        item: &WorkItem,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let actor = AuditActor::new(user_id.into(), user_id.into(), expected_kind);
        self.authorization
            .service
            .require(
                &actor,
                &internal::WORK_ITEM_RECIPIENT,
                AccessTarget::Task { id: item.base.id.clone(), version: item.base.version.to_string() },
                executor,
            )
            .await?;

        self.ensure_assignment_candidate_access_with_executor(item, executor)
            .await
            .map_err(task_read_error)?;

        self.ensure_assignment_separation(user_id, item, allow_current_owner, executor).await
    }

    // 创建审批个人责任前核对岗位分离，排除启动人、已形成责任者和原先或当前实际决定人。
    async fn ensure_assignment_separation(
        &self,
        user_id: &str,
        item: &WorkItem,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match item.work_item_type.assignment_separation_policy() {
            WorkItemAssignmentSeparationPolicy::ApprovalHistory => {
                self.ensure_approval_assignment_separation(user_id, item, allow_current_owner, executor).await
            },
            WorkItemAssignmentSeparationPolicy::DomainActors => {
                let excluded = self.domain_assignment_actors(item, executor).await?;
                if excluded.iter().any(|actor_id| actor_id == user_id) {
                    return Err(Error::Forbidden("目标账号违反业务岗位分离约束".to_string()));
                }
                Ok(())
            },
            WorkItemAssignmentSeparationPolicy::RoleAndParticipation => Ok(()),
            WorkItemAssignmentSeparationPolicy::FailClosed => {
                Err(Error::Forbidden("任务类型未注册可证明的岗位分离策略".to_string()))
            },
        }
    }

    // 以实例启动人、业务提交人、持久化责任历史和决定历史核验审批候选，不借当前负责人投影替代历史。
    async fn ensure_approval_assignment_separation(
        &self,
        user_id: &str,
        item: &WorkItem,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let _ = (user_id, item, allow_current_owner, executor);
        Err(Error::Forbidden("单据审批任务不得通过通用责任入口改派".to_string()))
    }

    // 为非审批正式决定任务读取权威提交人、经办人和历史决定记录，供判断岗位分离。
    async fn domain_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let actors = self.facts.assignment_separation_actors(item, executor).await?;
        non_empty_assignment_actors(actors)
    }

    // 统一接收者授权后，沿原执行器核验冻结业务版本。
    // 供给分配任务按具体账号和 `purchase_order:create` 授权，不额外引入团队池或固定角色约束。
    //
    // 对象存在、版本匹配且目标账号满足任务类型的访问条件时返回 `Ok(())`。
    //
    // 对象未注册、不存在、版本变化或目标账号访问条件不足时返回错误。
    //
    // `executor`：调用方事务执行器。
    async fn ensure_assignment_candidate_access_with_executor(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let policy = item
            .work_item_type
            .brief_relation(&item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.facts.load_object_facts(&keys, executor).await?;
        if !candidate_facts_current(item, &facts) {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }
}

// 解析采购单履约任务冻结的采购责任键。
// 非采购履约任务返回空；采购履约任务返回采购单 ID。
//
// 对象类型、责任角色、原因码或责任键不符合固定履约合同时返回错误。
pub(super) fn purchase_order_fulfillment_responsibility_id(item: &WorkItem) -> Result<Option<String>> {
    let key = item.fulfillment_responsibility_key().map_err(|_| {
        Error::BusinessLogicError(
            "履约任务的对象、责任角色、原因码或责任键不一致，请联系管理员修复后重试".to_string(),
        )
    })?;
    Ok(match key {
        Some(FulfillmentResponsibilityKey::PurchaseOrder(id)) => Some(id),
        Some(
            FulfillmentResponsibilityKey::WarehouseReceipt(_)
            | FulfillmentResponsibilityKey::WarehouseShip(_),
        )
        | None => None,
    })
}
const SYSTEM_OBJECT_OWNER: &str = "__system__";

// 校验目标账号可执行给定全部开放履约任务。
// 每个任务都通过候选人访问校验时无返回值。空任务列表也通过。
//
// 任一任务的对象访问校验失败时返回对应错误。
//
// `tasks`：待校验的开放履约任务。
pub(super) async fn ensure_fulfillment_tasks_candidate<A: WorkflowAuthorizationPort>(
    service: &WorkItemService<A>,
    tasks: &[WorkItem],
    target_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let actor = AuditActor::new(target_user_id.into(), target_user_id.into(), AccountKind::Admin);
    for task in tasks {
        service
            .authorization
            .service
            .require(
                &actor,
                &internal::WORK_ITEM_RECIPIENT,
                AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
                executor,
            )
            .await?;
        service.ensure_assignment_candidate_access_with_executor(task, executor).await?;
    }
    Ok(())
}

// 当审批候选人避开启动人、提交人、既有责任人和决定人时返回 `true`。
// 候选人不是启动人、提交人或决定人，且既有责任人中的同名账号仅在允许当前负责人时被放过，则返回 `true`。
//
// `responsibility_actor_ids`：既有责任人；`current_owner_user_id`：当前负责人。
// `allow_current_owner`：是否允许候选人保持为当前负责人。
#[cfg(test)]
pub(super) fn approval_assignment_separated(
    candidate_id: &str,
    started_by: &str,
    submitted_by: &str,
    responsibility_actor_ids: &[String],
    current_owner_user_id: Option<&str>,
    allow_current_owner: bool,
    decided_by: &[&str],
) -> bool {
    if candidate_id == started_by || candidate_id == submitted_by {
        return false;
    }
    if responsibility_actor_ids.iter().any(|actor_id| {
        actor_id == candidate_id && !(allow_current_owner && current_owner_user_id == Some(candidate_id))
    }) {
        return false;
    }
    !decided_by.contains(&candidate_id)
}

/// 工作项责任形成使用的实体岗位分离策略别名。
#[cfg(test)]
pub(super) type AssignmentSeparationPolicy = WorkItemAssignmentSeparationPolicy;

// 按资源逐一证明正式票款审计，并返回创建、过账或红冲等经办人。
// 每个资源都有成功的正式动作时，返回匹配经办前缀的操作人。
//
// 任一资源缺少成功的正式登记审计时返回禁止。
//
// `resource_ids`：必须逐一证明的资源 ID；`audits`：已读取的审计事实。
// `operator_actions`：经办动作前缀；`formal_actions`：正式登记动作前缀。
#[cfg(test)]
pub(super) fn audited_fact_operator_actors(
    resource_type: &str,
    resource_ids: &HashSet<String>,
    audits: &[crate::ports::WorkflowAuditFact],
    operator_actions: &[&str],
    formal_actions: &[&str],
) -> Result<Vec<String>> {
    let matches_action =
        |action: &str, prefixes: &[&str]| prefixes.iter().any(|prefix| action.starts_with(prefix));
    let mut actors = Vec::new();
    for resource_id in resource_ids {
        let facts = audits
            .iter()
            .filter(|audit| {
                audit.success
                    && audit.resource_type == resource_type
                    && audit.resource_id.as_deref() == Some(resource_id.as_str())
            })
            .collect::<Vec<_>>();
        if !facts.iter().any(|audit| matches_action(&audit.action, formal_actions)) {
            return Err(Error::Forbidden("无法从审计事实证明票款已经正式登记，任务分派失败关闭".to_string()));
        }
        actors.extend(
            facts
                .into_iter()
                .filter(|audit| matches_action(&audit.action, operator_actions))
                .map(|audit| audit.actor_id.clone()),
        );
    }
    Ok(actors)
}

// 去掉空白和系统占位后，要求岗位分离仍有权威责任人。
// 返回去空白、去空值并排除 `__system__` 后的非空集合。
//
// 过滤后没有责任人时返回禁止。
//
// `actors`：候选责任人 ID。
pub(super) fn non_empty_assignment_actors(actors: Vec<String>) -> Result<HashSet<String>> {
    let actors = actors
        .into_iter()
        .map(|actor| actor.trim().to_string())
        .filter(|actor| !actor.is_empty() && actor != SYSTEM_OBJECT_OWNER)
        .collect::<HashSet<_>>();
    if actors.is_empty() {
        return Err(Error::Forbidden("任务岗位分离所需的权威责任人事实缺失".to_string()));
    }
    Ok(actors)
}

#[cfg(test)]
mod transaction_tests {
    use erp_core::ids::WorkItemId;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::testing::unavailable_service;
    use super::*;
    use crate::entity::work_item::SupplierPortalReviewTaskData;

    #[tokio::test]
    async fn recipient_checks_exact_persisted_task_before_loading_business_sources() {
        let service = unavailable_service();
        let item = WorkItem::new_supplier_portal_review(
            WorkItemId::new("task"),
            SupplierPortalReviewTaskData {
                request_id: "request".into(),
                subject_version: "submission-1".into(),
                owner_user_id: "old-owner".into(),
                owner_organization_id: "org".into(),
                due_at: None,
                impact_summary: None,
            },
        )
        .unwrap();
        let error = service
            .ensure_assignment_candidate("recipient", AccountKind::Admin, &item, false, &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
        let error = service
            .ensure_assignment_candidate(
                "recipient",
                AccountKind::Admin,
                &item,
                false,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn reassign_uses_the_original_snapshot_and_stops_before_task_or_receipt_reads_on_denial() {
        let service = unavailable_service();
        let actor = AuditActor::new("manager".into(), "manager".into(), AccountKind::Admin);
        let command = service
            .reassign_command(
                "task".into(),
                ReassignWorkItemRequest {
                    expected_task_version: "1".into(),
                    target_user_id: "target".into(),
                    reason: "reason".into(),
                    idempotency_key: "key".into(),
                },
                actor,
            )
            .unwrap();
        let error = service.apply_reassign(&command, &mut NoTransaction).await.err().unwrap();
        assert!(matches!(error, WorkItemWriteError::Service(Error::ValidationError(_))));
        let error = service.apply_reassign(&command, &mut SnapshotExecutor::default()).await.err().unwrap();
        assert!(
            matches!(error, WorkItemWriteError::Service(Error::Internal(reason)) if reason == "测试授权设施不可用")
        );
    }

    #[tokio::test]
    async fn candidate_queries_require_the_same_snapshot_before_reading_accounts() {
        let service = unavailable_service();
        let actor = AuditActor::new("manager".into(), "manager".into(), AccountKind::Admin);
        let error =
            service.candidate_list("task", &actor, &mut SnapshotExecutor::default()).await.err().unwrap();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }
}
