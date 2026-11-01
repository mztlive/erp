//! 我方签约主体的最小事实；绑定由组合层读取 party 领域。

use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;

/// 只有启用公司主体才可创建模板和申请新号码。
pub struct SigningCompanyFact {
    pub name: String,
    pub active: bool,
}

/// 事务内重验公司主体，禁止按名称猜测编号组。
#[async_trait]
pub trait SigningCompanyPort: Send + Sync {
    /// 读取签约主体。
    /// # 参数
    /// * `id` - 我方公司 ID。
    /// * `executor` - 用例执行器。
    /// # 返回
    /// 公司名称与启停状态；非公司或已删除主体为 None。
    /// # 错误
    /// 事实读取失败。
    async fn company(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<SigningCompanyFact>>;
}
