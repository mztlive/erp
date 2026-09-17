//! 开放任务转交、候选人资格与岗位分离。

use std::collections::HashSet;
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::common::time::Instant;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::access::{
    ActorAccess, MANAGE_PERMISSION, active_role_ids, ensure_generic_work_item_mutation,
    ensure_item_in_managed_scope, ensure_managed_access, has_assignment_candidate_access, object_policy,
};
use super::order_access::{require_order_task_read, task_read_error};
use super::write::{IDEMPOTENCY_AUDIT_PREFIX, WorkItemWriteOutcome, expected_task_version, required_text};
use super::{
    ReassignWorkItemRequest, WorkItemConflictKind, WorkItemMutationOutcome, WorkItemReassignCandidateView,
    WorkItemService,
};
use crate::entity::work_item::{
    AvailableWorkItemAccount, FulfillmentResponsibilityKey, WorkItem, WorkItemAssignmentSeparationPolicy,
};
use crate::error::{Error, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::WorkItemExt;

const REASSIGN_VERSION_CONFLICT: &str = "任务版本已变化";
const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

struct AssignmentAuthorizationSnapshot {
    policy_revision: u64,
    actor_kind: erp_core::AccountKind,
    assignee_kind: erp_core::AccountKind,
    read_permission: String,
    actor_read_role_ids: Vec<String>,
    actor_manage_role_ids: Vec<String>,
    assignee_read_role_ids: Vec<String>,
    assignee_permissions: Vec<String>,
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
/// 事务内必须重验管理人、目标责任人与授权快照。
struct AssignmentPolicyAuditInput<'a> {
    /// 待转交任务。
    item: WorkItem,
    /// 期望任务版本。
    expected_task_version: u64,
    /// 目标责任人。
    target_user_id: String,
    /// 操作人。
    actor: &'a AuditActor,
    /// 强类型幂等命令收据。
    receipt: CommandReceipt,
    /// 权限安全的转交说明。
    audit_detail: String,
    /// 事务外冻结的授权快照。
    authorization: AssignmentAuthorizationSnapshot,
}

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 查询当前开放非审批任务可转交的具体账号。
    ///
    /// # 参数
    /// * `id` - 工作项稳定 ID
    /// * `actor` - 已通过鉴权且具有责任管理范围的操作人
    ///
    /// # 返回
    /// 返回当前仍有效、具备完整执行权限且满足任务责任约束的账号。
    ///
    /// # 错误
    /// 任务不存在、不是开放非审批任务、操作人不在管理范围，或授权事实读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 采购单责任任务的候选人必须同时能够执行该采购单全部开放履约任务；列表只作交互提示，
    /// 最终转交命令仍须在写事务内重验全部账号、授权与业务事实。
    pub async fn reassign_candidates(
        self,
        id: String,
        actor: AuditActor,
    ) -> Result<Vec<WorkItemReassignCandidateView>> {
        let managed_access = self.managed_access(&actor).await?;
        let item = self.load(id.clone()).await?;
        ensure_generic_work_item_mutation(&item)?;
        ensure_item_in_managed_scope(&item, &managed_access)?;

        let purchase_order_id = purchase_order_fulfillment_responsibility_id(&item)?;
        let cascade_tasks = if let Some(purchase_order_id) = purchase_order_id.as_deref() {
            let (_, tasks) = self
                .facts
                .purchase_order_fulfillment_scope(&item, purchase_order_id, &mut NoTransaction)
                .await?;
            Some(tasks)
        } else {
            None
        };

        let accounts =
            self.auth.list_accounts_by_kind(erp_core::AccountKind::Admin, &mut NoTransaction).await?;
        let mut candidates = Vec::new();
        for account in accounts {
            if item.owner_user_id.as_deref() == Some(account.id.as_str())
                || AvailableWorkItemAccount::from_account(&account).is_err()
            {
                continue;
            }
            let authorization =
                match self.assignment_authorization_snapshot(&actor, &account.id, &item, true).await {
                    Ok(authorization) => authorization,
                    Err(Error::Forbidden(_)) => continue,
                    Err(error) => return Err(error),
                };
            if let Some(tasks) = cascade_tasks.as_deref() {
                match ensure_fulfillment_tasks_candidate(
                    &self,
                    tasks,
                    &account.id,
                    &authorization.assignee_permissions,
                    &mut NoTransaction,
                )
                .await
                {
                    Ok(()) => {},
                    Err(Error::Forbidden(_)) => continue,
                    Err(error) => return Err(error),
                }
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
        Ok(candidates)
    }

    /// 受控转交开放任务。
    ///
    /// # 错误
    /// 缺少任务管理权限、目标资格无法证明、审批受阻或任务版本陈旧时返回错误。
    pub async fn reassign(
        self,
        id: String,
        req: ReassignWorkItemRequest,
        actor: AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let managed_access = self.managed_access(&actor).await?;
        let item = self.load(id.clone()).await?;
        ensure_generic_work_item_mutation(&item)?;
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.reassign";
        let target_user_id = required_text(&req.target_user_id, "目标用户不能为空")?;
        let reason = required_text(&req.reason, "转交原因不能为空")?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            action,
            "work_item",
            &id,
            &idempotency_key,
            [version, target_user_id.clone(), reason.clone()],
        )?;
        if let Some(replayed) = self.idempotent_replay(&receipt, &id).await? {
            ensure_generic_work_item_mutation(&replayed)?;
            return self.applied_outcome(replayed, &actor).await;
        }
        if item.base.version != expected_task_version {
            return self.conflict_outcome(&id, WorkItemConflictKind::Version, &actor).await;
        }
        ensure_item_in_managed_scope(&item, &managed_access)?;
        let authorization =
            self.assignment_authorization_snapshot(&actor, &target_user_id, &item, true).await?;
        let updated = self
            .reassign_with_assignment_policy_audit(AssignmentPolicyAuditInput {
                item,
                expected_task_version,
                target_user_id,
                actor: &actor,
                receipt,
                audit_detail: reason,
                authorization,
            })
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, &actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(&id, WorkItemConflictKind::Version, &actor).await
            },
        }
    }

    /// 形成事务外的授权版本锚点；事务内仍会重新读取全部资格与业务事实。
    async fn assignment_authorization_snapshot(
        &self,
        actor: &AuditActor,
        assignee_id: &str,
        item: &WorkItem,
        require_manager: bool,
    ) -> Result<AssignmentAuthorizationSnapshot> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let read_permission = policy.read_permission.to_string();
        let manage_permission = MANAGE_PERMISSION.to_string();
        for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
            let before = self.auth.current_policy_revision().await?;
            let actor_role_ids =
                active_role_ids(&self.auth, actor.kind(), actor.id(), &mut NoTransaction).await?;
            let actor_read_role_ids =
                self.roles_granting_permission(&actor_role_ids, &read_permission, true).await?;
            let actor_manage_role_ids =
                self.roles_granting_permission(&actor_role_ids, &manage_permission, true).await?;
            let assignee = self
                .auth
                .load_account(assignee_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
            let assignee = AvailableWorkItemAccount::from_account(&assignee)
                .map_err(|_| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
            let assignee_role_ids =
                active_role_ids(&self.auth, assignee.kind(), assignee_id, &mut NoTransaction).await?;
            let assignee_permissions = self.auth.permission_codes(assignee.kind(), assignee_id).await?;
            let assignee_read_role_ids =
                self.roles_granting_permission(&assignee_role_ids, &read_permission, true).await?;
            let snapshot = AssignmentAuthorizationSnapshot {
                policy_revision: before,
                actor_kind: actor.kind(),
                assignee_kind: assignee.kind(),
                read_permission: read_permission.clone(),
                actor_read_role_ids,
                actor_manage_role_ids,
                assignee_read_role_ids,
                assignee_permissions,
            };
            self.ensure_assignment_actor_access(
                actor.kind(),
                actor.id(),
                item,
                require_manager,
                &snapshot,
                &mut NoTransaction,
            )
            .await?;
            self.ensure_assignment_candidate(
                assignee_id,
                assignee.kind(),
                item,
                &snapshot,
                item.owner_user_id.as_deref() == Some(assignee_id),
                &mut NoTransaction,
            )
            .await?;
            let after = self.auth.policy_revision_with_executor(&mut NoTransaction).await?;
            if before == after {
                return Ok(snapshot);
            }
        }
        Err(Error::Rbac("授权策略持续变化，无法形成稳定的任务分派快照".to_string()))
    }

    /// 在调用方事务快照中重验操作人权限、管理范围与对象参与权。
    async fn ensure_assignment_actor_access(
        &self,
        actor_kind: erp_core::AccountKind,
        actor_id: &str,
        item: &WorkItem,
        require_manager: bool,
        authorization: &AssignmentAuthorizationSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .auth
            .load_account(actor_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, actor_kind)
            .map_err(|_| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        let access = self
            .assignment_access_for_executor(
                actor_kind,
                actor_id,
                &authorization.read_permission,
                &authorization.actor_read_role_ids,
                &authorization.actor_manage_role_ids,
                executor,
            )
            .await?;
        if require_manager {
            ensure_managed_access(&access)?;
            ensure_item_in_managed_scope(item, &access)?;
        }
        self.ensure_item_access_with_executor(item, &access, executor).await.map_err(task_read_error)
    }

    /// 在调用方事务快照中重验目标账号资格、对象访问权与岗位分离。
    ///
    /// # 参数
    /// * `user_id` - 待接收任务的具体账号 ID
    /// * `expected_kind` - 事务外授权快照冻结的账号类型
    /// * `item` - 待转交任务
    /// * `authorization` - 事务外形成的稳定授权快照
    /// * `allow_current_owner` - 是否允许目标账号保持为当前负责人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 目标账号仍有效、具备任务所需权限且满足岗位分离时返回 `Ok(())`。
    ///
    /// # 错误
    /// 账号失效、权限撤销、对象版本变化或岗位分离不满足时返回错误。
    async fn ensure_assignment_candidate(
        &self,
        user_id: &str,
        expected_kind: erp_core::AccountKind,
        item: &WorkItem,
        authorization: &AssignmentAuthorizationSnapshot,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .auth
            .load_account(user_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, expected_kind)
            .map_err(|_| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
        let mut access = self
            .assignment_access_for_executor(
                expected_kind,
                user_id,
                &authorization.read_permission,
                &authorization.assignee_read_role_ids,
                &[],
                executor,
            )
            .await?;
        if item.work_item_type.requires_full_execution_permissions() {
            // 执行任务除注册表读取权限外，还要求目标工作面使用的完整操作权限。
            // 该快照由同一 policy revision 形成，外层授权事务会以该 revision
            // 做 CAS 后才允许提交。
            access.permissions = authorization.assignee_permissions.clone();
        }
        self.ensure_assignment_candidate_access_with_executor(item, &access, executor)
            .await
            .map_err(task_read_error)?;
        self.ensure_assignment_separation(user_id, item, allow_current_owner, executor).await
    }

    /// 审批任务在形成个人责任前排除启动人、既往责任人、前序和当前决定人。
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

    /// 审批步骤按实例启动人、业务提交人、责任历史和决定历史排除候选人。
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

    /// 读取非审批正式决定任务的权威提交人、经办人及历史决定人。
    async fn domain_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let actors = self.facts.assignment_separation_actors(item, executor).await?;
        non_empty_assignment_actors(actors)
    }

    /// 使用调用方 executor 重验转交目标的对象访问条件。
    ///
    /// # 参数
    /// * `item` - 待转交任务
    /// * `access` - 由当前角色与权限形成的目标账号访问快照
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 对象存在、版本匹配且目标账号满足任务类型的访问条件时返回 `Ok(())`。
    ///
    /// # 错误
    /// 对象未注册、不存在、版本变化或目标账号访问条件不足时返回错误。
    ///
    /// # 关键业务约束
    /// 供给分配任务按具体账号和 `purchase_order:create` 授权，不额外引入团队池或固定角色约束。
    async fn ensure_assignment_candidate_access_with_executor(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.facts.load_object_facts(&keys, executor).await?;
        let fact = facts
            .get(&(policy.object_kind, item.business_object_id.clone()))
            .ok_or_else(|| Error::Forbidden("任务业务对象不可访问".into()))?;
        require_order_task_read(&self.auth, &access.actor_id, policy.object_kind, fact, executor).await?;
        if !has_assignment_candidate_access(item, access, &facts) {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 在同一事务内重验管理人、目标责任人及全部业务事实后执行转交与审计。
    ///
    /// # 用途
    /// 按授权快照转交任务并写入幂等审计。
    ///
    /// # 参数
    /// * `input` - 任务、目标责任人与审计字段
    ///
    /// # 返回
    /// 返回写入结果或版本冲突。
    ///
    /// # 错误
    /// 授权变化、版本冲突或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 事务内必须重验管理人和目标责任人，并以授权快照版本执行 policy CAS 后才可提交。
    async fn reassign_with_assignment_policy_audit(
        &self,
        input: AssignmentPolicyAuditInput<'_>,
    ) -> Result<WorkItemWriteOutcome> {
        let AssignmentPolicyAuditInput {
            item,
            expected_task_version,
            target_user_id,
            actor,
            receipt,
            audit_detail,
            authorization,
        } = input;
        let replay_receipt = receipt.clone();
        let replay_item_id = item.base.id.clone();
        let purchase_order_id = purchase_order_fulfillment_responsibility_id(&item)?;
        let source_user_id = item.owner_user_id.as_deref().unwrap_or("未指定").to_string();
        let selected_work_item_id = item.base.id.clone();
        let purchase_order_audit = purchase_order_id
            .as_ref()
            .map(|purchase_order_id| {
                PreparedWorkflowAudit::resource_with_id(
                    format!("{}-purchase-order", receipt.id()),
                    actor.clone(),
                    "purchase_order.owner_reassign",
                    "purchase_order",
                    purchase_order_id.clone(),
                    Some(format!(
                        "source_user_id={source_user_id};target_user_id={target_user_id};cascade=open_fulfillment_tasks;selected_work_item_id={selected_work_item_id}"
                    )),
                )
            })
            .transpose()?;
        let audit = PreparedWorkflowAudit::resource_with_id(
            receipt.id().to_string(),
            actor.clone(),
            receipt.action(),
            receipt.resource_type(),
            item.base.id.clone(),
            Some(receipt.message(Some(&audit_detail))),
        )?;
        let item_id = item.base.id;
        let actor_id = actor.id().to_string();
        let actor_kind = actor.kind();
        let policy_revision = authorization.policy_revision;
        let policy_rbac = self.auth.clone();
        let validation = self.clone();
        let audit_port = Arc::clone(&self.audit);
        let db = self.db.clone();
        let result = policy_rbac
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let mut current = db
                        .work_items()
                        .find_work_item(&item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
                    if current.base.version != expected_task_version {
                        return Err(Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string()));
                    }
                    let allow_current_owner =
                        current.owner_user_id.as_deref() == Some(target_user_id.as_str());
                    ensure_assignment_policy_in_transaction(
                        &validation,
                        AssignmentPolicyCheck {
                            actor_kind,
                            actor_id: &actor_id,
                            assignee_id: &target_user_id,
                            item: &current,
                            require_manager: true,
                            authorization: &authorization,
                            allow_current_owner,
                        },
                        session,
                    )
                    .await?;
                    current = if let Some(purchase_order_id) = purchase_order_id.as_deref() {
                        reassign_purchase_order_fulfillment_responsibility(
                            &validation,
                            ReassignPurchaseOrderFulfillmentInput {
                                selected: current,
                                purchase_order_id,
                                target_user_id: &target_user_id,
                                actor_id: &actor_id,
                                authorization: &authorization,
                            },
                            session,
                        )
                        .await?
                    } else {
                        current.reassign(target_user_id.clone(), Instant::now())?;
                        validation
                            .facts
                            .reassign_integration_handler(&mut current, &target_user_id, session)
                            .await?;
                        db.work_items().update(&mut current, session).await.map_err(|error| match error {
                            persistence_core::Error::OptimisticLockingError => {
                                Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
                            },
                            error => Error::from(error),
                        })?;
                        current
                    };
                    ensure_assignment_policy_in_transaction(
                        &validation,
                        AssignmentPolicyCheck {
                            actor_kind,
                            actor_id: &actor_id,
                            assignee_id: &target_user_id,
                            item: &current,
                            require_manager: true,
                            authorization: &authorization,
                            allow_current_owner: true,
                        },
                        session,
                    )
                    .await?;
                    if let Some(purchase_order_audit) = &purchase_order_audit {
                        audit_port.persist(purchase_order_audit, session).await?;
                    }
                    audit_port.persist(&audit, session).await?;
                    Ok(current)
                })
            })
            .await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(Error::ConflictError(message)) if message == REASSIGN_VERSION_CONFLICT => {
                Ok(WorkItemWriteOutcome::VersionConflict)
            },
            Err(error) => match self.idempotent_replay(&replay_receipt, &replay_item_id).await? {
                Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                None => Err(error),
            },
        }
    }
}

