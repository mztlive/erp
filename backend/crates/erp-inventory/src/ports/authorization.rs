//! Consumer port for warehouse-scoped inventory authorization facts.

use std::collections::BTreeSet;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::WarehouseId;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Port inventory uses to compute warehouse-scoped authorization from identity facts.
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// Return inventory warehouse scopes for `actor` on the caller executor snapshot.
    ///
    /// # Parameters
    /// * `actor` - authenticated audit actor
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Identity, policy or data-scope lookup failures.
    async fn authorize(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<InventoryAuthorization>;
}

/// Fail-closed authorization port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuthorizationPort;

#[async_trait]
impl AuthorizationPort for FailClosedAuthorizationPort {
    async fn authorize(
        &self,
        _actor: &AuditActor,
        _executor: &mut dyn Executor,
    ) -> Result<InventoryAuthorization> {
        Err(Error::Internal("库存授权端口未接线".to_string()))
    }
}

/// Local warehouse coverage used by inventory filters; not an identity aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WarehouseCoverage {
    All,
    Targets(Vec<String>),
}

impl WarehouseCoverage {
    fn from_targets(targets: impl IntoIterator<Item = String>) -> Option<Self> {
        let targets = targets.into_iter().collect::<BTreeSet<_>>();
        if targets.contains("*") {
            return Some(Self::All);
        }
        (!targets.is_empty()).then(|| Self::Targets(targets.into_iter().collect()))
    }

    fn covers(&self, warehouse_id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Targets(targets) => {
                targets.binary_search_by(|target| target.as_str().cmp(warehouse_id)).is_ok()
            },
        }
    }
}

/// 已由同一授权快照证明的仓库范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarehouseScope(Option<WarehouseCoverage>);

/// 列表动作的授权指纹；不含内部证明或全量人员集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryScopeMeta {
    scope_version: String,
    policy_version: u64,
    organization_version: u64,
    as_of: String,
}

impl InventoryScopeMeta {
    /// 无授权快照时的空元信息。
    pub fn empty() -> Self {
        Self {
            scope_version: String::new(),
            policy_version: 0,
            organization_version: 0,
            as_of: String::new(),
        }
    }

    /// 由公共解析结果构造列表元信息。
    ///
    /// # 参数
    /// * `scope_version` - 授权指纹
    /// * `policy_version` - 权限策略版本
    /// * `organization_version` - 组织关系版本
    /// * `as_of` - 授权时点
    ///
    /// # 返回
    /// 返回可写入列表信封的元信息。
    pub fn new(
        scope_version: impl Into<String>,
        policy_version: u64,
        organization_version: u64,
        as_of: impl Into<String>,
    ) -> Self {
        Self {
            scope_version: scope_version.into(),
            policy_version,
            organization_version,
            as_of: as_of.into(),
        }
    }

    /// 当前资源动作的范围指纹。
    pub fn scope_version(&self) -> &str {
        &self.scope_version
    }

    /// 权限策略版本。
    pub fn policy_version(&self) -> u64 {
        self.policy_version
    }

    /// 组织关系版本。
    pub fn organization_version(&self) -> u64 {
        self.organization_version
    }

    /// 授权时点。
    pub fn as_of(&self) -> &str {
        &self.as_of
    }
}

impl WarehouseScope {
    /// 判断目标仓库是否落在已证明范围内。
    pub fn covers(&self, warehouse_id: &str) -> bool {
        self.0.as_ref().is_some_and(|coverage| coverage.covers(warehouse_id))
    }

    /// 缺仓库维或空目标时不贡献对象。
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// 把调用方精确筛选与授权范围求交，形成 Repository 查询过滤。
    ///
    /// `None` 仅表示公司级且调用方未指定仓库；`Some([])` 必须由 Repository
    /// 解释为空结果，禁止退化为全量查询。
    pub fn repository_warehouse_ids(&self, requested: Option<WarehouseId>) -> Option<Vec<WarehouseId>> {
        match (&self.0, requested) {
            (Some(WarehouseCoverage::All), None) => None,
            (Some(coverage), Some(id)) if coverage.covers(id.as_ref()) => Some(vec![id]),
            (Some(WarehouseCoverage::Targets(allowed)), None) => {
                Some(allowed.iter().cloned().map(WarehouseId::new).collect())
            },
            (Some(WarehouseCoverage::All), Some(id)) => Some(vec![id]),
            (None, _) | (Some(WarehouseCoverage::Targets(_)), Some(_)) => Some(Vec::new()),
        }
    }

