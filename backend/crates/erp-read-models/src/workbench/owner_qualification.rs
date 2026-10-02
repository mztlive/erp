//! 已授权管理视图中的订单任务负责人资格；只返回阻断摘要，不修改任务责任。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use erp_workflow::ports::{
    ObjectFactMap, ObjectKind, OrderTaskSource, RolePermissionSnapshotFact, WorkflowAccountFact,
};
use erp_workflow::service::approval::approval_participant_permissions_with_executor;
use erp_workflow::service::work_item::access::required_execution_permissions;
use persistence_core::Executor;

use super::action_projection::ActionProjection;
use super::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemView, WorkbenchReadService,
};
use crate::{Error, Result};

/// 一个统计扫描阶段内最多复用 128 个负责人的角色事实，不保存对象授权结论。
pub(super) struct QualificationCache {
    enabled: bool,
    owners: HashMap<String, OwnerPermissions>,
}

/// 权限集覆盖与 policy revision 同时匹配才可复用；启用角色沿用原事务快照。
#[derive(Clone)]
struct OwnerPermissions {
    kind: AccountKind,
    required: BTreeSet<&'static str>,
    grants: RolePermissionSnapshotFact,
    enabled_roles: Vec<String>,
}

impl QualificationCache {
    /// 只在调用方事务执行器内启用；每个完整扫描创建独立缓存。
    pub(super) fn new(executor: &mut dyn Executor) -> Self {
        Self { enabled: executor.session().is_some(), owners: HashMap::new() }
    }

    /// 页面单批资格验证沿用逐次读取合同，不启用跨批复用。
    fn disabled() -> Self {
        Self { enabled: false, owners: HashMap::new() }
    }

    /// 返回与当前账号类型和权限需求一致的候选，版本仍由调用方实时校验。
    fn candidate(
        &self,
        account: &WorkflowAccountFact,
        required: &BTreeSet<&'static str>,
    ) -> Option<&OwnerPermissions> {
        self.owners
            .get(&account.id)
            .filter(|facts| facts.kind == account.kind && required.is_subset(&facts.required))
    }

    /// 达到固定上限后释放上一批缓存；不会累计所有任务、负责人或来源对象。
    fn insert(&mut self, owner: &str, facts: OwnerPermissions) {
        if !self.enabled {
            return;
        }
        if self.owners.len() >= 128 && !self.owners.contains_key(owner) {
            self.owners.clear();
        }
        self.owners.insert(owner.to_string(), facts);
    }
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 对当前页的订单关联任务按负责人分组重验；保留原责任和受控管理动作。
    ///
    /// # 参数
    /// `items` 为已授权当前页，`executor` 沿用页面事实读取快照。
    /// # 返回
    /// 原地标记负责人资格失效并保留合法管理动作。
    /// # 错误
    /// 账号、权限或业务来源资格读取失败时传播原错误。
    pub(super) async fn apply_owner_qualification(
        &self,
        items: &mut [WorkItemView],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut actions = items.iter().map(ActionProjection::from_view).collect::<Vec<_>>();
        self.qualify_approvals(&mut actions, executor).await?;
        let keys = qualification_keys(&actions);
        if !keys.is_empty() {
            let facts = self.facts_reader().load(&keys, executor).await?;
            let groups = owner_groups(&actions, &facts)?;
            self.qualify_owner_groups(&mut actions, groups, &mut QualificationCache::disabled(), executor)
                .await?;
        }
        for (action, item) in actions.into_iter().zip(items) {
            action.apply(item);
        }
        Ok(())
    }

    /// 统计复用页面资格重验，直接消费本批已读的权威来源。
    ///
    /// # 参数
    /// `items` 为最小动作投影，`facts` 为同一执行器中的对象权威事实。
    /// # 返回
    /// 原地阻断失去账号、执行权限或订单读取资格的负责人。
    /// # 错误
    /// 当前账号、权限、来源资格读取失败时传播原错误。
    pub(super) async fn apply_action_qualification(
        &self,
        items: &mut [ActionProjection],
        facts: &ObjectFactMap,
        qualification: &mut QualificationCache,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.qualify_approvals(items, executor).await?;
        let groups = owner_groups(items, facts)?;
        self.qualify_owner_groups(items, groups, qualification, executor).await
    }

