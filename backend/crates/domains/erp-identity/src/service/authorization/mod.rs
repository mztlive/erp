//! 业务授权唯一入口；固定规则及启动绑定不能由调用方替换。
mod approval_cancel;
mod approval_query;
mod approvals;
mod builder;
mod definitions;
mod facilities;
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
    /// 查询单个功能资格；不能作为对象访问或写入凭证。
    /// # 参数
    /// actor 为当前账号，permission 为具体权限，executor 为原执行器。
    /// # 返回
    /// 有效账号的启用角色授予权限时为 true，否则为 false。
    /// # 错误
    /// 非法权限或身份设施取证失败原样传播。
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

    /// 批量读取功能资格，不加载业务事实或数据范围。
    /// # 参数
    /// actor、permissions 为真实账号与具体权限集合；executor 为原执行器。
    /// # 返回
    /// 账号失效返回 None；有效账号返回逐项独立授权结果，允许零权限。
    /// # 错误
    /// 非法权限或身份设施取证失败原样传播。
    pub async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.facilities.permissions(actor, permissions, executor).await
    }

    /// 按固定权限选择查询入口并生成完整数据库授权条件。
    /// # 参数
    /// actor 为认证账号，permission 为查询权限，executor 为同一读取阶段执行器。
    /// # 返回
    /// 仓储可执行的 CompiledAccess；交付业务结果前仍须调用 validate。
    /// # 错误
    /// 未知权限、缺少绑定、完整资格不足、动态依赖或执行边界错误均拒绝。
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
