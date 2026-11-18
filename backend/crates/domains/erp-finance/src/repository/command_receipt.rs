//! 财务独立命令回执的窄读取入口；全部操作复用调用方 Executor。

use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::command_receipt::FinanceCommandReceipt;

/// 财务命令回执仓储；实体、集合和索引归财务领域拥有。
pub type FinanceCommandReceiptRepository<'a> = Repository<'a, FinanceCommandReceipt>;

/// 按精确命令身份读取，包括软删除记录，以便识别损坏回执。
#[allow(async_fn_in_trait)]
pub trait FinanceCommandReceiptRepositoryExt {
    async fn find_by_command_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceCommandReceipt>>;
}

impl FinanceCommandReceiptRepositoryExt for FinanceCommandReceiptRepository<'_> {
    async fn find_by_command_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceCommandReceipt>> {
        mongo_ops::find_many(&self.collection(), doc! { "id": id }, FindOptions::default(), executor).await
    }
}