    /// 任务链已由工作流服务校验；投影仅复核当前审批人账号和静态资格。
    async fn qualify_approvals(
        &self,
        items: &mut [ActionProjection],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let owners = items
            .iter()
            .filter(|item| item.status == WorkItemStatus::Open && item.work_item_type.is_document_approval())
            .filter_map(|item| item.owner_user_id.clone())
            .collect::<BTreeSet<_>>();
        if owners.is_empty() {
            return Ok(());
        }
        let accounts = self
            .auth
            .load_accounts(&owners.into_iter().collect::<Vec<_>>(), executor)
            .await?
            .into_iter()
            .map(|account| (account.id.clone(), account))
            .collect::<HashMap<_, _>>();
        let mut eligible = HashMap::new();
        for account in accounts.values() {
            let actor = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
            eligible.insert(
                account.id.clone(),
                account.is_active_backoffice()
                    && approval_participant_permissions_with_executor(&self.auth, &actor, executor).await?,
            );
        }
        for item in items
            .iter_mut()
            .filter(|item| item.status == WorkItemStatus::Open && item.work_item_type.is_document_approval())
        {
            if !item.owner_user_id.as_ref().is_some_and(|owner| eligible.get(owner) == Some(&true)) {
                block_action_owner(item);
            }
        }
        Ok(())
    }

    /// 账号批量读取；同一负责人只解析一次权限和每种订单的详情范围。
    async fn qualify_owner_groups(
        &self,
        items: &mut [ActionProjection],
        groups: OwnerGroups,
        qualification: &mut QualificationCache,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if groups.is_empty() {
            return Ok(());
        }
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
                    block_action_owner(&mut items[index]);
                }
                continue;
            };
            self.qualify_owner(items, account, &tasks, qualification, executor).await?;
        }
        Ok(())
    }

    /// 读取权与完整执行权分别验证；部门相同不能补执行资格。
    async fn qualify_owner(
        &self,
        items: &mut [ActionProjection],
        account: &WorkflowAccountFact,
        tasks: &[(usize, OrderTaskSource)],
        qualification: &mut QualificationCache,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let actor = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
        let required = tasks
            .iter()
            .filter_map(|(index, _)| execution_permissions(&items[*index]))
            .flatten()
            .collect::<BTreeSet<_>>();
        let permissions = owner_permissions(&self.auth, account, required, qualification, executor).await?;
        let sources = tasks.iter().map(|(_, source)| source.clone()).collect::<BTreeSet<_>>();
        let readable = self.auth.readable_order_sources(&actor, &sources, executor).await?;
        for (index, source) in tasks {
            let item = &mut items[*index];
            let executable = can_execute(item, &permissions.grants, &permissions.enabled_roles);
            let fulfillment = item.work_item_type == WorkItemType::FulfillmentOperation;
            if !order_source_allows_execution(readable.contains(source), executable, fulfillment)
                || !executable
            {
                block_action_owner(item);
            }
        }
        Ok(())
    }
}

/// 同拍同策略版本复用共同角色事实；来源资格仍由每批的精确订单授权独立证明。
async fn owner_permissions(
    auth: &impl WorkflowAuthorizationPort,
    account: &WorkflowAccountFact,
    required: BTreeSet<&'static str>,
    qualification: &mut QualificationCache,
    executor: &mut dyn Executor,
) -> Result<OwnerPermissions> {
    if let Some(facts) = qualification.candidate(account, &required)
        && auth.current_policy_revision().await? == facts.grants.policy_revision()
    {
        return Ok(facts.clone());
    }
    let grants = auth
        .role_permission_snapshot(account.kind, &account.id, &required.iter().copied().collect::<Vec<_>>())
        .await?;
    let enabled_roles = auth.enabled_role_ids(grants.role_ids(), executor).await?;
    let facts = OwnerPermissions { kind: account.kind, required, grants, enabled_roles };
    qualification.insert(&account.id, facts.clone());
    Ok(facts)
}

/// 本人履约任务在仍有执行权时，不因缺少销售单详情而失效。
///
/// 仓发的订单来源是销售单。仓储经办人没有销售单详情权限，任务却派给本人。
/// 非审批、非履约任务仍必须能读来源订单；审批另走精确任务授权。
fn order_source_allows_execution(readable: bool, executable: bool, fulfillment: bool) -> bool {
    readable || (fulfillment && executable)
}

/// 固定任务类型提供执行权限；审批额外要求静态决定权限。
fn execution_permissions(item: &ActionProjection) -> Option<Vec<&'static str>> {
    let mut required =
        required_execution_permissions(item.work_item_type, &item.business_object_type)?.to_vec();
    if item.work_item_type.is_document_approval() {
        required.push("approval_instance:decide");
    }
    Some(required)
}

