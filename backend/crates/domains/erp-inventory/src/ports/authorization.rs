//! 按仓库范围计算库存授权事实的消费端口。

use std::collections::BTreeSet;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::WarehouseId;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 库存用来从身份事实计算仓库范围授权的端口。
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// 在调用方执行器快照上返回 `actor` 的库存仓库范围。
    ///
    /// # 参数
    /// * `actor` - 已认证的审计操作者。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 返回该操作者的库存授权快照。
    ///
    /// # 错误
    /// 身份、策略或数据范围查询失败时返回对应错误。
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
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 范围版本、时点为空字符串，策略版本与组织版本为 0。
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 范围版本字符串。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn scope_version(&self) -> &str {
        &self.scope_version
    }

    /// 权限策略版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 权限策略版本。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn policy_version(&self) -> u64 {
        self.policy_version
    }

    /// 组织关系版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 组织关系版本。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn organization_version(&self) -> u64 {
        self.organization_version
    }

    /// 授权时点。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 授权时点字符串。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_of(&self) -> &str {
        &self.as_of
    }
}

impl WarehouseScope {
    /// 判断目标仓库是否落在已证明范围内。
    ///
    /// # 参数
    /// * `warehouse_id` - 待判断的仓库标识。
    ///
    /// # 返回
    /// 范围覆盖该仓库时为 `true`；范围为空或不包含该仓库时为 `false`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn covers(&self, warehouse_id: &str) -> bool {
        self.0.as_ref().is_some_and(|coverage| coverage.covers(warehouse_id))
    }

    /// 缺仓库维或空目标时不贡献对象。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 内部覆盖为空时为 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// 把调用方精确筛选与授权范围求交，形成 Repository 查询过滤。
    ///
    /// `None` 仅表示公司级且调用方未指定仓库；`Some([])` 必须由 Repository
    /// 解释为空结果，禁止退化为全量查询。
    ///
    /// # 参数
    /// * `requested` - 调用方指定的仓库；未指定时为 `None`。
    ///
    /// # 返回
    /// 公司级且未指定仓库时返回 `None`。有限目标且未指定仓库时返回允许的仓库。
    /// 指定仓库落在范围内时返回该仓库。范围为空，或指定仓库不在有限目标内时，返回空列表。
    ///
    /// # 错误
    /// 不返回错误。
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

    /// 不覆盖任何仓库的空范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回内部覆盖为 `None` 的范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn empty() -> Self {
        Self(None)
    }

    /// 公司级范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回覆盖全部仓库的范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn company() -> Self {
        Self(Some(WarehouseCoverage::All))
    }

    /// 由仓库标识目标构造范围；空输入失败关闭。
    ///
    /// 目标含 `*` 时为公司级。空集合时不覆盖任何仓库。
    ///
    /// # 参数
    /// * `targets` - 仓库标识；`*` 表示全部仓库。
    ///
    /// # 返回
    /// 返回去重后的仓库范围。
    ///
    /// # 错误
    /// 不返回错误。
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

/// 库存各动作的仓库范围集合（具名字段，避免同类型位置参数传错顺序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryScopeSet {
    /// 认证身份是否仍对应可登录账号。
    pub actor_active: bool,
    /// 余额列表范围。
    pub balance_list_scope: WarehouseScope,
    /// 余额详情范围。
    pub balance_detail_scope: WarehouseScope,
    /// 流水列表范围。
    pub movement_list_scope: WarehouseScope,
    /// 预占列表范围。
    pub reservation_list_scope: WarehouseScope,
    /// 调整列表范围。
    pub adjustment_list_scope: WarehouseScope,
    /// 对象读取范围。
    pub read_scope: WarehouseScope,
    /// 调整创建范围。
    pub create_scope: WarehouseScope,
    /// 调整更新范围。
    pub update_scope: WarehouseScope,
}

impl InventoryAuthorization {
    /// 未激活身份，且各仓库范围均为空。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `actor_active` 为假、各仓库范围为空、列表元信息为空的授权快照。
    ///
    /// # 错误
    /// 不返回错误。
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

