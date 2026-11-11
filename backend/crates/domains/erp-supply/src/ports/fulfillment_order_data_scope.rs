//! API 供应商订单范围授权 Port：只接收已解析事实，不暴露身份域实体或 Service。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 身份域编译的授权阶段凭证。
pub type FulfillmentOrderResolvedScope = crate::authorization::CompiledScope;

#[async_trait]
pub trait FulfillmentOrderDataScopePort: Send + Sync {
    /// 统一检查完整对象操作。
    /// # 参数
    /// actor、action 为当前入口；id 为完整目标；executor 为原执行器。
    /// # 返回
    /// identity 允许时成功。
    /// # 错误
    /// 未装配或无权时拒绝。
    async fn require(
        &self,
        _actor: &AuditActor,
        _action: &str,
        _id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(unwired())
    }

    /// 在调用方事务内证明履约订单资源动作并返回已解析事实。
    ///
    /// # 参数
    /// * `actor` - 服务端已认证身份
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
    /// 资源由本 Port 固定为 `supplier_fulfillment_order`。
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentOrderResolvedScope>;

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

/// 当前开放 W26 异常处理人事实；不把处理人物化到订单字段。
#[async_trait]
pub trait FulfillmentExceptionHandlerPort: Send + Sync {
    /// 按订单批量读取当前开放 W26 处理人。
    ///
    /// # 参数
    /// * `order_ids` - 供应商履约订单 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回订单 ID 到当前开放处理人的映射；无开放任务的订单省略。
    ///
    /// # 错误
    /// 未接线或查询失败时拒绝。
    async fn open_handler_user_ids(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;

    /// 列出当前开放 W26 处理人命中的订单 ID。
    ///
    /// # 参数
    /// * `handler_user_ids` - 处理人稳定 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回去重后的订单 ID；无命中时为空。
    ///
    /// # 错误
    /// 未接线或查询失败时拒绝。
    async fn order_ids_for_handlers(
        &self,
        handler_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 未接线时失败关闭，不得补公司范围。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFulfillmentOrderDataScopePort;

impl FailClosedFulfillmentOrderDataScopePort {
    /// 返回未接线的共享端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可注入履约服务的失败关闭端口。
    ///
    /// # 错误
    /// 无。
    pub fn shared() -> Arc<dyn FulfillmentOrderDataScopePort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl FulfillmentOrderDataScopePort for FailClosedFulfillmentOrderDataScopePort {
    async fn resolve(
        &self,
        _actor: &AuditActor,
        _action: &str,
        _executor: &mut dyn Executor,
    ) -> Result<FulfillmentOrderResolvedScope> {
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

/// 未接线时失败关闭的 W26 处理人端口。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFulfillmentExceptionHandlerPort;

impl FailClosedFulfillmentExceptionHandlerPort {
    /// 返回未接线的共享处理人端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回失败关闭端口。
    ///
    /// # 错误
    /// 无。
    pub fn shared() -> Arc<dyn FulfillmentExceptionHandlerPort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl FulfillmentExceptionHandlerPort for FailClosedFulfillmentExceptionHandlerPort {
    async fn open_handler_user_ids(
        &self,
        _order_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Err(unwired())
    }

    async fn order_ids_for_handlers(
        &self,
        _handler_user_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(unwired())
    }
}

/// 未装配授权 Port 时的失败关闭错误。
fn unwired() -> Error {
    Error::Internal("供应商履约订单范围端口未接线".into())
}
