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

impl CompiledAccess {
    /// 返回本次工作项展示批次的固定完整执行资格。
    /// 同次查询的启用角色完整覆盖该种类的固定资格时为 true；不证明任务可办理。
    ///
    /// 非工作项展示查询、未知种类、跨阶段或到期时拒绝；交付前仍须 validate 原查询。
    ///
    /// kind、source 为已授权任务的真实种类和来源类型；executor 为原读取执行器。
    pub fn execution_qualified(&self, kind: &str, source: &str, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_work_item_view(executor)?;
        work_items::execution_qualified(kind, source, &self.proof.qualified)
    }

    /// 返回本次工作项展示批次的固定审批决定资格。
    /// 同次查询的启用角色同时覆盖抄送历史与提交决定时为 true；不签发办理凭证。
    ///
    /// 非工作项展示查询、跨阶段或到期时拒绝；交付前仍须 validate 原查询。
    ///
    /// executor 为生成当前查询的原读取执行器。
    pub fn decision_qualified(&self, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_work_item_view(executor)?;
        Ok(work_items::decision_qualified(&self.proof.qualified))
    }

    fn ensure_work_item_view(&self, executor: &mut dyn Executor) -> Result<()> {
        if !work_items::view_permission(self.permission()) {
            return Err(Error::ValidationError("此查询不提供工作项操作展示资格".into()));
        }
        self.ensure_current(executor)
    }

    /// 供复合资金投影判断当前启用角色是否具有整账功能资格，不证明来源可读。返回整账前仍须执行完整来源过滤并复验原查询凭证。
    pub fn has_whole_qualification(&self) -> bool {
        self.whole.is_some() && rules::sources::whole_qualified(self.permission(), &self.proof.qualified)
    }
}
