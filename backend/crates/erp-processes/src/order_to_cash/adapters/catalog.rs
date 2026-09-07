//! Adapt current catalog qualification without importing catalog into sales.
use async_trait::async_trait;
use erp_catalog::CatalogExt;
use erp_core::common::time::BusinessDate;
use erp_sales::ports::sales_order::SellableSkuPort;
use persistence_core::Executor;

/// Catalog provider for exact SKU revision qualification.
pub struct CatalogQualificationAdapter {
    db: mongodb::Database,
}
impl CatalogQualificationAdapter {
    /// Bind the repository without loading any current catalog facts.
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
#[async_trait]
impl SellableSkuPort for CatalogQualificationAdapter {
    async fn qualified_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<Vec<(String, String)>> {
        Ok(self
            .db
            .catalog()
            .find_sellable_sku_refs(refs, date, executor)
            .await?
            .into_iter()
            .map(|row| (row.sku_id, row.sku_revision_id))
            .collect())
    }
}