/// 事务内分派策略重验输入。
///
/// # 用途
/// 将操作人、候选人与授权快照打包，供 [`ensure_assignment_policy_in_transaction`] 使用。
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
/// 操作人身份必须与授权快照一致。
struct AssignmentPolicyCheck<'a> {
    /// 操作人账号类型。
    actor_kind: erp_core::AccountKind,
    /// 操作人 ID。
    actor_id: &'a str,
    /// 目标责任人 ID。
    assignee_id: &'a str,
    /// 当前任务。
    item: &'a WorkItem,
    /// 是否要求管理人权限。
    require_manager: bool,
    /// 事务外冻结的授权快照。
    authorization: &'a AssignmentAuthorizationSnapshot,
    /// 是否允许候选人为当前责任人。
    allow_current_owner: bool,
}

/// 在任务责任事务内重放全部固定分派策略。
///
/// # 用途
/// 重验操作人授权与候选人资格后再允许写入。
///
/// # 参数
/// * `service` - 保留已装配事实与授权 Port 的任务服务
/// * `check` - 分派策略重验输入
/// * `executor` - 事务执行器
///
/// # 返回
/// 策略仍成立时返回 `Ok(())`。
///
/// # 错误
/// 身份变化、授权不足或候选人非法时返回错误。
///
/// # 关键业务约束
/// 必须在同一任务责任事务内调用。
async fn ensure_assignment_policy_in_transaction<A: crate::ports::WorkflowAuthorizationPort>(
    service: &WorkItemService<A>,
    check: AssignmentPolicyCheck<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if check.actor_kind != check.authorization.actor_kind {
        return Err(Error::Forbidden("操作账号身份已变化".to_string()));
    }
    service
        .ensure_assignment_actor_access(
            check.actor_kind,
            check.actor_id,
            check.item,
            check.require_manager,
            check.authorization,
            executor,
        )
        .await?;
    service
        .ensure_assignment_candidate(
            check.assignee_id,
            check.authorization.assignee_kind,
            check.item,
            check.authorization,
            check.allow_current_owner,
            executor,
        )
        .await
}

