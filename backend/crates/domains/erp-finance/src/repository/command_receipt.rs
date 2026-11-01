//! 财务独立命令回执的窄读取入口；全部操作复用调用方 Executor。

use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::command_receipt::FinanceCommandReceipt;

/// 财务命令回执仓储；实体、集合和索引归财务领域拥有。
pub type FinanceCommandReceiptRepository<'a> = Repository<'a, FinanceCommandReceipt>;

/// 按当前命令身份读取回执，保持查询规模由 ID 集合约束。
#[allow(async_fn_in_trait)]
pub trait FinanceCommandReceiptRepositoryExt {
    /// 批量读取所有当前 ID 匹配的回执，不用软删除掩盖命令去重事实。
    ///
    /// # 参数
    /// * `ids` - 当前请求的稳定 ID。
    /// * `executor` - 调用方事务或查证执行器。
    /// # 返回
    /// 返回命中回执；空输入不访问数据库。
    /// # 错误
    /// 数据库或反序列化失败时返回错误。
    async fn find_by_candidates(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceCommandReceipt>>;
}

impl FinanceCommandReceiptRepositoryExt for FinanceCommandReceiptRepository<'_> {
    async fn find_by_candidates(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceCommandReceipt>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        mongo_ops::find_many(
            &self.collection(),
            doc! { "id": { "$in": ids } },
            FindOptions::default(),
            executor,
        )
        .await
    }
}
