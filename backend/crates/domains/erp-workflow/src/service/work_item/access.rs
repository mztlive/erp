//! 任务授权统一交给 identity；业务只核验冻结事实及状态。
use std::collections::HashSet;

use application_core::AuditActor;
use erp_core::Result as CoreResult;
use erp_identity::Permission;
use persistence_core::Executor;

use super::order_access::{
    require_order_task_read, require_supplier_portal_task_read, require_supplier_portal_task_review,
    task_read_error,
};
use super::{WorkItemAuthorization, WorkItemService};
use crate::entity::work_item::{TaskAccess, WorkItem, WorkItemType};
use crate::error::{Error, ErrorCode, Result};
use crate::ports::{ObjectFactMap, TaskReadAction, WorkflowAuthorizationPort};
use crate::service::approval::execution::runtime_service::approval_task_readable_with_executor;
use crate::service::authorization::features;

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 任务详情已在本阶段授权后，计算展示动作资格。
    /// # 参数
    /// actor、item 为已授权账号及任务，executor 沿用原事务。
    /// # 返回
    /// 返回同角色执行、审批决定和任务管理的展示资格。
    /// # 错误
    /// 身份失效、功能取证或管理授权设施失败时返回错误。
    pub(super) async fn task_actions(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<TaskAccess> {
        let mut required = item.qualification_permissions()?;
        required.extend(
            ["approval_instance:read", "approval_instance:decide"]
                .into_iter()
                .map(Permission::parse)
                .collect::<CoreResult<Vec<_>>>()?,
        );
        let qualified = features(&self.db, &self.identity)
            .permissions(actor, &required, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("任务操作账号已失效".into()))?;
        let mut access = TaskAccess::new(actor.id().to_owned());
        if qualified.same_role(&item.qualification_permissions()?) {
            access.executable_task_ids.insert(item.base.id.clone());
        }
        let decide =
            [Permission::parse("approval_instance:read")?, Permission::parse("approval_instance:decide")?];
        if qualified.same_role(&decide) {
            access.decidable_task_ids.insert(item.base.id.clone());
        }
        match self.authorization.require(actor, "manage", &item.base.id, executor).await {
            Ok(()) => {
                access.managed_task_ids.insert(item.base.id.clone());
            },
            Err(Error::Forbidden(_)) => {},
            Err(error) => return Err(error),
        }
        Ok(access)
    }

    /// 证明精确任务及实际来源的读取资格，随后只检查业务事实。
    /// # 参数
    /// item、actor 为当前任务及真实账号，executor 沿用原事务。
    /// # 返回
    /// identity 授权及冻结业务事实均成立时成功。
    /// # 错误
    /// 权限、来源缺失、冻结版本或责任行不匹配时拒绝。
    pub async fn ensure_item_access_with_executor(
        &self,
        item: &WorkItem,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.require(actor, "detail", &item.base.id, executor).await?;
        self.ensure_task_facts(item, actor, executor).await
    }

    /// 详情已通过统一授权后，核验真实来源、冻结版本和责任事实。
    /// # 参数
    /// item、actor 为本阶段授权的任务及账号，executor 沿用原事务。
    /// # 返回
    /// 真实来源和冻结业务事实一致时成功。
    /// # 错误
    /// 来源不可读、事实缺失、版本或责任不匹配时拒绝。
    pub(super) async fn ensure_task_facts(
        &self,
        item: &WorkItem,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if item.work_item_type.is_document_approval() {
            return if approval_task_readable_with_executor(
                &self.identity,
                &self.db,
                &self.auth,
                actor,
                item,
                executor,
            )
            .await?
            {
                Ok(())
            } else {
                Err(Error::Forbidden("审批任务责任链不匹配".into()))
            };
        }
        let policy = item
            .work_item_type
            .brief_relation(&item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".into()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.facts.load_object_facts(&keys, executor).await?;
        let fact = facts
            .get(&(policy.object_kind, item.business_object_id.clone()))
            .ok_or_else(|| Error::Forbidden("任务业务对象不存在".into()))?;
        require_supplier_portal_task_read(
            &self.auth,
            actor,
            policy.object_kind,
            &item.business_object_id,
            executor,
        )
        .await?;
        require_order_task_read(
            &self.auth,
            actor,
            policy.object_kind,
            fact,
            TaskReadAction::Detail,
            executor,
        )
        .await?;
        if !task_facts_current(item, actor.id(), &facts) {
            return Err(Error::Forbidden("任务业务版本或冻结责任不匹配".into()));
        }
        Ok(())
    }

    /// 在业务决定事务证明同角色完整执行资格及精确任务访问。
    /// # 参数
    /// actor、item 为实际账号和任务，executor 为原写事务。
    /// # 返回
    /// 统一资格及业务版本均通过时成功。
    /// # 错误
    /// 失效身份、完整资格不足、对象授权或业务事实错误透传。
    pub async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_decision_access(actor, item, &self.authorization, executor).await
    }

    /// 在原执行器核验完整办理资格、指定任务事实和真实业务来源。
    /// # 参数
    /// actor、item 为账号与任务；authorization 提供已有或拟保存任务的事实；executor 为原事务。
    /// # 返回
    /// 同角色资格、任务范围、冻结事实和业务确认资格均成立时成功。
    /// # 错误
    /// 身份失效、范围或真实来源无权、版本变化时拒绝。
    pub(super) async fn ensure_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        authorization: &WorkItemAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let policy = item
            .work_item_type
            .brief_relation(&item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".into()))?;
        let required = item.qualification_permissions()?;
        let qualified = features(&self.db, &self.identity)
            .permissions(actor, &required, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".into()))?;
        if !qualified.same_role(&required) {
            return Err(Error::Forbidden("当前账号不具备同角色完整执行资格".into()));
        }
        authorization.require(actor, "detail", &item.base.id, executor).await.map_err(task_read_error)?;
        self.ensure_task_facts(item, actor, executor).await.map_err(task_read_error)?;
        require_supplier_portal_task_review(
            &self.auth,
            actor,
            policy.object_kind,
            &item.business_object_id,
            executor,
        )
        .await?;
        Ok(self.identity.ensure_policy_snapshot_with_executor(qualified.policy_revision(), executor).await?)
    }

    /// 在原事务证明任务管理权限及对象范围。
    /// # 参数
    /// actor、item 为真实账号和当前任务，executor 为原事务。
    /// # 返回
    /// identity 对当前任务管理判定成立时成功。
    /// # 错误
    /// 身份、功能资格或数据范围拒绝时失败。
    pub async fn ensure_task_manage(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.require(actor, "manage", &item.base.id, executor).await
    }
}

/// 验证已授权任务的业务版本与冻结询价责任，不执行访问授权。
/// # 参数
/// item 为任务、actor_id 为实际办理人、facts 为当前业务事实。
/// # 返回
/// 根事实存在且冻结版本和责任仍一致时为 true。
/// # 错误
/// 无；缺事实或版本损坏返回 false。
pub fn task_facts_current(item: &WorkItem, actor_id: &str, facts: &ObjectFactMap) -> bool {
    let Some(policy) = item.work_item_type.brief_relation(&item.business_object_type) else { return false };
    let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else { return false };
    if item.work_item_type == WorkItemType::SalesInquiryProcessing {
        let Some(ids) = fact.responsibility_scope_ids.as_ref() else { return false };
        let Some(version) =
            item.subject_version.strip_prefix("requirement-").and_then(|value| value.parse().ok())
        else {
            return false;
        };
        if !item.owns_inquiry_task(actor_id, &item.business_object_id, version, ids) {
            return false;
        }
    }
    fact.subject_versions.accepts_task(item.work_item_type, item.status, &item.base.id, &item.subject_version)
}

/// 验证转派候选使用的业务版本与冻结责任行，不决定账号权限。
/// # 参数
/// item 为待转派任务、facts 为当前业务事实。
/// # 返回
/// 任务状态及冻结责任完整且业务版本匹配时为 true。
/// # 错误
/// 无；缺事实或责任不完整返回 false。
pub fn candidate_facts_current(item: &WorkItem, facts: &ObjectFactMap) -> bool {
    if item.work_item_type == WorkItemType::ProcurementOrderCreation && !item.is_open_procurement_task() {
        return false;
    }
    let Some(policy) = item.work_item_type.brief_relation(&item.business_object_type) else { return false };
    let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else { return false };
    if item.work_item_type == WorkItemType::SalesInquiryProcessing
        && !fact.responsibility_scope_ids.as_ref().is_some_and(|ids| {
            !item.responsibility_scope_ids().is_empty()
                && item.responsibility_scope_ids().iter().all(|id| ids.contains(id))
        })
    {
        return false;
    }
    fact.subject_versions.accepts(&item.subject_version)
}

/// 将实体对审批任务的通用责任变更禁令映射为稳定审批错误码。
///
/// # 参数
/// * `item` - 待变更责任的工作项
///
/// # 返回
/// 实体允许通用责任变更时无返回值。
///
/// # 错误
/// 实体拒绝时返回 `ApprovalGenericWorkItemMutationForbidden`。
pub fn ensure_generic_work_item_mutation(item: &WorkItem) -> Result<()> {
    if item.work_item_type == WorkItemType::SalesInquiryProcessing {
        return Err(Error::Forbidden("询价责任变更必须使用询价专用命令".into()));
    }
    item.ensure_generic_responsibility_mutation()
        .map_err(|_| Error::from_approval_code(ErrorCode::ApprovalGenericWorkItemMutationForbidden))
}
