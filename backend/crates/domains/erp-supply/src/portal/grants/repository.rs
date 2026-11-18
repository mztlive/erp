//! 精确供应商/SKU资格查询，原始BSON仅位于领域仓储。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::super::{PortalSupplyExt, QuoteAccessGrant};
use crate::Result;

pub(super) struct PortalQuoteGrantRepository<'a> {
    db: &'a Database,
}
impl<'a> PortalQuoteGrantRepository<'a> {
    // 绑定门户资格仓储使用的数据库。
    // 返回尚未发起查询的仓储。
    pub(super) fn new(db: &'a Database) -> Self {
        Self { db }
    }

    // 按供应商与 SKU 读取一条报价资格。
    // 找到时返回记录；没有匹配文档时返回 `None`。
    // 仓储读取失败时向上传播错误。
    pub(super) async fn get(
        &self,
        supplier_id: &str,
        sku_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<QuoteAccessGrant>> {
        self.db
            .portal_quote_grants()
            .find_one(doc! {"supplier_id":supplier_id,"sku_id":sku_id}, executor)
            .await
            .map_err(Into::into)
    }
    // 列出该供应商的全部报价资格。
    // 返回匹配记录；没有记录时为空向量。
    // 仓储读取失败时向上传播错误。
    pub(super) async fn list(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<QuoteAccessGrant>> {
        self.db
            .portal_quote_grants()
            .find_many(doc! {"supplier_id":supplier_id}, executor)
            .await
            .map_err(Into::into)
    }
}
