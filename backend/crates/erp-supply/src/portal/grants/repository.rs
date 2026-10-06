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
    pub(super) fn new(db: &'a Database) -> Self {
        Self { db }
    }
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
