//! 按仓库范围计算库存授权事实的消费端口。

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::WarehouseId;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 本次库存用例需要取证的动作；未请求的动作不加载仓库范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InventoryAction {
    /// 库存余额列表。
    BalanceList,
    /// 库存余额详情。
    BalanceDetail,
    /// 库存流水列表。
    MovementList,
    /// 库存预占列表。
    ReservationList,
    /// 库存调整列表及其详情资格。
    AdjustmentList,
    /// 库存调整详情。
    AdjustmentDetail,
    /// 库存调整创建及其详情资格。
    Create,
    /// 库存调整更新及其详情资格。
    Update,
}
impl InventoryAction {
    /// 返回固定登记的入口权限。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前动作的具体权限码。
    /// # 错误
    /// 不执行 I/O。
    pub fn permission(self) -> &'static str {
        match self {
            Self::BalanceList => "stock_balance:list",
            Self::BalanceDetail => "stock_balance:detail",
            Self::MovementList => "stock_movement:list",
            Self::ReservationList => "stock_reservation:list",
            Self::AdjustmentList => "stock_adjustment:list",
            Self::AdjustmentDetail => "stock_adjustment:detail",
            Self::Create => "stock_adjustment:create",
            Self::Update => "stock_adjustment:update",
        }
    }

    /// 声明该动作是否同时要求同角色具备调整详情资格。
    /// # 参数
    /// 无。
    /// # 返回
    /// 列表、创建和更新调整单时为 true。
    /// # 错误
    /// 不执行 I/O。
    pub fn requires_adjustment_detail(self) -> bool {
        matches!(self, Self::AdjustmentList | Self::Create | Self::Update)
    }
}

/// 库存用来从身份事实计算仓库范围授权的端口。
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// 在调用方执行器快照上返回 `actor` 的库存仓库范围。
    ///
    /// # 参数
    /// * `actor` - 已认证的审计操作者。
    /// * `actions` - 当前用例实际使用的动作。
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
        actions: &[InventoryAction],
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
        _actions: &[InventoryAction],
        _executor: &mut dyn Executor,
    ) -> Result<InventoryAuthorization> {
        Err(Error::Internal("库存授权端口未接线".to_string()))
    }
}

/// identity 查询返回的完整仓库身份结果；不保存或解释角色条款。
#[derive(Clone, Default)]
pub struct WarehouseScope(Vec<String>, Option<Arc<dyn InventoryReadProof>>);

/// 由统一权限入口生成的原阶段复验能力。
#[async_trait]
pub trait InventoryReadProof: Send + Sync {
    /// 在原执行器复验读取凭证。
    /// # 参数
    /// executor 为业务原事务。
    /// # 返回
    /// 当前授权仍有效时成功。
    /// # 错误
    /// 权限、版本或有效期变化返回错误。
    async fn validate(&self, executor: &mut dyn Executor) -> Result<()>;
}
impl fmt::Debug for WarehouseScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
impl PartialEq for WarehouseScope {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for WarehouseScope {}

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
    /// 绑定统一授权查询产生的精确身份及原凭证。
    /// # 参数
    /// ids 为查询结果，proof 为本次读取的复验能力。
    /// # 返回
    /// 只在当前事务内使用的读取快照。
    /// # 错误
    /// 不执行 I/O。
    pub fn from_authorized(ids: Vec<String>, proof: Arc<dyn InventoryReadProof>) -> Self {
        Self(ids.into_iter().collect::<BTreeSet<_>>().into_iter().collect(), Some(proof))
    }
    /// 在实际业务仓储读取前后复验原授权。
    /// # 参数
    /// executor 为本次读取事务。
    /// # 返回
    /// 凭证仍有效时成功；已拒绝的空结果保持空。
    /// # 错误
    /// 非空结果缺凭证、授权变化或身份设施失败返回错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        match &self.1 {
            Some(proof) => proof.validate(executor).await,
            None if self.0.is_empty() => Ok(()),
            None => Err(Error::Forbidden("仓库结果缺少统一授权读取凭证".into())),
        }
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
        self.0.binary_search_by(|id| id.as_str().cmp(warehouse_id)).is_ok()
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
        self.0.is_empty()
    }

    /// 把调用方精确筛选与授权范围求交，形成 Repository 查询过滤。
    ///
    /// 返回值始终为精确身份集合；空集合不得退化为全量查询。
    ///
    /// # 参数
    /// * `requested` - 调用方指定的仓库；未指定时为 `None`。
    ///
    /// # 返回
    /// 未指定仓库时返回本次 identity 查询结果；指定仓库时仅收窄结果。
    /// 指定仓库不在查询结果内时返回空列表。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn repository_warehouse_ids(&self, requested: Option<WarehouseId>) -> Option<Vec<WarehouseId>> {
        Some(match requested {
            Some(id) if self.covers(id.as_ref()) => vec![id],
            Some(_) => Vec::new(),
            None => self.0.iter().cloned().map(WarehouseId::new).collect(),
        })
    }

    /// 不覆盖任何仓库的空范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回空仓库身份集合。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn empty() -> Self {
        Self(Vec::new(), None)
    }
}

