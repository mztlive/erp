//! 转交命令的事务内策略重验与写入。

use std::sync::Arc;

use erp_core::common::time::Instant;
use persistence_core::Executor;

use super::WorkItemService;
use super::order_access::task_read_error;
use super::reassign::{
    AssignmentAuthorizationSnapshot, AssignmentPolicyAuditInput, ensure_fulfillment_tasks_candidate,
    purchase_order_fulfillment_responsibility_id,
};
use super::write::WorkItemWriteOutcome;
use crate::entity::work_item::WorkItem;
use crate::error::{Error, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::WorkItemExt;
use crate::repository::prelude::*;

const REASSIGN_VERSION_CONFLICT: &str = "任务版本已变化";

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
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
    pub(super) async fn reassign_with_assignment_policy_audit(
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
            .run_authorized_policy_transaction(policy_revision, move |executor| {
                Box::pin(async move {
                    let mut current = db
                        .work_items()
                        .find_work_item(&item_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
                    if current.base.version != expected_task_version {
                        return Err(Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string()));
                    }
                    let allow_current_owner =
                        current.owner_user_id.as_deref() == Some(target_user_id.as_str());
                    ensure_assignment_policy(
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
                        executor,
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
                            executor,
                        )
                        .await?
                    } else {
                        current.reassign(target_user_id.clone(), Instant::now())?;
                        validation
                            .facts
                            .reassign_integration_handler(&mut current, &target_user_id, executor)
                            .await?;
                        db.work_items().update(&mut current, executor).await.map_err(
                            |error| match error {
                                persistence_core::Error::OptimisticLockingError => {
                                    Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
                                },
                                error => Error::from(error),
                            },
                        )?;
                        current
                    };
                    ensure_assignment_policy(
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
                        executor,
                    )
                    .await?;
                    if let Some(purchase_order_audit) = &purchase_order_audit {
                        audit_port.persist(purchase_order_audit, executor).await?;
                    }
                    audit_port.persist(&audit, executor).await?;
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
/// 将操作人、候选人与授权快照打包，供 [`ensure_assignment_policy`] 使用。
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
async fn ensure_assignment_policy<A: crate::ports::WorkflowAuthorizationPort>(
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