/// 解析采购单履约任务冻结的采购责任键。
///
/// # 参数
/// * `item` - 待转交工作项
///
/// # 返回
/// 非采购履约任务返回空；采购履约任务返回采购单 ID。
///
/// # 错误
/// 对象类型、责任角色、原因码或责任键不符合固定履约合同时返回错误。
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

/// 采购单履约责任转交入参。
struct ReassignPurchaseOrderFulfillmentInput<'a> {
    /// 管理员本次选中的开放任务。
    selected: WorkItem,
    /// 责任键解析出的采购单 ID。
    purchase_order_id: &'a str,
    /// 新采购责任人。
    target_user_id: &'a str,
    /// 管理员账号 ID。
    actor_id: &'a str,
    /// 事务外冻结的授权快照。
    authorization: &'a AssignmentAuthorizationSnapshot,
}

/// 原子变更采购单当前责任人与其全部开放采购履约任务。
///
/// # 参数
/// * `service` - 保留生产事实 Port 的任务服务
/// * `input` - 选中任务、目标责任人与授权快照
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 返回已完成转交的选中任务。
///
/// # 错误
/// 采购单、任务集合或原责任不一致，目标缺少任一履约权限，或 CAS 写入失败时返回错误。
///
/// # 关键业务约束
/// 完成和关闭的历史任务保持不变；只有同一 `purchase_order:{id}` 下的开放履约任务级联。
async fn reassign_purchase_order_fulfillment_responsibility<A: crate::ports::WorkflowAuthorizationPort>(
    service: &WorkItemService<A>,
    input: ReassignPurchaseOrderFulfillmentInput<'_>,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let (_, mut tasks) = service
        .facts
        .purchase_order_fulfillment_scope(&input.selected, input.purchase_order_id, executor)
        .await?;
    ensure_fulfillment_tasks_candidate(
        service,
        &tasks,
        input.target_user_id,
        &input.authorization.assignee_permissions,
        executor,
    )
    .await
    .map_err(task_read_error)?;

    service
        .facts
        .reassign_purchase_order_owner(
            input.purchase_order_id,
            input.target_user_id,
            input.actor_id,
            executor,
        )
        .await?;

    let reassigned_at = Instant::now();
    let mut selected_after = None;
    for task in &mut tasks {
        task.reassign(input.target_user_id.to_string(), reassigned_at)?;
        service.db.work_items().update(task, executor).await.map_err(|error| match error {
            persistence_core::Error::OptimisticLockingError => {
                Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
            },
            error => Error::from(error),
        })?;
        if task.base.id == input.selected.base.id {
            selected_after = Some(task.clone());
        }
    }
    selected_after.ok_or_else(|| Error::ConflictError("采购单开放履约任务已变化，请刷新后重试".to_string()))
}

const SYSTEM_OBJECT_OWNER: &str = "__system__";

/// 校验目标账号可执行给定全部开放履约任务。
async fn ensure_fulfillment_tasks_candidate<A: crate::ports::WorkflowAuthorizationPort>(
    service: &WorkItemService<A>,
    tasks: &[WorkItem],
    target_user_id: &str,
    permissions: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    let target_access = ActorAccess::new(target_user_id.to_string())
        .with_permissions(permissions.to_vec())
        .with_managed_owner_ids(Some(Vec::new()));
    for task in tasks {
        service.ensure_assignment_candidate_access_with_executor(task, &target_access, executor).await?;
    }
    Ok(())
}

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

/// 按资源逐一证明正式票款审计，并返回创建、过账或红冲等经办人。
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
