//! 按仓库范围计算库存授权事实的消费端口。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::bson::Document;
use persistence_core::Executor;

use crate::authorization::map_identity_error;
use crate::error::Result;

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

    /// 在原执行器检查实际仓库的动作资格。
    /// # 参数
    /// actor、action、warehouse 为真实账号、固定动作及实际仓库；executor 为原执行器。
    /// # 返回
    /// 功能、同角色附加资格和仓库范围均成立时成功。
    /// # 错误
    /// 无权、仓库缺失或设施读取失败时拒绝。
    async fn require_warehouse(
        &self,
        actor: &AuditActor,
        action: InventoryAction,
        warehouse: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 查询当前展示批次内有动作资格的仓库。
    /// # 参数
    /// actor、action 为真实账号及固定动作，ids 只含当前页仓库，executor 为原读取执行器。
    /// # 返回
    /// 已授权的候选子集；不枚举账号全部授权仓库。
    /// # 错误
    /// 设施或授权版本变化失败；普通无权返回空集合。
    async fn allowed_warehouses(
        &self,
        actor: &AuditActor,
        action: InventoryAction,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>>;
}

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

/// 当前读取阶段的库存查询凭证，直接用于库存集合聚合。
#[derive(Clone)]
pub struct InventoryQuery {
    authorization: AuthorizationService,
    compiled: Arc<CompiledAccess>,
    meta: InventoryScopeMeta,
    no_scope: bool,
}
impl InventoryQuery {
    /// 绑定统一授权服务和库存模型凭证。
    /// # 参数
    /// authorization、compiled 为同一登记实例；no_scope 为真实仓库授权存在性结果。
    /// # 返回
    /// 只在当前读取阶段使用的完整凭证。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(authorization: AuthorizationService, compiled: CompiledAccess, no_scope: bool) -> Self {
        let meta = InventoryScopeMeta::new(
            compiled.version(),
            compiled.policy_version(),
            compiled.organization_version(),
            compiled.as_of().as_utc().to_rfc3339(),
        );
        Self { authorization, compiled: Arc::new(compiled), meta, no_scope }
    }
    /// 读取本凭证绑定的库存集合。
    /// # 参数
    /// 无。
    /// # 返回
    /// 固定模型集合名，供拥有仓储防止凭证错配。
    /// # 错误
    /// 无。
    pub fn collection(&self) -> &str {
        &self.compiled.model().collection
    }
    /// 返回完整查询阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 必须置于分页、计数和汇总之前的授权阶段。
    /// # 错误
    /// 无。
    pub fn stages(&self) -> &[Document] {
        self.compiled.stages()
    }
    /// 在实际读取前后复验原凭证。
    /// # 参数
    /// executor 为原执行器。
    /// # 返回
    /// 原凭证仍有效时成功。
    /// # 错误
    /// 身份、权限、事实或版本失效拒绝。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.authorization.validate(&self.compiled, executor).await.map_err(map_identity_error)
    }
}

/// 当前用例实际请求的查询凭证；不保存八组全量仓库 ID。
#[derive(Clone)]
pub struct InventoryAuthorization {
    actor_active: bool,
    queries: BTreeMap<InventoryAction, InventoryQuery>,
}
impl InventoryAuthorization {
    /// 构造当前账号和实际读取动作的凭证集合。
    /// # 参数
    /// actor_active 为账号取证结果，queries 仅保存当前请求的查询动作。
    /// # 返回
    /// 有界的动作查询集合。
    /// # 错误
    /// 无。
    pub fn new(actor_active: bool, queries: BTreeMap<InventoryAction, InventoryQuery>) -> Self {
        Self { actor_active, queries }
    }
    /// 构造已失效账号的拒绝结果。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不包含任何查询凭证的结果。
    /// # 错误
    /// 无。
    pub fn inactive() -> Self {
        Self::new(false, BTreeMap::new())
    }
    /// 判断账号是否仍有效。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本阶段账号取证结果。
    /// # 错误
    /// 无。
    pub fn actor_is_active(&self) -> bool {
        self.actor_active
    }
    /// 返回本次已请求且获准的查询凭证。
    /// # 参数
    /// action 为实际列表动作。
    /// # 返回
    /// 无功能资格时为 None，仓储必须将其解释为拒绝结果。
    /// # 错误
    /// 无。
    pub fn query(&self, action: InventoryAction) -> Option<&InventoryQuery> {
        self.queries.get(&action)
    }
    /// 判断列表动作是否没有真实授权仓库。
    /// # 参数
    /// action 为实际列表动作。
    /// # 返回
    /// 无功能资格或仓库授权为空时为 true。
    /// # 错误
    /// 无。
    pub fn no_scope(&self, action: InventoryAction) -> bool {
        self.query(action).is_none_or(|query| query.no_scope)
    }
    /// 返回本次列表的真实授权元信息。
    /// # 参数
    /// action 为列表动作。
    /// # 返回
    /// 已授权查询元信息；无资格返回空信封。
    /// # 错误
    /// 无。
    pub fn meta(&self, action: InventoryAction) -> &InventoryScopeMeta {
        static EMPTY: InventoryScopeMeta = InventoryScopeMeta {
            scope_version: String::new(),
            policy_version: 0,
            organization_version: 0,
            as_of: String::new(),
        };
        self.query(action).map_or(&EMPTY, |query| &query.meta)
    }
    /// 复验本用例全部实际查询凭证。
    /// # 参数
    /// executor 为原读取或写入事务。
    /// # 返回
    /// 全部凭证仍有效时成功。
    /// # 错误
    /// 任一授权失效返回原错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        for query in self.queries.values() {
            query.validate(executor).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_actions_never_create_unrestricted_queries() {
        let active = InventoryAuthorization::new(true, BTreeMap::new());
        assert!(active.actor_is_active());
        for action in [
            InventoryAction::BalanceList,
            InventoryAction::MovementList,
            InventoryAction::ReservationList,
            InventoryAction::AdjustmentList,
        ] {
            assert!(active.query(action).is_none());
            assert!(active.no_scope(action));
            assert!(active.meta(action).scope_version().is_empty());
        }
        assert!(!InventoryAuthorization::inactive().actor_is_active());
    }
}
