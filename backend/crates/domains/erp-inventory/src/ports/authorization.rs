//! 按仓库范围计算库存授权事实的消费端口。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::QueryOutput;
use erp_identity::{AuthorizationService, CompiledAccess};
use erp_permissions::{Permission, inventory};
use mongodb::bson::Document;
use persistence_core::Executor;

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
    pub fn permission(self) -> Permission {
        match self {
            Self::BalanceList => inventory::STOCK_BALANCE_LIST,
            Self::BalanceDetail => inventory::STOCK_BALANCE_DETAIL,
            Self::MovementList => inventory::STOCK_MOVEMENT_LIST,
            Self::ReservationList => inventory::STOCK_RESERVATION_LIST,
            Self::AdjustmentList => inventory::STOCK_ADJUSTMENT_LIST,
            Self::AdjustmentDetail => inventory::STOCK_ADJUSTMENT_DETAIL,
            Self::Create => inventory::STOCK_ADJUSTMENT_CREATE,
            Self::Update => inventory::STOCK_ADJUSTMENT_UPDATE,
        }
    }
}

/// 库存用来从身份事实计算仓库范围授权的端口。
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// 在读取库存调整单之前执行统一详情对象授权。
    ///
    /// # 参数
    /// actor、id 为真实账号和调整单；executor 为完整详情读取的原快照事务。
    /// # 返回
    /// 统一服务按 STOCK_ADJUSTMENT_DETAIL 检查完整资格和真实仓库来源后成功。
    /// # 错误
    /// 资格、对象或仓库来源无权时拒绝；取证及执行边界错误原样返回。
    async fn require_adjustment(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在读取库存调整单之前执行统一修改对象授权。
    ///
    /// # 参数
    /// actor、id 为真实账号和既有调整单；executor 为修改命令的原快照事务。
    /// # 返回
    /// 统一服务按 STOCK_ADJUSTMENT_UPDATE 的固定完整资格和真实对象事实检查后成功。
    /// # 错误
    /// 资格、对象或真实仓库来源无权时拒绝；取证及执行边界错误原样返回。
    async fn require_adjustment_update(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在调用方执行器快照上返回 `actor` 的库存仓库范围。
    ///
    /// # 参数
    /// * `actor` - 已认证的审计操作者。
    /// * `action` - 当前读取阶段唯一使用的动作。
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
        action: InventoryAction,
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
    authorization: Arc<AuthorizationService>,
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
    pub fn new(authorization: Arc<AuthorizationService>, compiled: CompiledAccess, no_scope: bool) -> Self {
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
    /// 仓储仅检查当前执行边界与有效期，不重读授权依赖。
    /// # 参数
    /// executor 为本次读取的原执行器。
    /// # 返回
    /// 当前凭证有效时成功。
    /// # 错误
    /// 跨事务、跨阶段或到期时拒绝。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        if self.compiled.model().output != QueryOutput::Object {
            return Err(crate::Error::ValidationError("库存查询必须输出完整对象".into()));
        }
        self.compiled.ensure_current(executor).map_err(Into::into)
    }

    /// 在读取用例交付前复验本次真实授权依赖。
    /// # 参数
    /// executor 为生成凭证时的原执行器。
    /// # 返回
    /// 身份、范围、动态依赖及有效期均有效时成功。
    /// # 错误
    /// 任一依赖失效时拒绝交付。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.ensure_current(executor)?;
        self.authorization.validate(&self.compiled, executor).await.map_err(Into::into)
    }
}

/// 当前读取阶段唯一的查询凭证；不保存其他动作或全量仓库 ID。
#[derive(Clone)]
pub struct InventoryAuthorization {
    actor_active: bool,
    query: Option<(InventoryAction, InventoryQuery)>,
}
impl InventoryAuthorization {
    /// 构造当前账号和单个实际读取动作的凭证。
    /// # 参数
    /// actor_active 为账号取证结果，query 仅保存当前阶段唯一的查询动作及凭证。
    /// # 返回
    /// 单一读取阶段的查询授权。
    /// # 错误
    /// 无。
    pub fn new(actor_active: bool, query: Option<(InventoryAction, InventoryQuery)>) -> Self {
        Self { actor_active, query }
    }
    /// 构造已失效账号的拒绝结果。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不包含任何查询凭证的结果。
    /// # 错误
    /// 无。
    pub fn inactive() -> Self {
        Self::new(false, None)
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
        self.query.as_ref().filter(|(entry, _)| entry == &action).map(|(_, query)| query)
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
    /// 复验当前读取阶段唯一的查询凭证。
    /// # 参数
    /// executor 为原读取或写入事务。
    /// # 返回
    /// 当前凭证仍有效时成功。
    /// # 错误
    /// 授权失效返回原错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        if let Some((_, query)) = &self.query {
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
        let active = InventoryAuthorization::new(true, None);
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