    /// 由范围集合构造授权快照（组合层装配入口）。
    ///
    /// # 参数
    /// * `scopes` - 各动作的仓库范围集合
    ///
    /// # 返回
    /// 返回带空列表元信息的授权快照。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_scope_set(scopes: InventoryScopeSet) -> Self {
        Self {
            actor_active: scopes.actor_active,
            balance_list_scope: scopes.balance_list_scope,
            balance_detail_scope: scopes.balance_detail_scope,
            movement_list_scope: scopes.movement_list_scope,
            reservation_list_scope: scopes.reservation_list_scope,
            adjustment_list_scope: scopes.adjustment_list_scope,
            read_scope: scopes.read_scope,
            create_scope: scopes.create_scope,
            update_scope: scopes.update_scope,
            balance_list_meta: InventoryScopeMeta::empty(),
            movement_list_meta: InventoryScopeMeta::empty(),
            adjustment_list_meta: InventoryScopeMeta::empty(),
        }
    }

    /// 由组合层计算出的各动作仓库范围构造授权快照。
    ///
    /// 历史位置参数入口，委托 [`Self::from_scope_set`]；外部组合层未迁移前保留。
    ///
    /// # 参数
    /// * `actor_active` - 认证身份是否仍对应可登录账号。
    /// * `balance_list_scope` - 余额列表范围。
    /// * `balance_detail_scope` - 余额详情范围。
    /// * `movement_list_scope` - 流水列表范围。
    /// * `reservation_list_scope` - 预占列表范围。
    /// * `adjustment_list_scope` - 调整列表范围。
    /// * `read_scope` - 对象读取范围。
    /// * `create_scope` - 调整创建范围。
    /// * `update_scope` - 调整更新范围。
    ///
    /// # 返回
    /// 返回带空列表元信息的授权快照。
    ///
    /// # 错误
    /// 不返回错误。
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
        Self::from_scope_set(InventoryScopeSet {
            actor_active,
            balance_list_scope,
            balance_detail_scope,
            movement_list_scope,
            reservation_list_scope,
            adjustment_list_scope,
            read_scope,
            create_scope,
            update_scope,
        })
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
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 身份仍可登录时为 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn actor_is_active(&self) -> bool {
        self.actor_active
    }

    /// 返回库存余额列表的仓库范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 余额列表仓库范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn balance_list_scope(&self) -> &WarehouseScope {
        &self.balance_list_scope
    }

    /// 判断当前账号是否可读取目标仓库的库存余额详情。
    ///
    /// # 参数
    /// * `warehouse_id` - 目标仓库标识。
    ///
    /// # 返回
    /// 余额详情范围覆盖该仓库时为 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn can_read_balance_detail(&self, warehouse_id: &str) -> bool {
        self.balance_detail_scope.covers(warehouse_id)
    }

    /// 返回库存流水列表的仓库范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 流水列表仓库范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn movement_list_scope(&self) -> &WarehouseScope {
        &self.movement_list_scope
    }

    /// 返回库存预占列表的仓库范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 预占列表仓库范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn reservation_list_scope(&self) -> &WarehouseScope {
        &self.reservation_list_scope
    }

    /// 返回库存调整列表的联合 `list + detail` 仓库范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 调整列表仓库范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn adjustment_list_scope(&self) -> &WarehouseScope {
        &self.adjustment_list_scope
    }

    /// 返回对象读取仓库范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 对象读取仓库范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn read_scope(&self) -> &WarehouseScope {
        &self.read_scope
    }

    /// 判断当前账号是否可在目标仓库创建库存调整。
    ///
    /// # 参数
    /// * `warehouse_id` - 目标仓库标识。
    ///
    /// # 返回
    /// 创建范围覆盖该仓库时为 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn can_create(&self, warehouse_id: &str) -> bool {
        self.create_scope.covers(warehouse_id)
    }

    /// 判断当前账号是否可更新目标仓库的库存调整。
    ///
    /// # 参数
    /// * `warehouse_id` - 目标仓库标识。
    ///
    /// # 返回
    /// 更新范围覆盖该仓库时为 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn can_update(&self, warehouse_id: &str) -> bool {
        self.update_scope.covers(warehouse_id)
    }

    /// 余额列表授权指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 余额列表的授权元信息。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn balance_list_meta(&self) -> &InventoryScopeMeta {
        &self.balance_list_meta
    }

    /// 流水列表授权指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 流水列表的授权元信息。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn movement_list_meta(&self) -> &InventoryScopeMeta {
        &self.movement_list_meta
    }

    /// 调整列表授权指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 调整列表的授权元信息。
    ///
    /// # 错误
    /// 不返回错误。
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
