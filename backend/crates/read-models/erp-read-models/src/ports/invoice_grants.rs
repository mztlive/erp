//! 开票来源授权就绪检查由组合根装配，查询域不依赖自动授权编排。
use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;
#[async_trait]
pub trait InvoiceGrantReadiness: Send + Sync {
    /// 核验实际分配来源是否仍等待授权处理。
    /// # 参数
    /// request_id 和 recipient 来自服务端申请和认证身份；executor 为原请求事务。
    /// # 返回
    /// 正式事件和当前事实合法、授权尚未就绪时返回 true。
    /// # 错误
    /// 事实错配或读取失败返回错误，不把失败解释为合法来源。
    async fn pending(&self, request_id: &str, recipient: &str, executor: &mut dyn Executor) -> Result<bool>;
}
