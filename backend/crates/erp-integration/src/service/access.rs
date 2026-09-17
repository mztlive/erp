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

    /// 在调用方事务内按当前处理人事实重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `resource` - 错误任务或对账差异
    /// * `action` - 已注册动作
    /// * `owner_user_id` - 对象当前处理人
    /// * `owner_org_unit_id` - 处理人内部组织
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时成功。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound，不泄露存在性；未装配时失败关闭。
    ///
    /// # 关键业务约束
    /// 列表授权不替代对象重验；历史处理人不构成详情可见。
    pub async fn require_handler(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        owner_user_id: &str,
        owner_org_unit_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, scope) = self.resolve(actor, resource, action, executor).await?;
        if !scope.allows_object(owner_user_id, owner_org_unit_id) {
            return Err(Error::NotFound(hidden_object(resource)?));
        }
        if !self.allows_handler(&access, owner_user_id, owner_org_unit_id, false)? {
            return Err(Error::NotFound(hidden_object(resource)?));
        }
        Ok(())
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

/// 越权详情与对象不存在同码，避免枚举他域任务或差异。
///
/// 已知资源显式枚举，未知资源走内部错误（禁止默认归入任务文案，
/// 新增资源类型必须在此登记，否则测试失败关闭）。
///
/// # 参数
/// * `resource` - 错误任务（`integration_error_task`）或对账差异（`reconciliation_difference`）
///
/// # 返回
/// 返回与加载层一致的 NotFound 文案载荷。
///
/// # 错误
/// 未知资源类型时返回内部错误。
fn hidden_object(resource: &str) -> Result<String> {
    match resource {
        "integration_error_task" => Ok("任务不存在".to_string()),
        "reconciliation_difference" => Ok("差异不存在".to_string()),
        _ => Err(Error::Internal(format!("未知集成资源类型: {resource}"))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use erp_core::common::time::Instant;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::ports::FailClosedIntegrationDataScopePort;

    struct StubScope {
        resolved: IntegrationResolvedScope,
        allow: bool,
        historical: Mutex<Option<bool>>,
    }

    #[async_trait]
    impl IntegrationDataScopePort for StubScope {
        fn allows(&self, _scope: &IntegrationResolvedScope, object: &IntegrationScopeObject) -> Result<bool> {
            *self.historical.lock().expect("lock") = Some(object.historical_read_participant);
            Ok(self.allow)
        }

        async fn resolve(
            &self,
            _actor: &AuditActor,
            _resource: &str,
            _action: &str,
            _executor: &mut dyn Executor,
        ) -> Result<IntegrationResolvedScope> {
            Ok(self.resolved.clone())
        }

        async fn expand_org_units(
            &self,
            _org_unit_ids: &[String],
            _include_descendants: bool,
            _executor: &mut dyn Executor,
        ) -> Result<BTreeSet<String>> {
            Ok(BTreeSet::new())
        }

        async fn org_member_ids(
            &self,
            _org_unit_ids: &BTreeSet<String>,
            _at: Instant,
            _executor: &mut dyn Executor,
        ) -> Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn own_org(
            &self,
            _user_id: &str,
            _at: Instant,
            _executor: &mut dyn Executor,
        ) -> Result<Option<String>> {
            Ok(None)
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "actor".into(), erp_core::AccountKind::Admin)
    }

    fn resolved(resource: &str, self_owned: bool, org: &[&str], company: bool) -> IntegrationResolvedScope {
        IntegrationResolvedScope {
            user_id: "actor".into(),
            resource: resource.into(),
            action: "detail".into(),
            role_clauses: vec![IntegrationResolvedClause {
                company,
                self_owned,
                org_unit_ids: org.iter().map(|id| (*id).to_string()).collect(),
            }],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(0),
        }
    }

    fn stub_access(resolved: IntegrationResolvedScope, allow: bool) -> IntegrationAccess {
        IntegrationAccess::new(Arc::new(StubScope { resolved, allow, historical: Mutex::new(None) }))
    }

    #[tokio::test]
    async fn fail_closed_require_handler_rejects_unwired() {
        let access = IntegrationAccess::new(FailClosedIntegrationDataScopePort::shared());
        let error = access
            .require_handler(
                &actor(),
                "integration_error_task",
                "detail",
                "actor",
                "org-a",
                &mut NoTransaction,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message.contains("未接线")));
    }

    #[tokio::test]
    async fn require_handler_hides_out_of_scope_as_not_found() {
        let access = stub_access(resolved("integration_error_task", true, &[], false), true);
        let error = access
            .require_handler(
                &actor(),
                "integration_error_task",
                "detail",
                "other",
                "org-b",
                &mut NoTransaction,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::NotFound(message) if message == "任务不存在"));
        let access = stub_access(resolved("reconciliation_difference", true, &[], false), true);
        let error = access
            .require_handler(
                &actor(),
                "reconciliation_difference",
                "detail",
                "other",
                "org-b",
                &mut NoTransaction,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::NotFound(message) if message == "差异不存在"));
    }

    #[tokio::test]
    async fn require_handler_allows_current_handler_and_rejects_public_false() {
        let access = stub_access(resolved("integration_error_task", true, &[], false), true);
        access
            .require_handler(
                &actor(),
                "integration_error_task",
                "detail",
                "actor",
                "org-a",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let access = stub_access(resolved("integration_error_task", false, &[], true), false);
        let error = access
            .require_handler(
                &actor(),
                "integration_error_task",
                "detail",
                "other",
                "org-z",
                &mut NoTransaction,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn require_handler_detail_does_not_grant_historical_participation() {
        let stub = Arc::new(StubScope {
            resolved: resolved("integration_error_task", true, &[], false),
            allow: true,
            historical: Mutex::new(None),
        });
        IntegrationAccess::new(stub.clone())
            .require_handler(
                &actor(),
                "integration_error_task",
                "detail",
                "actor",
                "org-a",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(*stub.historical.lock().expect("lock"), Some(false));
    }
}
