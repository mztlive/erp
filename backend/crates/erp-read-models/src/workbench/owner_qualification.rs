//! 已授权管理视图中的订单任务负责人资格；只返回阻断摘要，不修改任务责任。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use application_core::AuditActor;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::WorkItemStatus;
use erp_workflow::ports::{ObjectFactMap, OrderTaskSource, RolePermissionSnapshotFact, WorkflowAccountFact};
use erp_workflow::service::work_item::access::required_execution_permissions;
use persistence_core::Executor;

use super::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemView, WorkbenchReadService,
};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 对当前页的订单关联任务按负责人分组重验；保留原责任和受控管理动作。
    pub(super) async fn apply_owner_qualification(
        &self,
        items: &mut [WorkItemView],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.qualify_approvals(items, executor).await?;
        let keys = items
            .iter()
            .filter(|item| item.status == WorkItemStatus::Open)
            .filter_map(|item| {
                item.work_item_type
                    .brief_relation(&item.business_object_type)
                    .filter(|relation| OrderTaskSource::required_for(relation.object_kind))
                    .map(|relation| (relation.object_kind, item.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        if keys.is_empty() {
            return Ok(());
        }
        let facts = self.facts_reader().load(&keys, executor).await?;
        let groups = owner_groups(items, &facts)?;
        self.qualify_owner_groups(items, groups, executor).await
    }

    /// 当前审批动作范围失效时保留原负责人并移除决定动作。
    async fn qualify_approvals(&self, items: &mut [WorkItemView], executor: &mut dyn Executor) -> Result<()> {
        let mut keys = HashSet::new();
        let mut owners = BTreeSet::new();
        for item in items
            .iter()
            .filter(|i| i.status == WorkItemStatus::Open && i.work_item_type.is_document_approval())
        {
            let kind = DocumentType::try_from_code(&item.business_object_type)
                .map_err(|_| Error::Internal("审批任务类型未登记".into()))?;
            keys.insert((kind, item.business_object_id.clone()));
            if let Some(id) = &item.owner_user_id {
                owners.insert(id.clone());
            }
        }
        if keys.is_empty() {
            return Ok(());
        }
        let objects = self.auth.approval_scope_objects(&keys, executor).await?;
        let accounts = self
            .auth
            .load_accounts(&owners.into_iter().collect::<Vec<_>>(), executor)
            .await?
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect::<HashMap<_, _>>();
        let mut decisions = HashMap::new();
        let mut reads = HashMap::new();
        for item in items
            .iter_mut()
            .filter(|i| i.status == WorkItemStatus::Open && i.work_item_type.is_document_approval())
        {
            let account = item
                .owner_user_id
                .as_ref()
                .and_then(|id| accounts.get(id))
                .filter(|a| a.is_active_backoffice());
            let Some(account) = account else {
                block_owner(item);
                continue;
            };
            let kind = DocumentType::try_from_code(&item.business_object_type)
                .map_err(|_| Error::Internal("审批任务类型未登记".into()))?;
            let Some(object) = objects.get(&(kind, item.business_object_id.clone())) else {
                block_owner(item);
                continue;
            };
            let actor = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
            if !decisions.contains_key(&account.id) {
                decisions.insert(
                    account.id.clone(),
                    self.auth.resolve_workflow_scope(&actor, "approval_instance:decide", executor).await?,
                );
            }
            let read_key = (account.id.clone(), kind);
            if !reads.contains_key(&read_key) {
                reads.insert(
                    read_key.clone(),
                    erp_workflow::service::approval::approval_document_read_scope_with_executor(
                        &self.auth, &actor, kind, executor,
                    )
                    .await?,
                );
            }
            if !decisions[&account.id].as_ref().is_some_and(|s| s.allows(object))
                || !reads[&read_key].covers_object(object)
            {
                block_owner(item);
            }
        }
        Ok(())
    }

    /// 账号批量读取；同一负责人只解析一次权限和每种订单的详情范围。
    async fn qualify_owner_groups(
        &self,
        items: &mut [WorkItemView],
        groups: OwnerGroups,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = groups.keys().filter(|id| !id.is_empty()).cloned().collect::<Vec<_>>();
        let accounts = self
            .auth
            .load_accounts(&ids, executor)
            .await?
            .into_iter()
            .map(|account| (account.id.clone(), account))
            .collect::<HashMap<_, _>>();
        for (owner, tasks) in groups {
            let Some(account) = accounts.get(&owner).filter(|account| account.is_active_backoffice()) else {
                for (index, _) in tasks {
                    block_owner(&mut items[index]);
                }
                continue;
            };
            self.qualify_owner(items, account, &tasks, executor).await?;
        }
        Ok(())
    }

    /// 读取权与完整执行权分别验证；部门相同不能补执行资格。
    async fn qualify_owner(
        &self,
        items: &mut [WorkItemView],
        account: &WorkflowAccountFact,
        tasks: &[(usize, OrderTaskSource)],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let actor = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
        let required = tasks
            .iter()
            .filter_map(|(index, _)| execution_permissions(&items[*index]))
            .flatten()
            .collect::<BTreeSet<_>>();
        let grants = self
            .auth
            .role_permission_snapshot(account.kind, &account.id, &required.into_iter().collect::<Vec<_>>())
            .await?;
        let enabled_roles = self.auth.enabled_role_ids(grants.role_ids(), executor).await?;
        let sources = tasks.iter().map(|(_, source)| source.clone()).collect::<BTreeSet<_>>();
        let readable = self.auth.readable_order_sources(&actor, &sources, executor).await?;
        for (index, source) in tasks {
            let item = &mut items[*index];
            if !readable.contains(source) || !can_execute(item, &grants, &enabled_roles) {
                block_owner(item);
            }
        }
        Ok(())
    }
}

/// 固定任务类型提供执行权限；审批额外要求静态决定权限。
fn execution_permissions(item: &WorkItemView) -> Option<Vec<&'static str>> {
    let mut required =
        required_execution_permissions(item.work_item_type, &item.business_object_type)?.to_vec();
    if item.work_item_type.is_document_approval() {
        required.push("approval_instance:decide");
    }
    Some(required)
}

/// 按既有任务执行合同逐权限检查当前有效角色；订单范围另由公共解析器校验。
fn can_execute(item: &WorkItemView, grants: &RolePermissionSnapshotFact, enabled_roles: &[String]) -> bool {
    let Some(required) = execution_permissions(item) else {
        return false;
    };
    required.iter().all(|permission| {
        grants.granting_role_ids(permission).iter().any(|role| enabled_roles.contains(role))
    })
}

type OwnerGroups = BTreeMap<String, Vec<(usize, OrderTaskSource)>>;

/// 精确的业务来源与当前负责人分组；不从任务展示组织推断内部组织。
fn owner_groups(items: &[WorkItemView], facts: &ObjectFactMap) -> Result<OwnerGroups> {
    let mut groups = BTreeMap::<String, Vec<(usize, OrderTaskSource)>>::new();
    for (index, item) in items.iter().enumerate() {
        let Some(relation) = item.work_item_type.brief_relation(&item.business_object_type) else {
            continue;
        };
        if item.status != WorkItemStatus::Open || !OrderTaskSource::required_for(relation.object_kind) {
            continue;
        }
        let source = facts
            .get(&(relation.object_kind, item.business_object_id.clone()))
            .and_then(|fact| fact.order_scope_source.clone())
            .filter(|source| source.matches_kind(relation.object_kind))
            .ok_or_else(|| Error::Internal("订单关联任务缺少权威范围来源".into()))?;
        groups.entry(item.owner_user_id.clone().unwrap_or_default()).or_default().push((index, source));
    }
    Ok(groups)
}

/// 资格失效只影响安全投影；已有审批阻断原因优先，受控非审批改派保留。
fn block_owner(item: &mut WorkItemView) {
    let approval = item.work_item_type.is_document_approval() || item.approval_node_execution_id.is_some();
    item.allowed_actions.retain(|action| {
        *action == WorkItemAllowedAction::View
            || (!approval && matches!(action, WorkItemAllowedAction::Reassign | WorkItemAllowedAction::Close))
    });
    if item.processing_state == ProcessingState::ApprovalBlocked {
        return;
    }
    let blocker = ProcessingBlockerView {
        code: "WORK_ITEM_OWNER_INELIGIBLE".into(),
        message: "当前负责人已失去账号、执行权限或关联订单读取资格，需由管理人员处理".into(),
    };
    item.processing_state = ProcessingState::ExecutionBlocked;
    item.processing_blocker = Some(blocker.clone());
    item.action_blockers.push(blocker);
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };

    use super::*;
    use crate::workbench::dto::WorkItemFields;

    fn view() -> WorkItemView {
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "purchase_receipt".into(),
                business_object_id: "receipt".into(),
                subject_version: "1".into(),
                owner_role: "role-procurement".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "original-owner".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "warehouse:warehouse-1:receipt",
        )
        .unwrap();
        let mut view = WorkItemView::from_fields(WorkItemFields::from(item), "context".into()).unwrap();
        view.allowed_actions = vec![
            WorkItemAllowedAction::View,
            WorkItemAllowedAction::Process,
            WorkItemAllowedAction::Approve,
            WorkItemAllowedAction::Reject,
            WorkItemAllowedAction::Reassign,
            WorkItemAllowedAction::Close,
        ];
        view
    }

    #[test]
    fn lost_qualification_retains_responsibility_and_controlled_management() {
        let mut item = view();
        let version = item.task_version.clone();
        block_owner(&mut item);
        assert_eq!(item.owner_user_id.as_deref(), Some("original-owner"));
        assert_eq!(item.owner_organization_id, "company");
        assert_eq!(item.task_version, version);
        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.processing_state, ProcessingState::ExecutionBlocked);
        assert_eq!(
            item.allowed_actions,
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Reassign, WorkItemAllowedAction::Close]
        );
    }

    #[test]
    fn blocked_approval_never_gains_runtime_transfer_and_keeps_original_blocker() {
        let mut item = view();
        item.work_item_type = WorkItemType::DocumentApproval;
        item.processing_state = ProcessingState::ApprovalBlocked;
        let original =
            ProcessingBlockerView { code: "APPROVAL_BLOCKED".into(), message: "审批原阻断".into() };
        item.processing_blocker = Some(original.clone());
        block_owner(&mut item);
        assert_eq!(item.processing_state, ProcessingState::ApprovalBlocked);
        assert_eq!(item.processing_blocker, Some(original));
        assert_eq!(item.allowed_actions, vec![WorkItemAllowedAction::View]);
    }

    #[test]
    fn execution_uses_existing_permission_union_but_excludes_disabled_roles() {
        let item = view();
        let required = execution_permissions(&item).unwrap();
        assert!(required.len() > 1);
        let roles = vec!["role-a".to_string(), "role-b".to_string()];
        let split = RolePermissionSnapshotFact::new(
            roles.clone(),
            HashMap::from([
                (roles[0].clone(), vec![required[0].to_string()]),
                (roles[1].clone(), required[1..].iter().map(|code| code.to_string()).collect()),
            ]),
            1,
        );
        assert!(can_execute(&item, &split, &roles));
        assert!(!can_execute(&item, &split, &roles[..1]));
        let complete = RolePermissionSnapshotFact::new(
            roles.clone(),
            HashMap::from([(roles[0].clone(), required.iter().map(|code| code.to_string()).collect())]),
            1,
        );
        assert!(can_execute(&item, &complete, &roles));
        assert!(!can_execute(&item, &complete, &roles[1..]));
    }
}
