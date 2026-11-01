//! 资金往来范围授权 Port：只接收已解析事实，不暴露身份域实体或 Service。
//!
//! M07 客户往来、M08 开票申请与记录、M09 供应商往来共用本 Port；各资源按自身
//! 动作分别解析，对象事实由各消费入口按同一映射提供。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 身份域已解析的资金正向范围条款；不含身份域实体句柄。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FundsResolvedClause {
    /// 公司范围覆盖该资源动作的全部资金单据。
    pub company: bool,
    /// 关联销售单或采购单的当前负责人为操作人的单据。
    pub self_owned: bool,
    /// 有效客户协作关联的单据；无客户维度的资源忽略本维度。
    pub collaborative: bool,
    /// 已由公共解析器展开的内部组织。
    pub org_unit_ids: Vec<String>,
}

impl FundsResolvedClause {
    /// 判断条款是否构成有效资金范围规则。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 含公司、负责人、协作或组织目标时为 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 规则存在但当前无对象不得标为无范围；空条款保持空集且不得补公司。
    pub fn has_scope_rules(&self) -> bool {
        self.company || self.self_owned || self.collaborative || !self.org_unit_ids.is_empty()
    }
}

/// 已通过资格检查的资金范围事实。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FundsResolvedScope {
    /// 已认证操作人。
    pub user_id: String,
    /// 本次解析的资金资源。
    pub resource: String,
    /// 本次解析的资源动作。
    pub action: String,
    /// 同角色正向范围。
    pub role_clauses: Vec<FundsResolvedClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<FundsResolvedClause>,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// 授权解析时点。
    pub as_of: Instant,
}

impl FundsResolvedScope {
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
        self.role_clauses.iter().any(FundsResolvedClause::has_scope_rules)
    }
}

/// 业务域提供的单对象责任事实；不包含原始授权规则。
#[derive(Debug, Clone, Default)]
pub struct FundsScopeObject {
    /// 当前操作人是否为关联销售单或采购单的当前负责人。
    pub owned: bool,
    /// 当前操作人是否具备有效客户协作事实。
    pub collaborating: bool,
    /// 按本资源口径取得的关联单据业务组织。
    pub org_unit_id: Option<String>,
}

/// 资金域消费的窄授权 Port；adapter 调用身份域公共解析器。
#[async_trait]
pub trait FundsDataScopePort: Send + Sync {
    /// 经生产 adapter 复用公共单对象范围判定。
    ///
    /// # 参数
    /// * `scope` - 当前动作已解析事实
    /// * `object` - 本域提供的关联责任事实
    ///
    /// # 返回
    /// 返回角色范围和个人上限共同允许的判定。
    ///
    /// # 错误
    /// 未装配、资源动作不符时失败；禁止以数据库创建判定作为兜底。
    fn allows(&self, _scope: &FundsResolvedScope, _object: &FundsScopeObject) -> Result<bool> {
        Err(unwired())
    }

    /// 在调用方事务内证明资金资源动作并返回已解析事实。
    ///
    /// # 参数
    /// * `actor` - 服务端已认证身份
    /// * `resource` - 资金资源
    /// * `action` - 该资源已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已通过资格检查的范围事实；全量、空集、缺少个人上限可区分。
    ///
    /// # 错误
    /// 未装配、未注册、无动作权限或版本变化必须明确失败。
    ///
    /// # 关键业务约束
    /// 不得把原始 `scope_type / scope_targets` 交给资金域再解析。
    async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<FundsResolvedScope>;

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
    ///
    /// # 关键业务约束
    /// 筛选只能收窄授权结果；不得把完整组织树交给资金域自行重建。
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
    ///
    /// # 关键业务约束
    /// 过期或未生效成员不得进入当前负责人组织筛选。
    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 未接线时失败关闭，不得补公司范围。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFundsDataScopePort;

impl FailClosedFundsDataScopePort {
    /// 返回未接线的共享端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可注入资金服务的失败关闭端口。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 仅用于不解析范围的装载路径；解析入口必须注入真实 adapter。
    pub fn shared() -> Arc<dyn FundsDataScopePort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl FundsDataScopePort for FailClosedFundsDataScopePort {
    async fn resolve(
        &self,
        _actor: &AuditActor,
        _resource: &str,
        _action: &str,
        _executor: &mut dyn Executor,
    ) -> Result<FundsResolvedScope> {
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
    Error::Internal("资金范围端口未接线".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_owner_clause_is_not_no_scope_when_objects_are_empty() {
        assert!(!FundsResolvedClause::default().has_scope_rules());
        assert!(FundsResolvedClause { self_owned: true, ..FundsResolvedClause::default() }.has_scope_rules());
        assert!(
            FundsResolvedClause { collaborative: true, ..FundsResolvedClause::default() }.has_scope_rules()
        );
        assert!(FundsResolvedClause { company: true, ..FundsResolvedClause::default() }.has_scope_rules());
        assert!(
            FundsResolvedClause { org_unit_ids: vec!["org-a".into()], ..FundsResolvedClause::default() }
                .has_scope_rules()
        );
        assert!(
            !FundsResolvedScope {
                user_id: "u1".into(),
                resource: "customer_receipt".into(),
                action: "list".into(),
                role_clauses: vec![],
                user_limit: Some(FundsResolvedClause { self_owned: true, ..FundsResolvedClause::default() }),
                policy_version: 1,
                organization_version: 1,
                scope_version: "v1".into(),
                as_of: Instant::from_unix_secs(0),
            }
            .has_scope_rules()
        );
    }

    #[test]
    fn unwired_port_fails_closed_without_company_fallback() {
        let port = FailClosedFundsDataScopePort;
        assert!(
            port.allows(
                &FundsResolvedScope {
                    user_id: "u1".into(),
                    resource: "customer_receipt".into(),
                    action: "list".into(),
                    role_clauses: vec![FundsResolvedClause {
                        company: true,
                        ..FundsResolvedClause::default()
                    }],
                    user_limit: None,
                    policy_version: 1,
                    organization_version: 1,
                    scope_version: "v1".into(),
                    as_of: Instant::from_unix_secs(0),
                },
                &FundsScopeObject::default(),
            )
            .is_err()
        );
    }
}
