//! 合同范围授权 Port：只接收已解析事实，不暴露身份域实体或 Service。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 身份域已编译的本次授权阶段。
pub type ContractResolvedScope = crate::authorization::CompiledScope;
#[async_trait]
pub trait ContractDataScopePort: Send + Sync {
    /// 检查完整合同对象。
    /// # 参数
    /// actor、action、id为可信入口和目标；executor为原执行器。
    /// # 返回
    /// identity允许时成功。
    /// # 错误
    /// 未装配或无权限拒绝。
    async fn require(
        &self,
        _actor: &AuditActor,
        _action: &str,
        _id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(unwired())
    }
    /// 检查实际客户来源的合同创建资格。
    /// # 参数
    /// actor和customer_id为实际来源；executor为原执行器。
    /// # 返回
    /// identity生成的阶段凭证。
    /// # 错误
    /// 无权限或来源不存在拒绝。
    async fn require_create(
        &self,
        _actor: &AuditActor,
        _customer_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<ContractResolvedScope> {
        Err(unwired())
    }

    /// 在调用方事务内证明合同资源动作并返回已解析事实。
    ///
    /// # 参数
    /// * `actor` - 服务端已认证身份
    /// * `action` - 合同资源已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已通过资格检查的范围事实；全量、空集、缺少个人上限可区分。
    ///
    /// # 错误
    /// 未装配、未注册、无动作权限或版本变化必须明确失败。
    ///
    /// # 关键业务约束
    /// 资源由本 Port 固定为合同；不得把原始 `scope_type / scope_targets` 交给合同域再解析。
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<ContractResolvedScope>;

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
    /// 筛选只能收窄授权结果；不得把完整组织树交给合同域自行重建。
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
    /// 过期或未生效成员不得进入当前主责组织筛选。
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
pub struct FailClosedContractDataScopePort;

impl FailClosedContractDataScopePort {
    /// 返回未接线的共享端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可注入合同 Service 的失败关闭端口。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 仅用于不解析范围的装载路径；解析入口必须注入真实 adapter。
    pub fn shared() -> Arc<dyn ContractDataScopePort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl ContractDataScopePort for FailClosedContractDataScopePort {
    async fn resolve(
        &self,
        _actor: &AuditActor,
        _action: &str,
        _executor: &mut dyn Executor,
    ) -> Result<ContractResolvedScope> {
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
    Error::Internal("合同范围端口未接线".into())
}
