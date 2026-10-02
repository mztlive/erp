//! 销售草稿续编使用锁定 SKU 修订的公司参考价。

use std::collections::HashMap;

use erp_catalog::entity::catalog::SkuRevision;
use erp_catalog::repository::{CatalogExt, SkuRevisionRepositoryExt};
use erp_core::ids::{SkuId, SkuRevisionId};
use erp_sales::dto::sales_order::{SalesReferencePricesView, WorkingCopyView};
use persistence_core::NoTransaction;

use super::SalesOrderReadService;
use crate::Result;

impl SalesOrderReadService {
    /// 在已授权的销售草稿中补齐锁定修订的销售参考价。
    ///
    /// # 参数
    /// * `view` - 已读取的销售工作副本
    ///
    /// # 返回
    /// 原地补齐引用匹配的公司价格，不重算或覆盖单据成交金额。
    ///
    /// # 错误
    /// SKU 修订批量读取失败时返回错误；缺失或引用不匹配的修订不补价。
    pub(super) async fn enrich_working_copy_prices(&self, view: &mut WorkingCopyView) -> Result<()> {
        let ids = view.lines.iter().filter_map(|line| line.sku_revision_id.clone()).collect::<Vec<_>>();
        let revisions = self.db.sku_revisions().find_by_ids(&ids, &mut NoTransaction).await?;
        let revisions = revisions.into_iter().map(|revision| (revision.base.id.clone(), revision)).collect();
        apply_reference_prices(view, &revisions);
        Ok(())
    }
}

/// 将精确所属 SKU 与修订对应的参考价补到草稿行。
fn apply_reference_prices(view: &mut WorkingCopyView, revisions: &HashMap<String, SkuRevision>) {
    for line in &mut view.lines {
        line.reference_prices = line
            .sku_id
            .as_ref()
            .zip(line.sku_revision_id.as_ref())
            .and_then(|(sku, id)| reference_prices_for(sku, id, revisions));
    }
}

/// 仅接受锁定修订及其真实所属 SKU，防止关联损坏时错取其他商品价格。
fn reference_prices_for(
    sku: &SkuId,
    id: &SkuRevisionId,
    revisions: &HashMap<String, SkuRevision>,
) -> Option<SalesReferencePricesView> {
    revisions.get(id.as_ref()).filter(|revision| &revision.sku_id == sku).map(prices_from_revision)
}

/// 从不可变公司 SKU 修订选择四个参考价与公司集采门槛。
fn prices_from_revision(revision: &SkuRevision) -> SalesReferencePricesView {
    SalesReferencePricesView {
        factory_price_gross: revision.factory_price_gross,
        sales_visible_price_gross: revision.sales_visible_price_gross,
        bulk_price_gross: revision.bulk_price_gross,
        bulk_min_quantity: revision.bulk_min_quantity,
        market_price: revision.market_price,
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
            factory_price_gross: Some(Amount::from_str("6.00").unwrap()),
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
}
