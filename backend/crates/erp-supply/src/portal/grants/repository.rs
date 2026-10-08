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
    /// 绑定门户资格仓储使用的数据库。
    ///
    /// # 参数
    /// * `db` - 目标数据库。
    ///
    /// # 返回
    /// 返回尚未发起查询的仓储。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 按供应商与 SKU 读取一条报价资格。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商标识。
    /// * `sku_id` - 公司 SKU 标识。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 找到时返回记录；没有匹配文档时返回 `None`。
    ///
    /// # 错误
    /// 仓储读取失败时返回对应错误。
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
    /// 列出该供应商的全部报价资格。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商标识。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回匹配记录；没有记录时为空向量。
    ///
    /// # 错误
    /// 仓储读取失败时返回对应错误。
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
