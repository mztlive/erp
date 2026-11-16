//! 任务授权统一交给 identity；业务只核验冻结事实及状态。
use std::collections::HashSet;

use application_core::AuditActor;
use erp_identity::{AccessTarget, CompiledAccess};
use erp_permissions::{internal, work_item};
use persistence_core::Executor;

use super::WorkItemService;
use super::order_access::task_read_error;
use crate::WorkItemExt;
use crate::entity::work_item::{TaskAccess, WorkItem, WorkItemType};
use crate::error::{Error, ErrorCode, Result};
use crate::ports::{ObjectFactMap, WorkflowAuthorizationPort};
use crate::repository::work_item::authorization::WorkItemAuthorizationExt;
use crate::service::approval::execution::runtime_service::approval_task_facts_current;

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 任务详情已在本阶段授权后，计算展示动作资格。
    /// # 参数
    /// actor、item 为已授权账号及任务，read 为原读取凭证，executor 沿用原事务。
    /// # 返回
    /// 返回同角色执行、审批决定和任务管理的展示资格。
    /// # 错误
    /// 身份失效、功能取证或管理授权设施失败时返回错误。
    pub(super) async fn task_actions(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        read: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> Result<TaskAccess> {
        let mut access = TaskAccess::new(actor.id().to_owned());
        if read.execution_qualified(item.work_item_type.as_str(), &item.business_object_type, executor)? {
            access.executable_task_ids.insert(item.base.id.clone());
        }
        if read.decision_qualified(executor)? {
            access.decidable_task_ids.insert(item.base.id.clone());
        }
        access.managed_task_ids = self
            .db
            .work_items()
            .authorized_ids(std::slice::from_ref(&item.base.id), read, executor)
            .await?
            .into_iter()
            .collect();
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
        self.authorization.require(actor, &work_item::DETAIL, &item.base.id, executor).await?;
        self.ensure_task_current(item, executor).await
    }

    /// 统一任务及来源授权完成后，只读取业务版本与冻结责任，不开始新的授权阶段。
    /// # 参数
    /// item 为同阶段已授权任务，executor 为整个读取用例的原执行器。
    /// # 返回
    /// 精确任务的业务版本、冻结责任及审批运行链全部匹配时成功。
    /// # 错误
    /// 来源业务事实缺失、冻结版本不匹配或仓储失败时拒绝。
    pub(super) async fn ensure_task_current(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if item.work_item_type.is_document_approval() {
            return if approval_task_facts_current(&self.db, item, executor).await? {
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
        if !task_facts_current(item, &facts) {
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
        self.ensure_decision_access(actor, item, executor).await
    }

    /// 在原执行器核验完整办理资格、指定任务事实和真实业务来源。
    /// # 参数
    /// actor、item 为账号与真实任务；executor 为原事务。
    /// # 返回
    /// 同角色资格、任务范围、冻结事实和业务确认资格均成立时成功。
    /// # 错误
    /// 身份失效、范围或真实来源无权、版本变化时拒绝。
    pub(super) async fn ensure_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .service
            .require(
                actor,
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::Task { id: item.base.id.clone(), version: item.base.version.to_string() },
                executor,
            )
            .await
            .map_err(|error| task_read_error(error.into()))?;
        self.ensure_task_current(item, executor).await.map_err(task_read_error)
    }
}

/// 验证已授权任务的业务版本与冻结询价责任，不执行访问授权。
/// # 参数
/// item 为已授权任务，facts 为当前业务事实。
/// # 返回
/// 根事实存在且冻结版本和责任仍一致时为 true。
/// # 错误
/// 无；缺事实或版本损坏返回 false。
pub fn task_facts_current(item: &WorkItem, facts: &ObjectFactMap) -> bool {
    let Some(policy) = item.work_item_type.brief_relation(&item.business_object_type) else { return false };
    let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else { return false };
    if item.work_item_type == WorkItemType::SalesInquiryProcessing {
        let Some(ids) = fact.responsibility_scope_ids.as_ref() else { return false };
        let Some(version) =
            item.subject_version.strip_prefix("requirement-").and_then(|value| value.parse().ok())
        else {
            return false;
        };
        if !item.inquiry_task_current(&item.business_object_id, version, ids) {
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

#[cfg(test)]
mod decision_tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::work_item::SupplierPortalReviewTaskData;
    use crate::service::work_item::testing::unavailable_service;

    #[tokio::test]
    async fn decision_stops_before_business_facts_when_central_authorization_cannot_prove_access() {
        let service = unavailable_service();
        let actor = AuditActor::new("reviewer".into(), "reviewer".into(), AccountKind::Admin);
        let mut task = WorkItem::new_supplier_portal_review(
            WorkItemId::new("task"),
            SupplierPortalReviewTaskData {
                request_id: "request".into(),
                subject_version: "submission-1".into(),
                owner_user_id: "reviewer".into(),
                owner_organization_id: "org".into(),
                due_at: None,
                impact_summary: None,
            },
        )
        .unwrap();
        for terminal in [false, true] {
            if terminal {
                task.complete_supplier_portal_review(actor.id(), Instant::from_unix_secs(100)).unwrap();
            }
            assert!(matches!(
                service.ensure_domain_decision_access(&actor, &task, &mut NoTransaction).await,
                Err(Error::ValidationError(_))
            ));
            assert!(
                matches!(service.ensure_domain_decision_access(&actor, &task, &mut SnapshotExecutor::default()).await,
                Err(Error::Internal(reason)) if reason == "测试授权设施不可用")
            );
        }
    }
}
