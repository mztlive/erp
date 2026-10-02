//! Adapt current catalog qualification without importing catalog into sales.
use async_trait::async_trait;
use erp_catalog::entity::catalog::SkuSalesPrices;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_core::common::time::BusinessDate;
use erp_core::money::UnitPrice;
use erp_sales::ports::sales_order::{
    SalesReferencePriceFact, SalesReferencePricePort, SalesReferencePriceRequest, SellableSkuPort,
};
use persistence_core::Executor;

use crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery;

/// Catalog provider for exact SKU revision qualification.
pub struct CatalogQualificationAdapter {
    query: std::sync::Arc<dyn CatalogSupplyQueryPort>,
}
impl CatalogQualificationAdapter {
    /// Bind the repository without loading any current catalog facts.
    pub fn new(db: mongodb::Database) -> Self {
        Self { query: std::sync::Arc::new(MongoCatalogSupplyQuery::new(db)) }
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
            .query
            .find_sellable_sku_refs(refs, date, executor)
            .await?
            .into_iter()
            .map(|row| (row.sku_id, row.sku_revision_id))
            .collect())
    }
}

#[async_trait]
impl SalesReferencePricePort for CatalogQualificationAdapter {
    async fn reference_prices(
        &self,
        requests: &[SalesReferencePriceRequest],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<Vec<SalesReferencePriceFact>> {
        let refs = requests
            .iter()
            .map(|request| (request.sku_id.clone(), request.sku_revision_id.clone()))
            .collect::<Vec<_>>();
        let rows = self.query.find_sellable_sku_refs(&refs, date, executor).await?;
        let mut facts = Vec::with_capacity(requests.len());
        for request in requests {
            let Some(row) = rows
                .iter()
                .find(|row| row.sku_id == request.sku_id && row.sku_revision_id == request.sku_revision_id)
            else {
                continue;
            };
            let prices = SkuSalesPrices {
                sales_visible_price_gross: Some(row.sales_visible_price_gross),
                bulk_price_gross: row.bulk_price_gross,
                bulk_min_quantity: row.bulk_min_quantity,
            };
            if let Some(price) = prices.reference_price(request.quantity) {
                facts.push(SalesReferencePriceFact {
                    request: request.clone(),
                    unit_price_gross: UnitPrice::try_from(price.to_decimal())?,
                });
            }
        }
        Ok(facts)
    }
}

#[cfg(test)]
mod tests;