#[cfg(test)]
impl WarehouseScope {
    /// 由仓库标识目标构造范围；空输入失败关闭。
    ///
    /// 输入必须是 identity 查询返回的真实仓库 ID；空集合保持空。
    ///
    /// # 参数
    /// * `targets` - 真实仓库标识。
    ///
    /// # 返回
    /// 返回去重后的仓库范围。
    ///
    /// # 错误
    /// 不返回错误。
    fn from_targets(targets: Vec<String>) -> Self {
        Self(targets.into_iter().collect::<BTreeSet<_>>().into_iter().collect(), None)
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// 在原业务事务内复验本阶段全部读取凭证。
    /// # 参数
    /// executor 为授权和实际仓储读取共用的事务。
    /// # 返回
    /// 各已授权结果仍有效时成功。
    /// # 错误
    /// 身份、策略、映射版本或有效期变化拒绝。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        for scope in [
            &self.balance_list_scope,
            &self.balance_detail_scope,
            &self.movement_list_scope,
            &self.reservation_list_scope,
            &self.adjustment_list_scope,
            &self.read_scope,
            &self.create_scope,
            &self.update_scope,
        ] {
            scope.validate(executor).await?;
        }
        Ok(())
    }
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
    fn authorized_warehouse_results_only_narrow_repository_filters() {
        let single = WarehouseScope::from_targets(vec!["warehouse-1".to_string()]);
        assert_eq!(single.repository_warehouse_ids(None), Some(vec![WarehouseId::new("warehouse-1")]));
        assert_eq!(single.repository_warehouse_ids(Some(WarehouseId::new("warehouse-2"))), Some(Vec::new()));
    }

    #[test]
    fn balance_read_scope_is_independent_from_adjustment_read_and_create_scopes() {
        let authorization = InventoryAuthorization::from_scope_set(super::InventoryScopeSet {
            actor_active: true,
            balance_list_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            balance_detail_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            movement_list_scope: WarehouseScope::empty(),
            reservation_list_scope: WarehouseScope::empty(),
            adjustment_list_scope: WarehouseScope::empty(),
            read_scope: WarehouseScope::empty(),
            create_scope: WarehouseScope::empty(),
            update_scope: WarehouseScope::empty(),
        });
        assert!(authorization.can_read_balance_detail("warehouse-1"));
        assert!(!authorization.read_scope().covers("warehouse-1"));
        assert!(!authorization.can_create("warehouse-1"));
    }

    #[test]
    fn adjustment_detail_scope_does_not_imply_list_scope() {
        let authorization = InventoryAuthorization::from_scope_set(super::InventoryScopeSet {
            actor_active: true,
            balance_list_scope: WarehouseScope::empty(),
            balance_detail_scope: WarehouseScope::empty(),
            movement_list_scope: WarehouseScope::empty(),
            reservation_list_scope: WarehouseScope::empty(),
            adjustment_list_scope: WarehouseScope::empty(),
            read_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            create_scope: WarehouseScope::empty(),
            update_scope: WarehouseScope::empty(),
        });

        assert!(authorization.read_scope().covers("warehouse-1"));
        assert_eq!(authorization.adjustment_list_scope().repository_warehouse_ids(None), Some(Vec::new()));
    }

    #[test]
    fn movement_and_reservation_list_scopes_are_independent() {
        let authorization = InventoryAuthorization::from_scope_set(super::InventoryScopeSet {
            actor_active: true,
            balance_list_scope: WarehouseScope::empty(),
            balance_detail_scope: WarehouseScope::empty(),
            movement_list_scope: WarehouseScope::from_targets(vec!["warehouse-1".to_string()]),
            reservation_list_scope: WarehouseScope::from_targets(vec!["warehouse-2".to_string()]),
            adjustment_list_scope: WarehouseScope::empty(),
            read_scope: WarehouseScope::empty(),
            create_scope: WarehouseScope::empty(),
            update_scope: WarehouseScope::empty(),
        });

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

#[cfg(test)]
mod exact_result_tests {
    use super::WarehouseScope;
    #[test]
    fn warehouse_result_never_interprets_wildcard_as_company_authorization() {
        let result = WarehouseScope::from_targets(vec!["*".into()]);
        assert!(!result.covers("another-warehouse"));
    }
    struct CountingProof(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    #[async_trait::async_trait]
    impl super::InventoryReadProof for CountingProof {
        async fn validate(&self, _: &mut dyn persistence_core::Executor) -> crate::Result<()> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn partial_snapshot_keeps_unrequested_actions_empty_and_revalidates_loaded_scope() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let reads = Arc::new(AtomicUsize::new(0));
        let snapshot = super::InventoryAuthorization::from_scope_set(super::InventoryScopeSet {
            actor_active: true,
            create_scope: WarehouseScope::from_authorized(
                vec!["wh".into()],
                Arc::new(CountingProof(reads.clone())),
            ),
            ..Default::default()
        });
        assert!(snapshot.can_create("wh"));
        assert!(!snapshot.can_update("wh"));
        assert!(!snapshot.can_read_balance_detail("wh"));
        assert!(snapshot.balance_list_scope().is_empty());
        assert!(snapshot.read_scope().is_empty());
        snapshot.validate(&mut persistence_core::NoTransaction).await.unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    struct Proof(bool);
    #[async_trait::async_trait]
    impl super::InventoryReadProof for Proof {
        async fn validate(&self, _: &mut dyn persistence_core::Executor) -> crate::Result<()> {
            if self.0 { Ok(()) } else { Err(crate::Error::ConflictError("DATA_SCOPE_CHANGED".into())) }
        }
    }
    #[tokio::test]
    async fn authorized_warehouse_results_revalidate_original_proof_and_reject_fabrication() {
        let mut executor = persistence_core::NoTransaction;
        let valid = WarehouseScope::from_authorized(vec!["wh".into()], std::sync::Arc::new(Proof(true)));
        assert!(valid.validate(&mut executor).await.is_ok());
        let expired = WarehouseScope::from_authorized(vec!["wh".into()], std::sync::Arc::new(Proof(false)));
        assert!(matches!(expired.validate(&mut executor).await, Err(crate::Error::ConflictError(_))));
        let unproved = WarehouseScope::from_targets(vec!["wh".into()]);
        assert!(matches!(unproved.validate(&mut executor).await, Err(crate::Error::Forbidden(_))));
    }
}
