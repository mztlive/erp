//! 精确 SKU 及单位资格读取；不向调用方返回内部供给资料。

use erp_catalog::CatalogExt;
use erp_catalog::entity::catalog::ProductKind;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_core::common::time::BusinessDate;
use erp_sales::entity::sales_inquiry::InquiryFulfillmentKind;
use erp_sales::entity::sales_inquiry_import::InquiryImportSku;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery;

pub(super) async fn sku_facts(
    db: &Database,
    codes: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<InquiryImportSku>> {
    if codes.is_empty() {
        return Ok(Vec::new());
    }
    let skus = db.catalog().skus_by_exact_codes(codes, executor).await?;
    let ids = skus.into_iter().map(|sku| sku.base.id).collect::<Vec<_>>();
    let rows = MongoCatalogSupplyQuery::new(db.clone())
        .find_sellable_skus_by_ids(&ids, BusinessDate::today(), executor)
        .await?;
    let unit_ids = rows.iter().map(|row| row.base_unit_id.clone()).collect::<Vec<_>>();
    let units = db.unit_of_measures().list_active_by_ids(&unit_ids, executor).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let unit = units
                .iter()
                .find(|unit| unit.base.id == row.base_unit_id && unit.stable.status.is_active())?;
            let fulfillment_kind = import_kind(row.product_kind)?;
            Some(InquiryImportSku {
                sku_id: row.sku_id,
                sku_revision_id: row.sku_revision_id,
                sku_code: row.sku_no,
                name: row.name,
                specification: row.specification.unwrap_or_default(),
                unit_code: unit.unit_code.clone(),
                quantity_scale: u32::from(unit.quantity_scale),
                fulfillment_kind,
            })
        })
        .collect())
}

fn import_kind(kind: ProductKind) -> Option<InquiryFulfillmentKind> {
    match kind {
        ProductKind::Physical => Some(InquiryFulfillmentKind::Physical),
        ProductKind::Virtual => Some(InquiryFulfillmentKind::Virtual),
        ProductKind::OfflineService => Some(InquiryFulfillmentKind::Service),
        ProductKind::Voucher => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_import_accepts_supported_kinds_and_excludes_vouchers() {
        assert_eq!(import_kind(ProductKind::Physical), Some(InquiryFulfillmentKind::Physical));
        assert_eq!(import_kind(ProductKind::Virtual), Some(InquiryFulfillmentKind::Virtual));
        assert_eq!(import_kind(ProductKind::OfflineService), Some(InquiryFulfillmentKind::Service));
        assert_eq!(import_kind(ProductKind::Voucher), None);
    }
}
