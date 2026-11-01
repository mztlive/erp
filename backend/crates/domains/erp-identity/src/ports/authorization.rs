//! 组合根使用的授权事实。不暴露 `RbacService`、`AccountCore` 或 `Role`。

use async_trait::async_trait;

use crate::entity::access_control::OrganizationCoverage;
use crate::entity::rbac::Permission;
use crate::error::Result;

/// Minimal organization-scope fact returned to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationScopeFact {
    coverage: OrganizationCoverage,
}

impl OrganizationScopeFact {
    /// 包装已算出的组织覆盖事实，不暴露角色或账号聚合。
    ///
    /// # 参数
    /// * `coverage` - 已计算的组织覆盖
    ///
    /// # 返回
    /// 返回不暴露 `Role` 或 `AccountCore` 的事实包装。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(coverage: OrganizationCoverage) -> Self {
        Self { coverage }
    }

    /// 借用组织覆盖事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回内部 `OrganizationCoverage` 的借用。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn coverage(&self) -> &OrganizationCoverage {
        &self.coverage
    }
}

/// Authorization facts for composition-root adapters.
#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    /// 判断 `subject` 是否允许 `permission`。
    ///
    /// # 参数
    /// * `subject` - Casbin 主体键
    /// * `permission` - 所需权限
    ///
    /// # 返回
    /// 主体被允许时返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 策略加载或鉴权执行失败时返回错误。
    async fn allows(&self, subject: &str, permission: &Permission) -> Result<bool>;

    /// 返回主体的组织覆盖事实，而不是角色聚合。
    ///
    /// # 参数
    /// * `subject` - Casbin 主体键
    ///
    /// # 返回
    /// 已配置组织覆盖时返回该事实；未配置时返回 `None`。
    ///
    /// # 错误
    /// 策略或数据范围查询失败时返回错误。
    async fn organization_scope(&self, subject: &str) -> Result<Option<OrganizationScopeFact>>;
}
