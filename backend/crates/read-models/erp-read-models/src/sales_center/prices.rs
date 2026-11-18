//! 已授权销售行的价格对比；只读锁定 SKU 修订，不使用当前目录价格。

use std::collections::{HashMap, HashSet};

use erp_catalog::entity::catalog::SkuRevision;
use erp_catalog::repository::{CatalogExt, SkuRevisionRepositoryExt};
use erp_core::ids::{SalesOrderRevisionLineId, SkuId, SkuRevisionId};
use erp_core::money::{Amount, UnitPrice};
use erp_sales::dto::sales_order::{SalesOrderWorkingCopyLineView, SalesReferencePricesView};
use erp_sales::entity::sales_order::SalesProcurementPriceBasis;
use erp_sales::entity::sales_order::inquiry_source::SalesOrderInquirySource;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

mod repository;

use repository::inquiry_sources_by_orders;

use crate::Result;
use crate::sales_center::materials::repository::revision_lines;

/// 同一销售版本行的含税单价对比；缺失原价不伪装成零或未调价。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesLinePrices {
    pub original_procurement_supply_price_gross: Option<UnitPrice>,
    pub procurement_supply_price_gross: Option<UnitPrice>,
    pub unit_price_gross: Option<UnitPrice>,
    pub procurement_price_basis: Option<SalesProcurementPriceBasis>,
}

impl From<&SalesOrderWorkingCopyLineView> for SalesLinePrices {
    fn from(line: &SalesOrderWorkingCopyLineView) -> Self {
        Self {
            original_procurement_supply_price_gross: line.original_procurement_supply_price_gross,
            procurement_supply_price_gross: line.procurement_supply_price_gross,
            unit_price_gross: line.unit_price_gross,
            procurement_price_basis: line.procurement_price_basis,
        }
    }
}

/// 为已授权行批量补齐精确修订价格，缺失档位保持空值。
/// # 参数
/// `db`、已授权的 `lines` 与调用方 `executor`。
/// # 返回
/// 原地补齐价格，不修改保存的销售价和供给价。
/// # 错误
/// 数据库读取失败时返回错误。
pub(crate) async fn enrich_line_prices(
    db: &Database,
    lines: &mut [SalesOrderWorkingCopyLineView],
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = lines.iter().filter_map(|line| line.sku_revision_id.clone()).collect::<HashSet<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let revisions = db.sku_revisions().find_by_ids(&ids.into_iter().collect::<Vec<_>>(), executor).await?;
    let revisions = revisions.into_iter().map(|revision| (revision.base.id.clone(), revision)).collect();

    for line in lines {
        line.reference_prices = line
            .sku_id
            .as_ref()
            .zip(line.sku_revision_id.as_ref())
            .and_then(|(sku, id)| reference_prices_for(sku, id, &revisions));
        line.original_procurement_supply_price_gross =
            original_price(line.reference_prices.as_ref(), line.procurement_price_basis)
                .map(|price| UnitPrice::try_from(price.to_decimal()))
                .transpose()?;
    }
    Ok(())
}

