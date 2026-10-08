//! 供应商履约明细：按冻结供给修订批量补齐 SKU 名称与单位。

use erp_supply::dto::supplier_fulfillment::SupplierFulfillmentItemView;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentItem;
use persistence_core::NoTransaction;

use super::fulfillment_detail::SupplierFulfillmentDetailReadService;
use super::fulfillment_dto::SupplierFulfillmentItemDisplayView;
use super::repository::fulfillment_display::{ItemNames, item_names};
use crate::Result;

impl SupplierFulfillmentDetailReadService {
    pub(super) async fn item_display(
        &self,
        items: Vec<SupplierFulfillmentItem>,
    ) -> Result<Vec<SupplierFulfillmentItemDisplayView>> {
        let names = item_names(&self.db, &items, &mut NoTransaction).await?;
        Ok(items.into_iter().map(|item| item_view(item, &names)).collect())
    }
}

fn item_view(item: SupplierFulfillmentItem, names: &ItemNames) -> SupplierFulfillmentItemDisplayView {
    let revision = names.revisions.get(item.supplier_offering_revision_id.as_ref());
    let offering = revision.and_then(|(offering_id, _)| names.offerings.get(offering_id));
    let sku = offering.and_then(|sku_id| names.skus.get(sku_id));
    let sku_revision = offering.and_then(|sku_id| {
        let (revision_id, _) = names.skus.get(sku_id)?;
        let revision = names.sku_revisions.get(revision_id.as_deref()?)?;
        (revision.sku_id.as_ref() == sku_id.as_str()).then_some(&revision.name)
    });
    let unit = sku.and_then(|(_, unit_id)| names.units.get(unit_id));
    SupplierFulfillmentItemDisplayView {
        product_name: sku_revision.cloned(),
        unit_name: unit.cloned(),
        supplier_offering_revision_no: revision.map(|(_, revision_no)| *revision_no),
        item: SupplierFulfillmentItemView {
            id: item.base.id,
            supplier_fulfillment_order_id: item.supplier_fulfillment_order_id.to_string(),
            supplier_offering_revision_id: item.supplier_offering_revision_id.to_string(),
            supplier_sku_code_snapshot: item.supplier_sku_code_snapshot,
            supplier_product_code_snapshot: item.supplier_product_code_snapshot,
            quantity: item.quantity,
            unit_cost_snapshot_gross: item.unit_cost_snapshot_gross,
            cost_snapshot_total_gross: item.cost_snapshot_total_gross,
            input_tax_rate: item.input_tax_rate,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use erp_core::ids::{
        SkuId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierOfferingRevisionId,
    };
    use erp_core::money::{Quantity, Rate, UnitPrice};
    use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentItemData;

    use super::*;
    use crate::supplier_center::repository::fulfillment_display::SkuRevisionName;

    fn item() -> SupplierFulfillmentItem {
        let data = SupplierFulfillmentItemData::from_unit_cost(
            SupplierFulfillmentOrderId::new("order-1"),
            SupplierOfferingRevisionId::new("offering-revision-1"),
            "SUP-SKU-1",
            Some("SUP-SPU-1".into()),
            Quantity::from_str("3").unwrap(),
            UnitPrice::from_str("9.99").unwrap(),
            Rate::from_str("0.13").unwrap(),
        )
        .unwrap();
        SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-1"), data).unwrap()
    }

    #[test]
    fn frozen_offering_revision_selects_readable_sku_and_unit() {
        let names = ItemNames {
            revisions: HashMap::from([("offering-revision-1".into(), ("offering-1".into(), 7))]),
            offerings: HashMap::from([("offering-1".into(), "sku-1".into())]),
            skus: HashMap::from([("sku-1".into(), (Some("sku-revision-1".into()), "unit-1".into()))]),
            sku_revisions: HashMap::from([(
                "sku-revision-1".into(),
                SkuRevisionName { sku_id: SkuId::new("sku-1"), name: "节日福利礼包".into() },
            )]),
            units: HashMap::from([("unit-1".into(), "盒".into())]),
        };
        let view = item_view(item(), &names);
        assert_eq!(view.product_name.as_deref(), Some("节日福利礼包"));
        assert_eq!(view.unit_name.as_deref(), Some("盒"));
        assert_eq!(view.supplier_offering_revision_no, Some(7));
        let json = serde_json::to_value(view).unwrap();
        assert_eq!(json["supplier_sku_code_snapshot"], "SUP-SKU-1");
        assert_eq!(json["quantity"], serde_json::to_value(item().quantity).unwrap());
    }

    #[test]
    fn missing_relations_leave_name_empty_without_id_fallback() {
        let view = item_view(item(), &ItemNames::default());
        assert!(view.product_name.is_none());
        assert!(view.unit_name.is_none());
        assert!(view.supplier_offering_revision_no.is_none());
        assert_eq!(view.item.supplier_offering_revision_id, "offering-revision-1");
        assert_eq!(view.item.supplier_product_code_snapshot.as_deref(), Some("SUP-SPU-1"));
    }

    #[test]
    fn another_sku_revision_does_not_supply_product_name() {
        let names = ItemNames {
            revisions: HashMap::from([("offering-revision-1".into(), ("offering-1".into(), 7))]),
            offerings: HashMap::from([("offering-1".into(), "sku-1".into())]),
            skus: HashMap::from([("sku-1".into(), (Some("sku-revision-2".into()), "unit-1".into()))]),
            sku_revisions: HashMap::from([(
                "sku-revision-2".into(),
                SkuRevisionName { sku_id: SkuId::new("sku-2"), name: "其它 SKU 商品".into() },
            )]),
            units: HashMap::from([("unit-1".into(), "盒".into())]),
        };
        let view = item_view(item(), &names);
        assert!(view.product_name.is_none());
        assert_eq!(view.unit_name.as_deref(), Some("盒"));
        assert_eq!(view.supplier_offering_revision_no, Some(7));
        assert_eq!(view.item.id, "item-1");
        assert_eq!(view.item.supplier_fulfillment_order_id, "order-1");
        assert_eq!(view.item.supplier_offering_revision_id, "offering-revision-1");
        assert_eq!(view.item.supplier_sku_code_snapshot, "SUP-SKU-1");
    }
}
