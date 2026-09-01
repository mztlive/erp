//! 库存列表读取、调整动作与 Warehouse DataScope 授权边界。
//!
//! 本模块只把 IAM/RBAC 与 DataScope 事实收敛为 Service 可消费的仓库范围；
//! Repository 仅接收已证明的仓库过滤，不解释 actor、角色或权限。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, Executor};
use entities::{
    access_control::{DataScope, DataScopeSubjectType, OrganizationCoverage},
    ids::WarehouseId,
    Permission,
};
use mongodb::Database;

use crate::{
    approval::approval_actor_is_active_with_executor, audit::AuditActor, errors::Result, iam::RbacService,
};

const DETAIL_PERMISSION: &str = "stock_adjustment:detail";
const ADJUSTMENT_LIST_PERMISSION: &str = "stock_adjustment:list";
const CREATE_PERMISSION: &str = "stock_adjustment:create";
const UPDATE_PERMISSION: &str = "stock_adjustment:update";
const BALANCE_LIST_PERMISSION: &str = "stock_balance:list";
const BALANCE_DETAIL_PERMISSION: &str = "stock_balance:detail";
const MOVEMENT_LIST_PERMISSION: &str = "stock_movement:list";
const RESERVATION_LIST_PERMISSION: &str = "stock_reservation:list";

/// 已由同一授权快照证明的仓库范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WarehouseScope(Option<OrganizationCoverage>);

impl WarehouseScope {
    /// 判断目标仓库是否落在已证明范围内。
    pub(crate) fn covers(&self, warehouse_id: &str) -> bool {
        self.0
            .as_ref()
            .is_some_and(|coverage| coverage.covers(warehouse_id))
    }

    /// 把调用方精确筛选与授权范围求交，形成 Repository 查询过滤。
    ///
    /// `None` 仅表示公司级且调用方未指定仓库；`Some([])` 必须由 Repository
    /// 解释为空结果，禁止退化为全量查询。
    pub(crate) fn repository_warehouse_ids(
        &self,
        requested: Option<WarehouseId>,
    ) -> Option<Vec<WarehouseId>> {
        match (&self.0, requested) {
            (Some(OrganizationCoverage::All), None) => None,
            (Some(coverage), Some(id)) if coverage.covers(id.as_ref()) => Some(vec![id]),
            (Some(OrganizationCoverage::Targets(allowed)), None) => {
                Some(allowed.iter().cloned().map(WarehouseId::new).collect())
            }
            (Some(OrganizationCoverage::All), Some(id)) => Some(vec![id]),
            (None, _) | (Some(OrganizationCoverage::Targets(_)), Some(_)) => Some(Vec::new()),
        }
    }

    fn empty() -> Self {
        Self(None)
    }

    fn company() -> Self {
        Self(Some(OrganizationCoverage::All))
    }

    fn from_targets(targets: Vec<String>) -> Self {
        Self(OrganizationCoverage::from_targets(targets))
    }
}

/// 各库存列表读取范围，以及库存调整读取、创建、更新的同角色联合范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryAuthorization {
    actor_active: bool,
    balance_list_scope: WarehouseScope,
    balance_detail_scope: WarehouseScope,
    movement_list_scope: WarehouseScope,
    reservation_list_scope: WarehouseScope,
    adjustment_list_scope: WarehouseScope,
    read_scope: WarehouseScope,
    create_scope: WarehouseScope,
    update_scope: WarehouseScope,
}

impl InventoryAuthorization {
    /// 判断认证身份在事务快照内是否仍对应可登录账号。
    pub(crate) fn actor_is_active(&self) -> bool {
        self.actor_active
    }

    /// 返回库存余额列表的仓库范围。
    pub(crate) fn balance_list_scope(&self) -> &WarehouseScope {
        &self.balance_list_scope
    }

    /// 判断当前账号是否可读取目标仓库的库存余额详情。
    pub(crate) fn can_read_balance_detail(&self, warehouse_id: &str) -> bool {
        self.balance_detail_scope.covers(warehouse_id)
    }

    /// 返回库存流水列表的仓库范围。
    pub(crate) fn movement_list_scope(&self) -> &WarehouseScope {
        &self.movement_list_scope
    }

    /// 返回库存预占列表的仓库范围。
    pub(crate) fn reservation_list_scope(&self) -> &WarehouseScope {
        &self.reservation_list_scope
    }

