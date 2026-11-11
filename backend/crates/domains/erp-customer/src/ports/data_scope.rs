//! 客户授权端口只传输 identity 查询凭证与组织事实。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::CompiledAccess;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 当前读取阶段的授权凭证与展示元信息。
#[derive(Clone)]
pub struct CustomerResolvedScope {
    pub user_id: String,
    pub current: bool,
    pub scope_version: String,
    pub as_of: Instant,
    pub compiled: Option<Arc<CompiledAccess>>,
}

/// 客户仅消费 identity 编译结果，不接收权限条款。
#[async_trait]
pub trait CustomerDataScopePort: Send + Sync {
    /// 生成并验证完整客户查询凭证。
    /// # 参数
    /// actor、action 为当前服务端入口，executor 为调用方事务。
    /// # 返回
    /// 返回绑定当前读取阶段的查询凭证。
    /// # 错误
    /// 身份、登记、权限或事实读取失败时拒绝。
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerResolvedScope>;
    /// 复验读取阶段的凭证。
    /// # 参数
    /// access 为原凭证，executor 为原事务。
    /// # 返回
    /// 凭证仍有效时成功。
    /// # 错误
    /// 权限或映射版本变化时拒绝。
    async fn validate(&self, access: &CustomerResolvedScope, executor: &mut dyn Executor) -> Result<()>;
    /// 检查创建入口的账号与功能资格；对象范围在持久化事务内重验。
    /// # 参数
    /// actor 为当前账号，executor 为原事务。
    /// # 返回
    /// 返回展示元信息。
    /// # 错误
    /// 账号或功能资格失效时拒绝。
    async fn creation(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerResolvedScope>;
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
    /// 筛选只能收窄授权结果；不得把完整组织树交给客户域自行重建。
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

/// 缺少装配时所有授权入口失败。
#[derive(Clone, Copy, Default)]
pub struct FailClosedCustomerDataScopePort;
impl FailClosedCustomerDataScopePort {
    /// 构造失败关闭端口。
    /// # 参数
    /// 无。
    /// # 返回
    /// 共享端口。
    /// # 错误
    /// 无。
    pub fn shared() -> Arc<dyn CustomerDataScopePort> {
        Arc::new(Self)
    }
}
#[async_trait]
impl CustomerDataScopePort for FailClosedCustomerDataScopePort {
    async fn resolve(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<CustomerResolvedScope> {
        Err(unwired())
    }
    async fn validate(&self, _: &CustomerResolvedScope, _: &mut dyn Executor) -> Result<()> {
        Err(unwired())
    }
    async fn creation(&self, _: &AuditActor, _: &mut dyn Executor) -> Result<CustomerResolvedScope> {
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

fn unwired() -> Error {
    Error::Internal("客户授权端口未接线".into())
}