/// 按调用方已经授权的精确销售版本行批量返回价格对比。
/// # 参数
/// `db`、允许的版本行 `ids` 与调用方 `executor`；不得传入未授权行。
/// # 返回
/// 按版本行 ID 索引的价格；不扩大销售单范围。
/// # 错误
/// 数据库读取失败或商品行快照不完整时返回错误。
pub(crate) async fn revision_line_prices(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, SalesLinePrices>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids = ids.iter().cloned().collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();
    let lines = revision_lines(db, &ids, executor).await?;
    let goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(
            &ids.iter().map(SalesOrderRevisionLineId::new).collect::<Vec<_>>(),
            executor,
        )
        .await?;

    let mut views = lines
        .iter()
        .map(|line| {
            SalesOrderWorkingCopyLineView::from_revision_line(
                line,
                goods.iter().find(|goods| goods.revision_line_id.as_ref() == line.base.id),
                None,
            )
            .map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()?;
    enrich_line_prices(db, &mut views, executor).await?;

    let revision_ids =
        lines.iter().map(|line| line.sales_order_revision_id.to_string()).collect::<HashSet<_>>();
    let revisions = db
        .sales_order_revisions()
        .find_revisions_by_ids(&revision_ids.into_iter().collect::<Vec<_>>(), executor)
        .await?;
    let order_ids =
        revisions.iter().map(|revision| revision.sales_order_id.to_string()).collect::<HashSet<_>>();
    enrich_inquiry_prices(db, &mut views, &order_ids.into_iter().collect::<Vec<_>>(), executor).await?;

    Ok(views.iter().map(|line| (line.id.clone(), SalesLinePrices::from(line))).collect())
}

/// 为已授权销售行补齐询价转单时保存的原报价，价格精度保留四位。
/// # 参数
/// `db`、授权行 `lines`、这些行所属的 `order_ids` 与调用方执行器。
/// # 返回
/// 仅在稳定行及 SKU 修订匹配且没有商品池取价档位时采用原询价快照。
/// # 错误
/// 来源读取失败时返回错误；不匹配的行保持原来的未知值。
pub(crate) async fn enrich_inquiry_prices(
    db: &Database,
    lines: &mut [SalesOrderWorkingCopyLineView],
    order_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    if order_ids.is_empty() || !lines.iter().any(|line| line.procurement_price_basis.is_none()) {
        return Ok(());
    }

    let sources = inquiry_sources_by_orders(db, order_ids, executor).await?;
    for line in lines {
        if let Some(price) = inquiry_price(
            &line.sales_order_line_id,
            line.sku_id.as_ref(),
            line.sku_revision_id.as_ref(),
            line.procurement_price_basis,
            &sources,
        ) {
            line.original_procurement_supply_price_gross = Some(price);
        }
    }
    Ok(())
}

fn inquiry_price(
    line_id: &str,
    sku_id: Option<&SkuId>,
    revision_id: Option<&SkuRevisionId>,
    basis: Option<SalesProcurementPriceBasis>,
    sources: &[SalesOrderInquirySource],
) -> Option<UnitPrice> {
    if basis.is_some() {
        return None;
    }
    sources
        .iter()
        .flat_map(|source| &source.lines)
        .find(|binding| {
            binding.sales_order_line_id.as_ref() == line_id
                && sku_id == Some(&binding.sku_id)
                && revision_id == Some(&binding.sku_revision_id)
        })
        .and_then(|binding| binding.price.unit_price_gross)
}

fn reference_prices_for(
    sku: &SkuId,
    id: &SkuRevisionId,
    revisions: &HashMap<String, SkuRevision>,
) -> Option<SalesReferencePricesView> {
    revisions.get(id.as_ref()).filter(|revision| &revision.sku_id == sku).map(|revision| {
        SalesReferencePricesView {
            sales_visible_price_gross: revision.sales_visible_price_gross,
            bulk_price_gross: revision.bulk_price_gross,
            bulk_min_quantity: revision.bulk_min_quantity,
            market_price: revision.market_price,
        }
    })
}

fn original_price(
    prices: Option<&SalesReferencePricesView>,
    basis: Option<SalesProcurementPriceBasis>,
) -> Option<Amount> {
    let prices = prices?;
    match basis? {
        SalesProcurementPriceBasis::Bulk => prices.bulk_price_gross,
        SalesProcurementPriceBasis::Dropship => prices.sales_visible_price_gross,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_catalog::entity::catalog::EnableStatus;
    use erp_catalog::entity::sku_revision::SkuRevisionData;
    use erp_core::common::time::BusinessDate;
    use erp_core::money::{Amount, Quantity};

    use super::*;

    #[test]
    fn draft_reference_prices_use_exact_locked_revision_and_verify_sku_identity() {
        let sku = SkuId::new("sku-1");
        let locked = SkuRevisionId::new("locked");
        let mut data = SkuRevisionData {
            sku_id: sku.clone(),
            revision_no: 1,
            name: "礼盒".into(),
            description: None,
            specification: None,
            barcode: None,
            source_main_image_asset_id: None,
            weight_kg: None,
            volume_m3: None,
            sales_visible_price_gross: Some(Amount::from_str("10.00").unwrap()),
            bulk_price_gross: Some(Amount::from_str("8.00").unwrap()),
            bulk_min_quantity: Some(Quantity::from_str("10.000001").unwrap()),
            market_price: Some(Amount::from_str("12.00").unwrap()),
            status: EnableStatus::Active,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
        };
        let old = SkuRevision::new(locked.clone(), data.clone()).unwrap();
        data.revision_no = 2;
        data.sales_visible_price_gross = Some(Amount::from_str("99.00").unwrap());
        let current = SkuRevision::new(SkuRevisionId::new("current"), data).unwrap();
        let revisions = [(old.base.id.clone(), old), (current.base.id.clone(), current)].into();

        let prices = reference_prices_for(&sku, &locked, &revisions).unwrap();
        assert_eq!(prices.sales_visible_price_gross.unwrap().to_string(), "10.00");
        assert_eq!(prices.bulk_price_gross.unwrap().to_string(), "8.00");
        assert_eq!(prices.bulk_min_quantity.unwrap().to_string(), "10.000001");
        assert!(reference_prices_for(&SkuId::new("another"), &locked, &revisions).is_none());
        assert!(reference_prices_for(&sku, &SkuRevisionId::new("missing"), &revisions).is_none());
    }
    #[test]
    fn original_price_preserves_tier_missing_zero_and_unknown_basis() {
        use SalesProcurementPriceBasis::{Bulk, Dropship};
        let mut prices = SalesReferencePricesView {
            sales_visible_price_gross: Some("10".parse().unwrap()),
            bulk_price_gross: Some("8".parse().unwrap()),
            bulk_min_quantity: None,
            market_price: None,
        };
        assert_eq!(original_price(Some(&prices), Some(Bulk)), prices.bulk_price_gross);
        assert_eq!(original_price(Some(&prices), Some(Dropship)), prices.sales_visible_price_gross);
        assert_eq!(original_price(Some(&prices), None), None);
        assert_eq!(original_price(None, Some(Bulk)), None);

        prices.bulk_price_gross = None;
        assert_eq!(original_price(Some(&prices), Some(Bulk)), None);
        prices.bulk_price_gross = Some("0".parse().unwrap());
        assert_eq!(original_price(Some(&prices), Some(Bulk)), prices.bulk_price_gross);
    }
    #[test]
    fn inquiry_original_price_uses_frozen_binding_and_preserves_four_decimals() {
        use entity_core::BaseModel;
        use erp_core::common::time::Instant;
        use erp_core::ids::{CustomerAccountId, SalesOrderLineId};
        use erp_sales::entity::sales_inquiry::InquirySupplyPrice;
        use erp_sales::entity::sales_order::inquiry_source::InquirySalesLineBinding;

        let sku = SkuId::new("sku");
        let revision = SkuRevisionId::new("locked");
        let other_sku = SkuId::new("other");
        let current_revision = SkuRevisionId::new("current");
        let mut source = SalesOrderInquirySource {
            base: BaseModel::fake(),
            inquiry_id: "inquiry".into(),
            inquiry_no: "IQ-1".into(),
            requirement_version: 1,
            feedback_round: 1,
            customer_id: CustomerAccountId::new("customer"),
            consumed: true,
            released: false,
            fees: Vec::new(),
            lines: vec![InquirySalesLineBinding {
                sales_order_line_id: SalesOrderLineId::new("line"),
                inquiry_line_id: "demand".into(),
                price: InquirySupplyPrice {
                    price_id: "price".into(),
                    unit_price_gross: Some("8.1234".parse().unwrap()),
                    tax_rate: Some("0.13".parse().unwrap()),
                    valid_until: Instant::from_unix_secs(1),
                },
                sku_id: sku.clone(),
                sku_revision_id: revision.clone(),
                fulfillment_due_at: Instant::from_unix_secs(2),
                quantity: "2".parse().unwrap(),
                allocations: Vec::new(),
            }],
        };

        let read = |line, sku, revision, basis, source: &SalesOrderInquirySource| {
            inquiry_price(line, sku, revision, basis, std::slice::from_ref(source))
        };
        assert_eq!(
            read("line", Some(&sku), Some(&revision), None, &source),
            source.lines[0].price.unit_price_gross
        );
        assert_eq!(read("other", Some(&sku), Some(&revision), None, &source), None);
        assert_eq!(read("line", Some(&other_sku), Some(&revision), None, &source), None);
        assert_eq!(read("line", Some(&sku), Some(&current_revision), None, &source), None);
        assert_eq!(
            read("line", Some(&sku), Some(&revision), Some(SalesProcurementPriceBasis::Bulk), &source),
            None
        );

        source.lines[0].price.unit_price_gross = Some("0".parse().unwrap());
        assert_eq!(
            read("line", Some(&sku), Some(&revision), None, &source),
            source.lines[0].price.unit_price_gross
        );

        source.lines[0].price.unit_price_gross = None;
        assert_eq!(read("line", Some(&sku), Some(&revision), None, &source), None);
    }
}