/// 按既有任务执行合同逐权限检查当前有效角色；订单范围另由公共解析器校验。
fn can_execute(
    item: &ActionProjection,
    grants: &RolePermissionSnapshotFact,
    enabled_roles: &[String],
) -> bool {
    let Some(required) = execution_permissions(item) else {
        return false;
    };
    required.iter().all(|permission| {
        grants.granting_role_ids(permission).iter().any(|role| enabled_roles.contains(role))
    })
}

type OwnerGroups = BTreeMap<String, Vec<(usize, OrderTaskSource)>>;

/// 精确的业务来源与当前负责人分组；不从任务展示组织推断内部组织。
fn owner_groups(items: &[ActionProjection], facts: &ObjectFactMap) -> Result<OwnerGroups> {
    let mut groups = BTreeMap::<String, Vec<(usize, OrderTaskSource)>>::new();
    for (index, item) in items.iter().enumerate() {
        let Some(relation) = item.work_item_type.brief_relation(&item.business_object_type) else {
            continue;
        };
        if item.status != WorkItemStatus::Open
            || item.work_item_type.is_document_approval()
            || !OrderTaskSource::required_for(relation.object_kind)
        {
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
fn block_action_owner(item: &mut ActionProjection) {
    let approval = item.work_item_type.is_document_approval() || item.approval_node_execution_id.is_some();
    item.access.allowed_actions.retain(|action| {
        *action == WorkItemAllowedAction::View
            || (!approval && matches!(action, WorkItemAllowedAction::Reassign | WorkItemAllowedAction::Close))
    });
    if item.access.processing_state == ProcessingState::ApprovalBlocked {
        return;
    }
    let blocker = ProcessingBlockerView {
        code: "WORK_ITEM_OWNER_INELIGIBLE".into(),
        message: "当前负责人已失去账号、执行权限或关联订单读取资格，需由管理人员处理".into(),
    };
    item.access.processing_state = ProcessingState::ExecutionBlocked;
    item.access.processing_blocker = Some(blocker.clone());
    item.access.action_blockers.push(blocker);
}

/// 当前页普通任务仅装载资格判定必需的订单来源。
fn qualification_keys(items: &[ActionProjection]) -> HashSet<(ObjectKind, String)> {
    items
        .iter()
        .filter(|item| item.status == WorkItemStatus::Open && !item.work_item_type.is_document_approval())
        .filter_map(|item| {
            item.work_item_type
                .brief_relation(&item.business_object_type)
                .filter(|relation| OrderTaskSource::required_for(relation.object_kind))
                .map(|relation| (relation.object_kind, item.business_object_id.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };

    use super::*;
    use crate::workbench::dto::WorkItemFields;
    use crate::workbench::test_auth::TestAuth;

    /// 同扫描阶段命中只复用共同角色事实，policy 变化或权限扩展仍重新读取。
    #[tokio::test]
    async fn owner_permissions_reuse_requires_matching_revision_and_permission_coverage() {
        let auth = TestAuth::default();
        auth.revision.store(1, Ordering::SeqCst);
        let account = WorkflowAccountFact::new("owner", AccountKind::Admin, true);
        let action = ActionProjection::from_view(&view());
        let required = execution_permissions(&action).unwrap().into_iter().collect::<BTreeSet<_>>();
        let mut cache = QualificationCache { enabled: true, owners: HashMap::new() };
        let mut executor = persistence_core::NoTransaction;
        let first =
            owner_permissions(&auth, &account, required.clone(), &mut cache, &mut executor).await.unwrap();
        let repeated = owner_permissions(
            &auth,
            &account,
            BTreeSet::from([*required.first().unwrap()]),
            &mut cache,
            &mut executor,
        )
        .await
        .unwrap();
        assert_eq!(first.grants.policy_revision(), repeated.grants.policy_revision());
        assert!(can_execute(&action, &first.grants, &first.enabled_roles));
        assert!(can_execute(&action, &repeated.grants, &repeated.enabled_roles));
        assert_eq!(*auth.trace.lock().unwrap(), vec!["snapshot:owner", "enabled_roles", "revision"]);
        auth.revision.store(2, Ordering::SeqCst);
        let refreshed =
            owner_permissions(&auth, &account, required, &mut cache, &mut executor).await.unwrap();
        assert_eq!(refreshed.grants.policy_revision(), 2);
        assert_eq!(&auth.trace.lock().unwrap()[3..], &["revision", "snapshot:owner", "enabled_roles"]);
        let expanded = owner_permissions(
            &auth,
            &account,
            BTreeSet::from(["electronic_delivery:confirm"]),
            &mut cache,
            &mut executor,
        )
        .await
        .unwrap();
        assert_eq!(expanded.grants.granting_role_ids("electronic_delivery:confirm"), vec!["executor"]);
        assert_eq!(&auth.trace.lock().unwrap()[6..], &["snapshot:owner", "enabled_roles"]);
    }

    /// revision 读取失败不得回退到缓存授权，也不得进入后续资格步骤。
    #[tokio::test]
    async fn owner_permission_cache_fails_closed_on_revision_failure() {
        let auth = TestAuth::default();
        let account = WorkflowAccountFact::new("owner", AccountKind::Admin, true);
        let mut cache = QualificationCache { enabled: true, owners: HashMap::new() };
        let mut executor = persistence_core::NoTransaction;
        owner_permissions(&auth, &account, BTreeSet::new(), &mut cache, &mut executor).await.unwrap();
        auth.fail_revision.store(true, Ordering::SeqCst);
        let result = owner_permissions(&auth, &account, BTreeSet::new(), &mut cache, &mut executor).await;
        assert!(matches!(result, Err(Error::Rbac(message)) if message == "revision failed"));
        assert_eq!(*auth.trace.lock().unwrap(), vec!["snapshot:owner", "enabled_roles", "revision"]);
    }

    /// 非事务执行器不复用，事务阶段缓存最多保留固定数量的负责人事实。
    #[tokio::test]
    async fn owner_permission_cache_is_bounded_and_disabled_without_transaction() {
        let auth = TestAuth::default();
        let account = WorkflowAccountFact::new("owner", AccountKind::Admin, true);
        let mut executor = persistence_core::NoTransaction;
        let mut cache = QualificationCache::new(&mut executor);
        for _ in 0..2 {
            owner_permissions(&auth, &account, BTreeSet::new(), &mut cache, &mut executor).await.unwrap();
        }
        assert!(cache.owners.is_empty());
        assert_eq!(
            *auth.trace.lock().unwrap(),
            vec!["snapshot:owner", "enabled_roles", "snapshot:owner", "enabled_roles"]
        );
        let mut cache = QualificationCache { enabled: true, owners: HashMap::new() };
        let facts = OwnerPermissions {
            kind: AccountKind::Admin,
            required: BTreeSet::new(),
            grants: RolePermissionSnapshotFact::new(Vec::new(), HashMap::new(), 1),
            enabled_roles: Vec::new(),
        };
        for index in 0..128 {
            cache.insert(&format!("owner-{index}"), facts.clone());
        }
        assert_eq!(cache.owners.len(), 128);
        cache.insert("owner-new", facts);
        assert_eq!(cache.owners.len(), 1);
    }

    /// 与正式页面相同，资格重验只回填动作和状态。
    fn block_owner_view(item: &mut WorkItemView) {
        let mut action = ActionProjection::from_view(item);
        block_action_owner(&mut action);
        action.apply(item);
    }

    #[test]
    fn assigned_fulfillment_stays_executable_without_sales_order_detail() {
        assert!(order_source_allows_execution(false, true, true));
        assert!(!order_source_allows_execution(false, false, true));
        assert!(!order_source_allows_execution(false, true, false));
        assert!(order_source_allows_execution(true, true, false));
    }

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
        block_owner_view(&mut item);
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
        block_owner_view(&mut item);
        assert_eq!(item.processing_state, ProcessingState::ApprovalBlocked);
        assert_eq!(item.processing_blocker, Some(original));
        assert_eq!(item.allowed_actions, vec![WorkItemAllowedAction::View]);
    }

    #[test]
    fn execution_uses_existing_permission_union_but_excludes_disabled_roles() {
        let item = view();
        let required = execution_permissions(&ActionProjection::from_view(&item)).unwrap();
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
        assert!(can_execute(&ActionProjection::from_view(&item), &split, &roles));
        assert!(!can_execute(&ActionProjection::from_view(&item), &split, &roles[..1]));
        let complete = RolePermissionSnapshotFact::new(
            roles.clone(),
            HashMap::from([(roles[0].clone(), required.iter().map(|code| code.to_string()).collect())]),
            1,
        );
        assert!(can_execute(&ActionProjection::from_view(&item), &complete, &roles));
        assert!(!can_execute(&ActionProjection::from_view(&item), &complete, &roles[1..]));
    }
}
