//! 当前开放 W26 异常处理人的跨域事实。
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 当前开放 W26 异常处理人事实；不把处理人物化到订单字段。
#[async_trait]
pub trait FulfillmentExceptionHandlerPort: Send + Sync {
    /// 按订单批量读取当前开放 W26 处理人。
    /// 返回订单 ID 到当前开放处理人的映射；无开放任务的订单省略。
    /// 未接线或查询失败时拒绝。
    async fn open_handler_user_ids(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;

    /// 列出当前开放 W26 处理人命中的订单 ID。
    /// 返回去重后的订单 ID；无命中时为空。
    /// 未接线或查询失败时拒绝。
    async fn order_ids_for_handlers(
        &self,
        handler_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 未接线时失败关闭的 W26 处理人端口。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFulfillmentExceptionHandlerPort;

impl FailClosedFulfillmentExceptionHandlerPort {
    /// 返回未接线的共享处理人端口。
    /// 返回失败关闭端口。
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

// 未装配授权 Port 时的失败关闭错误。
fn unwired() -> Error {
    Error::Internal("供应商履约订单范围端口未接线".into())
}