    /// 返回库存调整列表的联合 `list + detail` 仓库范围。
    pub(crate) fn adjustment_list_scope(&self) -> &WarehouseScope {
        &self.adjustment_list_scope
    }

    /// 返回对象读取仓库范围。
    pub(crate) fn read_scope(&self) -> &WarehouseScope {
        &self.read_scope
    }

    /// 判断当前账号是否可在目标仓库创建库存调整。
    pub(crate) fn can_create(&self, warehouse_id: &str) -> bool {
        self.create_scope.covers(warehouse_id)
    }

    /// 判断当前账号是否可更新目标仓库的库存调整。
    pub(crate) fn can_update(&self, warehouse_id: &str) -> bool {
        self.update_scope.covers(warehouse_id)
    }
}

/// 在调用方执行器快照内形成库存读取及调整读写授权。
///
/// `create_scope`/`update_scope` 只采用同一个启用角色同时授予 `detail` 与目标
/// 动作的范围，禁止把不同角色的权限或 DataScope 拼接。
pub(crate) async fn inventory_authorization_with_executor(
    db: &Database,
    rbac: &RbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<InventoryAuthorization> {
    if !approval_actor_is_active_with_executor(db, actor, executor).await? {
        return Ok(InventoryAuthorization::inactive());
    }
    let detail = Permission::parse(DETAIL_PERMISSION)?;
    let adjustment_list = Permission::parse(ADJUSTMENT_LIST_PERMISSION)?;
    let create = Permission::parse(CREATE_PERMISSION)?;
    let update = Permission::parse(UPDATE_PERMISSION)?;
    let balance_list = Permission::parse(BALANCE_LIST_PERMISSION)?;
    let balance_detail = Permission::parse(BALANCE_DETAIL_PERMISSION)?;
    let movement_list = Permission::parse(MOVEMENT_LIST_PERMISSION)?;
    let reservation_list = Permission::parse(RESERVATION_LIST_PERMISSION)?;
    let permissions = [
        detail.clone(),
        adjustment_list.clone(),
        create.clone(),
        update.clone(),
        balance_list.clone(),
        balance_detail.clone(),
        movement_list.clone(),
        reservation_list.clone(),
    ];
    let snapshot = rbac
        .role_permission_snapshot(actor.kind(), actor.id(), &permissions)
        .await?;
    let enabled = enabled_role_ids(db, snapshot.role_ids(), executor).await?;
    let balance_list_roles = enabled_grants(snapshot.granting_role_ids(&balance_list), &enabled);
    let balance_detail_roles = enabled_grants(snapshot.granting_role_ids(&balance_detail), &enabled);
    let movement_list_roles = enabled_grants(snapshot.granting_role_ids(&movement_list), &enabled);
    let reservation_list_roles = enabled_grants(snapshot.granting_role_ids(&reservation_list), &enabled);
    let adjustment_list_roles = enabled_grants(
        snapshot.granting_role_ids_for_all(&[adjustment_list, detail.clone()]),
        &enabled,
    );
    let read_roles = enabled_grants(snapshot.granting_role_ids(&detail), &enabled);
    let create_roles = enabled_grants(
        snapshot.granting_role_ids_for_all(&[detail.clone(), create]),
        &enabled,
    );
    let update_roles = enabled_grants(snapshot.granting_role_ids_for_all(&[detail, update]), &enabled);
    let user_scopes = db
        .data_scopes()
        .list_by_subject(DataScopeSubjectType::User, actor.id(), executor)
        .await?;
    let role_scopes = load_role_scopes(
        db,
        [
            balance_list_roles.as_slice(),
            balance_detail_roles.as_slice(),
            movement_list_roles.as_slice(),
            reservation_list_roles.as_slice(),
            adjustment_list_roles.as_slice(),
            read_roles.as_slice(),
            create_roles.as_slice(),
            update_roles.as_slice(),
        ],
        executor,
    )
    .await?;
    let authorization = InventoryAuthorization {
        actor_active: true,
        balance_list_scope: scope_from_role_facts(&user_scopes, &balance_list_roles, &role_scopes),
        balance_detail_scope: scope_from_role_facts(&user_scopes, &balance_detail_roles, &role_scopes),
        movement_list_scope: scope_from_role_facts(&user_scopes, &movement_list_roles, &role_scopes),
        reservation_list_scope: scope_from_role_facts(&user_scopes, &reservation_list_roles, &role_scopes),
        adjustment_list_scope: scope_from_role_facts(&user_scopes, &adjustment_list_roles, &role_scopes),
        read_scope: scope_from_role_facts(&user_scopes, &read_roles, &role_scopes),
        create_scope: scope_from_role_facts(&user_scopes, &create_roles, &role_scopes),
        update_scope: scope_from_role_facts(&user_scopes, &update_roles, &role_scopes),
    };
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor)
        .await?;
    Ok(authorization)
}

