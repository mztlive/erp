//! 应付进度条件更新入口（金额 BSON 形态与进度管道复用仓储共用实现）。

use mongodb::bson::Document;
use persistence_core::{Executor, Result};

use crate::repository::owned::PayableAccountRepository;
pub(super) use crate::repository::progress::{amount_bson, progress_pipeline};

impl<'a> PayableAccountRepository<'a> {
    /// 执行单文档条件更新（管道形态）。
    ///
    /// 直接按执行器会话语义执行：带会话时加入调用方事务，否则自动提交；
    /// 仓储不自行开启或提交事务。
    ///
    /// # 参数
    /// * `filter` - 更新条件（含核销进度守卫）
    /// * `pipeline` - 聚合管道更新（重算进度与派生状态）
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 条件命中并完成更新时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub(super) async fn conditional_update(
        &self,
        filter: Document,
        pipeline: Vec<Document>,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let result = match executor.session() {
            Some(session) => self.collection().update_one(filter, pipeline).session(session).await?,
            None => self.collection().update_one(filter, pipeline).await?,
        };
        Ok(result.matched_count == 1)
    }
}
