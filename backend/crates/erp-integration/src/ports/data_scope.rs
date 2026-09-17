//! 集成范围授权 Port：只接收已解析事实，不暴露身份域实体或 Service。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 身份域已解析的集成正向范围条款；不含身份域实体句柄。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntegrationResolvedClause {
    /// 公司范围覆盖该资源动作的全部对象。
    pub company: bool,
    /// 当前处理人为操作人的对象。
    pub self_owned: bool,
    /// 已由公共解析器展开的内部组织。
    pub org_unit_ids: Vec<String>,
}

impl IntegrationResolvedClause {
    /// 判断条款是否构成有效集成范围规则（与仓储侧 `IntegrationScopeClause::is_empty` 互为否定）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 含公司、本人处理或组织目标时为 true（即仓储侧 `is_empty() == false`）。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 空条款保持空集且不得补公司。
    pub fn has_scope_rules(&self) -> bool {
        self.company || self.self_owned || !self.org_unit_ids.is_empty()
    }

    /// 判断条款是否为空（与 [`has_scope_rules`](Self::has_scope_rules) 互为否定）。
    ///
    /// # 返回
    /// 无公司、无本人处理且无组织目标时为 true。
    pub fn is_empty(&self) -> bool {
        !self.has_scope_rules()
    }
}

/// 已通过资格检查的集成范围事实。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegrationResolvedScope {
    /// 已认证操作人。
    pub user_id: String,
    /// 本次解析的集成资源。
    pub resource: String,
    /// 本次解析的动作。
    pub action: String,
    /// 同角色正向范围。
    pub role_clauses: Vec<IntegrationResolvedClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<IntegrationResolvedClause>,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// 授权解析时点。
    pub as_of: Instant,
}

impl IntegrationResolvedScope {
    /// 判断角色正向范围是否含有效规则。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 任一角色条款构成有效规则时为 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只看角色条款；个人上限不单独构成“有范围规则”。
    pub fn has_scope_rules(&self) -> bool {
        self.role_clauses.iter().any(IntegrationResolvedClause::has_scope_rules)
    }
}

/// 业务域提供的单对象责任事实；不包含原始授权规则。
#[derive(Debug, Clone, Default)]
pub struct IntegrationScopeObject {
    /// 当前操作人是否为对象的当前处理人。
    pub owned: bool,
    /// 当前操作人是否具备合法历史读取参与。
    pub historical_read_participant: bool,
    /// 当前处理人所属内部组织。
    pub org_unit_id: Option<String>,
}

/// 集成域消费的窄授权 Port；adapter 调用身份域公共解析器。
#[async_trait]
pub trait IntegrationDataScopePort: Send + Sync {
    /// 经生产 adapter 复用公共单对象范围判定。
    ///
    /// # 参数
    /// * `scope` - 当前动作已解析事实
    /// * `object` - 本域提供的当前处理人与组织事实
    ///
    /// # 返回
    /// 返回角色范围和个人上限共同允许的判定。
    ///
    /// # 错误
    /// 未装配、资源动作不符时失败；禁止以数据库创建判定作为兜底。
    fn allows(&self, _scope: &IntegrationResolvedScope, _object: &IntegrationScopeObject) -> Result<bool> {
        Err(unwired())
    }

    /// 在调用方事务内证明集成资源动作并返回已解析事实。
    ///
    /// # 参数
    /// * `actor` - 服务端已认证身份
    /// * `resource` - 错误任务或对账差异资源
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已通过资格检查的范围事实；全量、空集、缺少个人上限可区分。
    ///
    /// # 错误
    /// 未装配、未注册、无动作权限或版本变化必须明确失败。
    ///
    /// # 关键业务约束
    /// 不得把原始 `scope_type / scope_targets` 交给集成域再解析。
    async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<IntegrationResolvedScope>;

    /// 展开请求中的组织及其可选下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织拒绝，不得忽略后查询全部组织。
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>>;

    /// 读取指定内部组织在给定时点的有效主属成员。
    ///
    /// # 参数
    /// * `org_unit_ids` - 已展开的内部组织
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回排序去重后的人员 ID。
    ///
    /// # 错误
    /// 组织关系非法或读取失败时拒绝。
    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 查询账号在解析时点的唯一主属组织。
    ///
    /// # 参数
    /// * `user_id` - 当前账号
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 存在唯一主属组织时返回其 ID；没有主属组织时返回 `None`。
    ///
    /// # 错误
    /// 同一时点存在多条主属关系时拒绝。
    ///
    /// # 关键业务约束
    /// 不得默认放入根组织或公司范围。
    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>>;
}

/// 未接线时失败关闭，不得补公司范围。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedIntegrationDataScopePort;

impl FailClosedIntegrationDataScopePort {
    /// 返回未接线的共享端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可注入集成 Service 的失败关闭端口。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 仅用于不解析范围的装载路径；解析入口必须注入真实 adapter。
    pub fn shared() -> Arc<dyn IntegrationDataScopePort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl IntegrationDataScopePort for FailClosedIntegrationDataScopePort {
    async fn resolve(
        &self,
        _actor: &AuditActor,
        _resource: &str,
        _action: &str,
        _executor: &mut dyn Executor,
    ) -> Result<IntegrationResolvedScope> {
        Err(unwired())
    }

    async fn expand_org_units(
        &self,
        _org_unit_ids: &[String],
        _include_descendants: bool,
        _executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Err(unwired())
    }

    async fn org_member_ids(
        &self,
        _org_unit_ids: &BTreeSet<String>,
        _at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(unwired())
    }

    async fn own_org(
        &self,
        _user_id: &str,
        _at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Err(unwired())
    }
}

/// 未装配授权 Port 时的失败关闭错误。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回内部错误，不退化为公司范围。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 禁止捕获后回退旧读取器或补 Company。
fn unwired() -> Error {
    Error::Internal("集成范围端口未接线".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_self_owned_is_not_no_scope_when_objects_are_empty() {
        assert!(!IntegrationResolvedClause::default().has_scope_rules());
        assert!(
            IntegrationResolvedClause { self_owned: true, ..IntegrationResolvedClause::default() }
                .has_scope_rules()
        );
        assert!(
            IntegrationResolvedClause { company: true, ..IntegrationResolvedClause::default() }
                .has_scope_rules()
        );
        assert!(
            IntegrationResolvedClause {
                org_unit_ids: vec!["org-a".into()],
                ..IntegrationResolvedClause::default()
            }
            .has_scope_rules()
        );
        assert!(
            !IntegrationResolvedScope {
                user_id: "u1".into(),
                resource: "integration_error_task".into(),
                action: "list".into(),
                role_clauses: vec![],
                user_limit: Some(IntegrationResolvedClause {
                    self_owned: true,
                    ..IntegrationResolvedClause::default()
                }),
                policy_version: 1,
                organization_version: 1,
                scope_version: "v1".into(),
                as_of: Instant::from_unix_secs(0),
            }
            .has_scope_rules()
        );
    }
}
