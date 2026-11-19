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

/// 统一执行 ERP 业务授权；只持有身份设施和已校验的固定绑定，不保存请求状态。
#[derive(Clone)]
pub struct AuthorizationService {
    // 读取本次账号资格、人员范围及记录授权的身份设施。
    facilities: Arc<dyn Facilities>,
    // 按资源绑定真实对象及其来源的事实端口。
    facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
    // 按查询权限绑定已冻结的根模型及来源映射。
    mappings: BTreeMap<Permission, Arc<dyn AccessMapping>>,
    // 标识签发服务；克隆共享标识，独立装配的服务不能互验凭证。
    token: Arc<()>,
}

impl AuthorizationService {
    /// 为按钮或可选字段检查单项功能资格：有效账号须有启用角色授予该权限。非法权限和取证故障不转为 `false`。
    ///
    /// 此结果不证明对象资格，业务读取或写入仍须调用 `query` 或 `require`。
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

    /// 一次读取当前账号的多项功能资格，不读取对象或范围。账号失效返回 `None`，非法权限和取证错误原样传播。
    ///
    /// 需要多个权限出自同一启用角色时，使用 `QualifiedPermissions::same_role`，不能把独立命中视为同角色资格。
    pub async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.facilities.permissions(actor, permissions, executor).await
    }

    /// 列表、计数、汇总或查询式导出前，按固定权限及已绑定的 Mapping 编译完整数据库授权过滤。
    ///
    /// 仓储须先执行授权过滤再分页或统计；用例沿原执行器调用 `validate` 后才能交付。未知或不支持的权限、绑定缺失、资格不足及依赖或阶段错误均拒绝。
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

        let (proof, qualified) = self
            .resolve_target(
                actor,
                permission,
                Some(mapping.model()),
                None,
                rules::task_query(permission),
                executor,
            )
            .await?;

        compile(proof, qualified, mapping, executor).await
    }
}

impl CompiledAccess {
    /// 按已授权任务的真实种类和来源，用当前批次的启用角色检查同角色完整执行资格，仅供展示。
    ///
    /// 拒绝非工作项展示查询、未知种类、跨阶段及到期凭证。须使用原读取执行器，交付前仍须复验；通过不代表可以办理任务。
    pub fn execution_qualified(&self, kind: &str, source: &str, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_work_item_view(executor)?;
        work_items::execution_qualified(kind, source, &self.qualified)
    }

    /// 检查当前工作项展示批次是否有启用角色同时具备抄送历史与提交决定权限，不签发办理凭证。
    ///
    /// 须使用原读取执行器，拒绝非展示查询、跨阶段及到期凭证，交付前仍须复验。
    pub fn decision_qualified(&self, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_work_item_view(executor)?;
        Ok(work_items::decision_qualified(&self.qualified))
    }

    // 投影操作按钮前，确认凭证来自固定工作项展示入口，且原读取阶段和期限仍有效。
    fn ensure_work_item_view(&self, executor: &mut dyn Executor) -> Result<()> {
        if !work_items::view_permission(self.permission()) {
            return Err(Error::ValidationError("此查询不提供工作项操作展示资格".into()));
        }
        self.ensure_current(executor)
    }

    /// 供复合资金投影检查是否有整账计划及同角色功能资格。返回整账前仍须执行完整来源过滤并复验原查询凭证。
    pub fn has_whole_qualification(&self) -> bool {
        self.has_whole_plan() && rules::sources::whole_qualified(self.permission(), &self.qualified)
    }
}