impl InventoryAuthorization {
    fn inactive() -> Self {
        Self {
            actor_active: false,
            balance_list_scope: WarehouseScope::empty(),
            balance_detail_scope: WarehouseScope::empty(),
            movement_list_scope: WarehouseScope::empty(),
            reservation_list_scope: WarehouseScope::empty(),
            adjustment_list_scope: WarehouseScope::empty(),
            read_scope: WarehouseScope::empty(),
            create_scope: WarehouseScope::empty(),
            update_scope: WarehouseScope::empty(),
        }
    }
}

/// 从 policy 角色中只保留事务快照内仍启用的角色。
async fn enabled_role_ids(
    db: &Database,
    role_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashSet<String>> {
    Ok(db
        .roles()
        .enabled_roles(role_ids, executor)
        .await?
        .into_iter()
        .map(|role| role.base.id)
        .collect())
}

fn enabled_grants(role_ids: Vec<String>, enabled: &HashSet<String>) -> Vec<String> {
    role_ids
        .into_iter()
        .filter(|role_id| enabled.contains(role_id))
        .collect()
}

/// 一次批量读取全部候选授权角色的范围事实。
async fn load_role_scopes<const N: usize>(
    db: &Database,
    role_sets: [&[String]; N],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Vec<DataScope>>> {
    let mut role_ids = role_sets.into_iter().flatten().cloned().collect::<Vec<_>>();
    role_ids.sort();
    role_ids.dedup();
    let scopes = db
        .data_scopes()
        .list_by_subjects(DataScopeSubjectType::Role, &role_ids, executor)
        .await?;
    Ok(scopes_by_subject(scopes))
}

/// 对每个授权角色分别执行角色范围与用户范围交集，再合并仓库覆盖。
fn scope_from_role_facts(
    user_scopes: &[DataScope],
    permitted_role_ids: &[String],
    scopes_by_role: &HashMap<String, Vec<DataScope>>,
) -> WarehouseScope {
    // 默认政策由 Service 持有：用户未配置显式范围时为 All；角色缺失范围失败关闭。
    let user = OrganizationCoverage::from_scopes(user_scopes).unwrap_or(OrganizationCoverage::All);
    let mut warehouses = Vec::new();
    for role_id in permitted_role_ids {
        let Some(role) = scopes_by_role
            .get(role_id)
            .and_then(|scopes| OrganizationCoverage::from_scopes(scopes))
        else {
            continue;
        };
        match role.intersect(&user) {
            Some(OrganizationCoverage::All) => return WarehouseScope::company(),
            Some(OrganizationCoverage::Targets(targets)) => warehouses.extend(targets),
            None => {}
        }
    }
    WarehouseScope::from_targets(warehouses)
}

fn scopes_by_subject(scopes: Vec<DataScope>) -> HashMap<String, Vec<DataScope>> {
    let mut grouped = HashMap::new();
    for scope in scopes {
        grouped
            .entry(scope.subject_id.clone())
            .or_insert_with(Vec::new)
            .push(scope);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{scope_from_role_facts, scopes_by_subject, InventoryAuthorization, WarehouseScope};
    use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
    use entities::ids::{DataScopeId, WarehouseId};

    fn scope(
        id: &str,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        scope_type: DataScopeType,
        targets: &[&str],
    ) -> DataScope {
        DataScope::new(
            DataScopeId::new(id),
            DataScopeData {
                subject_type,
                subject_id: subject_id.to_string(),
                scope_type,
                scope_targets: targets.iter().map(|item| (*item).to_string()).collect(),
            },
        )
        .unwrap()
    }

    #[test]
    fn company_and_single_warehouse_scopes_form_repository_filters() {
        let company = WarehouseScope::company();
        assert_eq!(company.repository_warehouse_ids(None), None);
        assert_eq!(
            company.repository_warehouse_ids(Some(WarehouseId::new("warehouse-1"))),
            Some(vec![WarehouseId::new("warehouse-1")])
        );

        let single = WarehouseScope::from_targets(vec!["warehouse-1".to_string()]);
        assert_eq!(
            single.repository_warehouse_ids(None),
            Some(vec![WarehouseId::new("warehouse-1")])
        );
        assert_eq!(
            single.repository_warehouse_ids(Some(WarehouseId::new("warehouse-2"))),
            Some(Vec::new())
        );
    }

    #[test]
    fn role_and_user_scopes_intersect_before_roles_are_unioned() {
        let user = vec![scope(
            "user-scope",
            DataScopeSubjectType::User,
            "user-1",
            DataScopeType::Organization,
            &["warehouse-2"],
        )];
        let role_scopes = scopes_by_subject(vec![
            scope(
                "role-a-scope",
                DataScopeSubjectType::Role,
                "role-a",
                DataScopeType::Organization,
                &["warehouse-1", "warehouse-2"],
            ),
            scope(
                "role-b-scope",
                DataScopeSubjectType::Role,
                "role-b",
                DataScopeType::Company,
                &[],
            ),
        ]);
        let result =
            scope_from_role_facts(&user, &["role-a".to_string(), "role-b".to_string()], &role_scopes);
        assert_eq!(
            result,
            WarehouseScope::from_targets(vec!["warehouse-2".to_string()])
        );
    }

    #[test]
    fn missing_role_scope_fails_closed_even_when_user_scope_is_unrestricted() {
        let result = scope_from_role_facts(&[], &["role-a".to_string()], &HashMap::new());
        assert_eq!(result, WarehouseScope::empty());
    }

    #[test]
    fn company_role_scope_is_company_when_user_has_no_explicit_cap() {
        let role_scopes = scopes_by_subject(vec![scope(
            "role-company",
            DataScopeSubjectType::Role,
            "role-a",
            DataScopeType::Company,
            &[],
        )]);
        assert_eq!(
            scope_from_role_facts(&[], &["role-a".to_string()], &role_scopes),
            WarehouseScope::company()
        );
    }

    #[test]
    fn balance_read_scope_is_independent_from_adjustment_read_and_create_scopes() {
        let authorization = InventoryAuthorization {
            actor_active: true,
            balance_list_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            balance_detail_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            movement_list_scope: WarehouseScope::empty(),
            reservation_list_scope: WarehouseScope::empty(),
            adjustment_list_scope: WarehouseScope::empty(),
            read_scope: WarehouseScope::empty(),
            create_scope: WarehouseScope::empty(),
            update_scope: WarehouseScope::empty(),
        };
        assert!(authorization.can_read_balance_detail("warehouse-1"));
        assert!(!authorization.read_scope().covers("warehouse-1"));
        assert!(!authorization.can_create("warehouse-1"));
    }

    #[test]
    fn adjustment_detail_scope_does_not_imply_list_scope() {
        let authorization = InventoryAuthorization {
            actor_active: true,
            balance_list_scope: WarehouseScope::empty(),
            balance_detail_scope: WarehouseScope::empty(),
            movement_list_scope: WarehouseScope::empty(),
            reservation_list_scope: WarehouseScope::empty(),
            adjustment_list_scope: WarehouseScope::empty(),
            read_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            create_scope: WarehouseScope::empty(),
            update_scope: WarehouseScope::empty(),
        };

        assert!(authorization.read_scope().covers("warehouse-1"));
        assert_eq!(
            authorization
                .adjustment_list_scope()
                .repository_warehouse_ids(None),
            Some(Vec::new())
        );
    }

    #[test]
    fn movement_and_reservation_list_scopes_are_independent() {
        let authorization = InventoryAuthorization {
            actor_active: true,
            balance_list_scope: WarehouseScope::empty(),
            balance_detail_scope: WarehouseScope::empty(),
            movement_list_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            reservation_list_scope: WarehouseScope::from_targets(vec!["warehouse-2".to_string()]),
            adjustment_list_scope: WarehouseScope::empty(),
            read_scope: WarehouseScope::empty(),
            create_scope: WarehouseScope::empty(),
            update_scope: WarehouseScope::empty(),
        };

        assert!(authorization.movement_list_scope().covers("warehouse-1"));
        assert!(!authorization.movement_list_scope().covers("warehouse-2"));
        assert!(authorization.reservation_list_scope().covers("warehouse-2"));
        assert!(!authorization.reservation_list_scope().covers("warehouse-1"));
    }
}