    /// Empty scope that covers no warehouse.
    pub fn empty() -> Self {
        Self(None)
    }

    /// Company-wide scope.
    pub fn company() -> Self {
        Self(Some(WarehouseCoverage::All))
    }

    /// Scope from warehouse id targets; empty input is fail-closed.
    pub fn from_targets(targets: Vec<String>) -> Self {
        Self(WarehouseCoverage::from_targets(targets))
    }
}

/// 各库存列表读取范围，以及库存调整读取、创建、更新的同角色联合范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryAuthorization {
    actor_active: bool,
    balance_list_scope: WarehouseScope,
    balance_detail_scope: WarehouseScope,
    movement_list_scope: WarehouseScope,
    reservation_list_scope: WarehouseScope,
    adjustment_list_scope: WarehouseScope,
    read_scope: WarehouseScope,
    create_scope: WarehouseScope,
    update_scope: WarehouseScope,
    balance_list_meta: InventoryScopeMeta,
    movement_list_meta: InventoryScopeMeta,
    adjustment_list_meta: InventoryScopeMeta,
}

impl InventoryAuthorization {
    /// Inactive actor with empty warehouse scopes.
    pub fn inactive() -> Self {
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
            balance_list_meta: InventoryScopeMeta::empty(),
            movement_list_meta: InventoryScopeMeta::empty(),
            adjustment_list_meta: InventoryScopeMeta::empty(),
        }
    }

    /// Construct scopes computed by the composition adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn from_scopes(
        actor_active: bool,
        balance_list_scope: WarehouseScope,
        balance_detail_scope: WarehouseScope,
        movement_list_scope: WarehouseScope,
        reservation_list_scope: WarehouseScope,
        adjustment_list_scope: WarehouseScope,
        read_scope: WarehouseScope,
        create_scope: WarehouseScope,
        update_scope: WarehouseScope,
    ) -> Self {
        Self {
            actor_active,
            balance_list_scope,
            balance_detail_scope,
            movement_list_scope,
            reservation_list_scope,
            adjustment_list_scope,
            read_scope,
            create_scope,
            update_scope,
            balance_list_meta: InventoryScopeMeta::empty(),
            movement_list_meta: InventoryScopeMeta::empty(),
            adjustment_list_meta: InventoryScopeMeta::empty(),
        }
    }

    /// 绑定列表动作的授权指纹；人员筛选不改变仓库维。
    ///
    /// # 参数
    /// * `balance_list_meta` - 余额列表元信息
    /// * `movement_list_meta` - 流水列表元信息
    /// * `adjustment_list_meta` - 调整列表元信息
    ///
    /// # 返回
    /// 返回带范围信封的授权快照。
    pub fn with_list_meta(
        mut self,
        balance_list_meta: InventoryScopeMeta,
        movement_list_meta: InventoryScopeMeta,
        adjustment_list_meta: InventoryScopeMeta,
    ) -> Self {
        self.balance_list_meta = balance_list_meta;
        self.movement_list_meta = movement_list_meta;
        self.adjustment_list_meta = adjustment_list_meta;
        self
    }

    /// 判断认证身份在事务快照内是否仍对应可登录账号。
    pub fn actor_is_active(&self) -> bool {
        self.actor_active
    }

    /// 返回库存余额列表的仓库范围。
    pub fn balance_list_scope(&self) -> &WarehouseScope {
        &self.balance_list_scope
    }

    /// 判断当前账号是否可读取目标仓库的库存余额详情。
    pub fn can_read_balance_detail(&self, warehouse_id: &str) -> bool {
        self.balance_detail_scope.covers(warehouse_id)
    }

    /// 返回库存流水列表的仓库范围。
    pub fn movement_list_scope(&self) -> &WarehouseScope {
        &self.movement_list_scope
    }

    /// 返回库存预占列表的仓库范围。
    pub fn reservation_list_scope(&self) -> &WarehouseScope {
        &self.reservation_list_scope
    }

    /// 返回库存调整列表的联合 `list + detail` 仓库范围。
    pub fn adjustment_list_scope(&self) -> &WarehouseScope {
        &self.adjustment_list_scope
    }

    /// 返回对象读取仓库范围。
    pub fn read_scope(&self) -> &WarehouseScope {
        &self.read_scope
    }

    /// 判断当前账号是否可在目标仓库创建库存调整。
    pub fn can_create(&self, warehouse_id: &str) -> bool {
        self.create_scope.covers(warehouse_id)
    }

    /// 判断当前账号是否可更新目标仓库的库存调整。
    pub fn can_update(&self, warehouse_id: &str) -> bool {
        self.update_scope.covers(warehouse_id)
    }

    /// 余额列表授权指纹。
    pub fn balance_list_meta(&self) -> &InventoryScopeMeta {
        &self.balance_list_meta
    }

    /// 流水列表授权指纹。
    pub fn movement_list_meta(&self) -> &InventoryScopeMeta {
        &self.movement_list_meta
    }

    /// 调整列表授权指纹。
    pub fn adjustment_list_meta(&self) -> &InventoryScopeMeta {
        &self.adjustment_list_meta
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WarehouseId;

    use super::{InventoryAuthorization, WarehouseScope};

    #[test]
    fn company_and_single_warehouse_scopes_form_repository_filters() {
        let company = WarehouseScope::company();
        assert_eq!(company.repository_warehouse_ids(None), None);
        assert_eq!(
            company.repository_warehouse_ids(Some(WarehouseId::new("warehouse-1"))),
            Some(vec![WarehouseId::new("warehouse-1")])
        );

        let single = WarehouseScope::from_targets(vec!["warehouse-1".to_string()]);
        assert_eq!(single.repository_warehouse_ids(None), Some(vec![WarehouseId::new("warehouse-1")]));
        assert_eq!(single.repository_warehouse_ids(Some(WarehouseId::new("warehouse-2"))), Some(Vec::new()));
    }

    #[test]
    fn balance_read_scope_is_independent_from_adjustment_read_and_create_scopes() {
        let authorization = InventoryAuthorization::from_scopes(
            true,
            WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
        );
        assert!(authorization.can_read_balance_detail("warehouse-1"));
        assert!(!authorization.read_scope().covers("warehouse-1"));
        assert!(!authorization.can_create("warehouse-1"));
    }

    #[test]
    fn adjustment_detail_scope_does_not_imply_list_scope() {
        let authorization = InventoryAuthorization::from_scopes(
            true,
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
        );

        assert!(authorization.read_scope().covers("warehouse-1"));
        assert_eq!(authorization.adjustment_list_scope().repository_warehouse_ids(None), Some(Vec::new()));
    }

    #[test]
    fn movement_and_reservation_list_scopes_are_independent() {
        let authorization = InventoryAuthorization::from_scopes(
            true,
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            WarehouseScope::from_targets(vec!["warehouse-2".to_string()]),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
            WarehouseScope::empty(),
        );

        assert!(authorization.movement_list_scope().covers("warehouse-1"));
        assert!(!authorization.movement_list_scope().covers("warehouse-2"));
        assert!(authorization.reservation_list_scope().covers("warehouse-2"));
        assert!(!authorization.reservation_list_scope().covers("warehouse-1"));
    }

    #[test]
    fn missing_warehouse_dimension_does_not_contribute_objects() {
        let empty = WarehouseScope::empty();
        assert!(empty.is_empty());
        assert!(!empty.covers("warehouse-1"));
        assert_eq!(empty.repository_warehouse_ids(None), Some(Vec::new()));
        assert_eq!(empty.repository_warehouse_ids(Some(WarehouseId::new("warehouse-1"))), Some(Vec::new()));
    }
}
