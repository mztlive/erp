//! 集成对象读取与写入范围；当前处理人及其内部组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::error::{Error, Result};
use crate::ports::{
    IntegrationDataScopePort, IntegrationResolvedClause, IntegrationResolvedScope, IntegrationScopeObject,
};
use crate::repository::{IntegrationReadScope, IntegrationScopeClause};

/// 集成对象访问范围；列表、详情和写命令复用同一解析。
#[derive(Clone)]
pub struct IntegrationAccess {
    scope: Arc<dyn IntegrationDataScopePort>,
}

impl IntegrationAccess {
    /// 绑定范围授权 Port。
    ///
    /// # 参数
    /// * `scope` - 组合层注入的集成范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围。
    pub fn new(scope: Arc<dyn IntegrationDataScopePort>) -> Self {
        Self { scope }
    }

    /// 在调用方事务内证明资源动作并映射处理人责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `resource` - 错误任务或对账差异
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和仓储条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(IntegrationResolvedScope, IntegrationReadScope)> {
        let access = self.scope.resolve(actor, resource, action, executor).await?;
        Ok((access.clone(), integration_scope(&access, actor.id())))
    }

    /// 经生产 adapter 复用公共单对象判定。
    ///
    /// # 参数
    /// * `access` - 当前动作已解析事实
    /// * `object` - 本域当前处理人事实
    ///
    /// # 返回
    /// 范围允许时为 true。
    ///
    /// # 错误
    /// 资源动作不符或未装配时拒绝。
    pub fn allows(&self, access: &IntegrationResolvedScope, object: &IntegrationScopeObject) -> Result<bool> {
        self.scope.allows(access, object)
    }

    /// 查询处理人在解析时点的唯一主属组织。
    ///
    /// # 参数
    /// * `user_id` - 拟写入的处理人
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 存在唯一主属组织时返回其 ID。
    ///
    /// # 错误
    /// 未装配、组织关系非法或缺少有效内部组织时拒绝。
    pub async fn require_handler_org(
        &self,
        user_id: &str,
        at: erp_core::common::time::Instant,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        if user_id.trim().eq_ignore_ascii_case("me") {
            return Err(Error::ValidationError("处理人不得使用 me 作为人员 ID".into()));
        }
        self.scope
            .own_org(user_id, at, executor)
            .await?
            .filter(|org| !org.eq_ignore_ascii_case("company"))
            .ok_or_else(|| Error::ValidationError("处理人缺少有效内部组织".into()))
    }

    /// 展开请求组织及其可选下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织或未装配时拒绝。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await
    }

    /// 构造对象事实并判定当前动作是否覆盖。
    ///
    /// # 参数
    /// * `access` - 当前动作已解析事实
    /// * `owner_user_id` - 当前处理人
    /// * `owner_org_unit_id` - 处理人内部组织
    /// * `historical` - 是否仅历史参与；写动作必须为 false
    ///
    /// # 返回
    /// 范围允许时为 true。
    ///
    /// # 错误
    /// 未装配或资源不符时拒绝。
    pub fn allows_handler(
        &self,
        access: &IntegrationResolvedScope,
        owner_user_id: &str,
        owner_org_unit_id: &str,
        historical: bool,
    ) -> Result<bool> {
        self.scope.allows(
            access,
            &IntegrationScopeObject {
                owned: owner_user_id == access.user_id,
                historical_read_participant: historical,
                org_unit_id: Some(owner_org_unit_id.to_string()),
            },
        )
    }
}

/// 映射同角色正向范围和独立个人上限，保持交集关系。
///
/// # 参数
/// * `access` - Port 返回的已解析授权
/// * `user` - 当前账号
///
/// # 返回
/// 返回集成仓储条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 本人负责解释当前处理人；组织解释处理人内部组织。
pub fn integration_scope(access: &IntegrationResolvedScope, user: &str) -> IntegrationReadScope {
    IntegrationReadScope {
        roles: access.role_clauses.iter().map(|clause| map_clause(clause, user)).collect(),
        user_limit: access.user_limit.as_ref().map(|clause| map_clause(clause, user)),
    }
}

/// 仅接受已解析的内部组织范围，不读取创建人作为授权。
fn map_clause(clause: &IntegrationResolvedClause, actor: &str) -> IntegrationScopeClause {
    IntegrationScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| actor.into()),
        owner_org_unit_ids: clause.org_unit_ids.clone(),
    }
}

/// 供测试与组合根绑定数据库的访问器。
pub fn integration_access(_db: Database, scope: Arc<dyn IntegrationDataScopePort>) -> IntegrationAccess {
    IntegrationAccess::new(scope)
}
