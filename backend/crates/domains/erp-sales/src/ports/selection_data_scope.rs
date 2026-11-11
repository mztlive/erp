//! 选品册与方案范围授权 Port：只接收已解析事实，不暴露身份域实体或 Service。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::{AuthorizationService, CompiledAccess};
use persistence_core::Executor;

use crate::error::{Error, Result};
use crate::service::authorization_error::identity_error;

/// 身份域生成的查询凭证及展示元数据；不传递权限规则。
#[derive(Clone)]
pub struct SelectionResolvedScope {
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub scope_version: String,
    pub as_of: Instant,
    pub access: Arc<CompiledAccess>,
    pub authorization: AuthorizationService,
}

impl SelectionResolvedScope {
    /// 在原执行器复验授权。
    /// # 参数
    /// executor 为原读取执行器。
    /// # 返回
    /// 当前凭证仍有效时成功。
    /// # 错误
    /// 身份、映射、时点或版本变化时拒绝。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.authorization.validate(&self.access, executor).await.map_err(identity_error)
    }
}

/// 选品域消费的窄授权 Port；adapter 调用身份域公共解析器。
#[async_trait]
pub trait SelectionDataScopePort: Send + Sync {
    /// 检查完整业务对象。
    /// # 参数
    /// actor、resource、action 与 id 指定服务端操作目标，executor 为原事务。
    /// # 返回
    /// 统一授权允许时成功。
    /// # 错误
    /// 目标缺失、无资格或身份设施失败时拒绝。
    async fn require(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let _ = (actor, resource, action, id, executor);
        Err(unwired())
    }

    /// 检查拟创建选品册的可信责任事实。
    /// # 参数
    /// actor 为操作人，owner/org 为已校验的拟写入责任，executor 为原事务。
    /// # 返回
    /// 统一授权允许时成功。
    /// # 错误
    /// 创建资格或责任范围不足时拒绝。
    async fn require_new(
        &self,
        actor: &AuditActor,
        owner: &str,
        org: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let _ = (actor, owner, org, executor);
        Err(unwired())
    }

    /// 在调用方事务内证明选品资源动作并返回已解析事实。
    ///
    /// # 参数
    /// * `actor` - 服务端已认证身份
    /// * `resource` - 选品册或方案资源
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
    /// 不得把原始 `scope_type / scope_targets` 交给选品域再解析。
    async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<SelectionResolvedScope>;

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
    /// 筛选只能收窄授权结果；不得把完整组织树交给选品域自行重建。
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
    /// 过期或未生效成员不得进入当前责任组织筛选。
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
pub struct FailClosedSelectionDataScopePort;

impl FailClosedSelectionDataScopePort {
    /// 返回未接线的共享端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可注入选品服务的失败关闭端口。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 仅用于不解析范围的装载路径；解析入口必须注入真实 adapter。
    pub fn shared() -> Arc<dyn SelectionDataScopePort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl SelectionDataScopePort for FailClosedSelectionDataScopePort {
    async fn resolve(
        &self,
        _actor: &AuditActor,
        _resource: &str,
        _action: &str,
        _executor: &mut dyn Executor,
    ) -> Result<SelectionResolvedScope> {
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
    Error::Internal("选品范围端口未接线".into())
}
