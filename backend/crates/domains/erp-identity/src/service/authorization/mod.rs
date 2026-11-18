//! 业务授权唯一入口；固定规则及启动绑定不能由调用方替换。
mod approval_cancel;
mod approval_decision;
mod approval_query;
mod approvals;
mod builder;
mod definitions;
mod facilities;
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod fixed;
mod materials;
mod operations;
mod organization;
mod policy_bundles;
pub(crate) use organization::OrganizationScope;
mod require;
mod resolve;
pub(crate) mod rules;
mod targets;
#[cfg(test)]
mod tests;
mod transfers;
#[cfg(feature = "test-support")]
mod unavailable;
mod validate;
mod work_item_query;
pub(crate) mod work_items;

use std::collections::BTreeMap;
use std::slice;
use std::sync::Arc;

use application_core::AuditActor;
pub use builder::AuthorizationBuilder;
use facilities::Facilities;
use persistence_core::Executor;

use crate::repository::authorization::compile;
use crate::{
    AccessFactsPort, AccessMapping, CompiledAccess, Error, Permission, QualifiedPermissions, Result,
};

/// 共享授权服务。只保存身份设施及校验后的不可变实现绑定。
#[derive(Clone)]
pub struct AuthorizationService {
    facilities: Arc<dyn Facilities>,
    facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
    mappings: BTreeMap<Permission, Arc<dyn AccessMapping>>,
    token: Arc<()>,
}

impl AuthorizationService {
    /// 为按钮展示或可选字段检查单项功能权限。有效账号有启用角色授予该权限时返回 `true`；非法权限和取证故障不降级为 `false`。
    ///
    /// 业务读取或写入仍须通过 `query` 或 `require`，不能复用此结果作为对象凭证。
    pub async fn has_permission(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self
            .permissions(actor, slice::from_ref(permission), executor)
            .await?
            .is_some_and(|qualified| qualified.same_role(slice::from_ref(permission))))
    }

    /// 批量检查当前账号的功能资格，不读取对象事实或数据范围。失效账号返回 `None`，非法权限及取证错误原样传播。
    ///
    /// 若多项权限必须来自同一启用角色，使用 `QualifiedPermissions::same_role`；各项独立命中并不能证明同角色资格。
    pub async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.facilities.permissions(actor, permissions, executor).await
    }

    /// 在列表、总数、汇总或查询式导出之前，按固定权限和已登记 Mapping 编译完整数据库授权条件。
    ///
    /// 仓储在分页或统计之前执行条件；用例须沿原执行器调用 `validate` 后才能交付结果。
    /// 未知或不支持的权限、缺少绑定、资格不足以及依赖或执行边界错误均拒绝。
    pub async fn query(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        executor.next_read();
        if !rules::query_supported(permission)? {
            return Err(Error::ValidationError("此权限不支持查询".into()));
        }
        let mapping = self
            .mappings
            .get(permission)
            .ok_or_else(|| Error::ValidationError("缺少查询 Mapping 绑定".into()))?;

        let proof = self
            .resolve_target(
                actor,
                permission,
                Some(mapping.model()),
                None,
                rules::task_query(permission),
                executor,
            )
            .await?;

        compile(&proof, mapping, mapping.model(), executor).await
    }
}
