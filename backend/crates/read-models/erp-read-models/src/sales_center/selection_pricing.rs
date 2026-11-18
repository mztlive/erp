//! 优先展示批次冻结参考价；旧册仅按精确修订补齐，禁止读取当前商品价。

use std::collections::{BTreeMap, BTreeSet};

use erp_catalog::repository::{CatalogExt, SkuRevisionRepositoryExt};
use erp_core::ids::SkuRevisionId;
use erp_core::money::Amount;
use erp_sales::dto::sales_selection::{DisplayItemView, SalesSelectionBookletView};
use erp_sales::entity::sales_selection::{
    DisplayKind, SalesSelectionDisplayItem, SelectionPriceComparison, SelectionReferencePrices, SkuSnapshot,
    try_sum,
};
use erp_sales::repository::sales_selection::SalesSelectionDomainRepository;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

type OriginalPrices = BTreeMap<(String, String), SelectionReferencePrices>;

/// 为已授权选品册补齐冻结参考价与相对集采价的加价比例。
///
/// # 参数
/// `db` 为数据库，`view` 必须来自已完成册权限校验的详情，`executor` 为调用方读取边界。
///
/// # 返回
/// 优先使用册内快照；历史册按原修订补齐，缺失参考价不得推算。
///
/// # 错误
/// 批次读取、修订读取或套餐参考价求和失败时返回错误。
pub async fn enrich_selection_prices(
    db: &Database,
    view: &mut SalesSelectionBookletView,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(batch) = view.batch_id.as_deref() else {
        return Ok(());
    };

    let items =
        SalesSelectionDomainRepository::new(db).list_effective_items(&view.id, batch, executor).await?;
    let ids = items
        .iter()
        .flat_map(|item| match &item.kind {
            DisplayKind::SingleSku { sku } => std::slice::from_ref(sku),
            DisplayKind::Package { members, .. } => members.as_slice(),
        })
        .filter(|sku| sku.reference_prices.is_none())
        .map(|sku| sku.sku_revision_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(SkuRevisionId::new)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return apply_comparisons(&mut view.items, &items, &BTreeMap::new());
    }

    let originals = db
        .sku_revisions()
        .find_by_ids(&ids, executor)
        .await?
        .into_iter()
        .map(|revision| {
            (
                (revision.sku_id.to_string(), revision.base.id),
                SelectionReferencePrices {
                    dropship_price_gross: revision.sales_visible_price_gross,
                    bulk_price_gross: revision.bulk_price_gross,
                    market_price: revision.market_price,
                },
            )
        })
        .collect();

    apply_comparisons(&mut view.items, &items, &originals)
}

fn reference_prices(sku: &SkuSnapshot, originals: &OriginalPrices) -> Option<SelectionReferencePrices> {
    sku.reference_prices
        .clone()
        .or_else(|| originals.get(&(sku.sku_id.to_string(), sku.sku_revision_id.to_string())).cloned())
}

fn package_references(
    members: &[SkuSnapshot],
    originals: &OriginalPrices,
) -> Result<SelectionReferencePrices> {
    let prices = members.iter().map(|sku| reference_prices(sku, originals)).collect::<Vec<_>>();
    let sum = |field: fn(&SelectionReferencePrices) -> Option<Amount>| -> Result<Option<Amount>> {
        prices
            .iter()
            .map(|price| price.as_ref().and_then(field))
            .collect::<Option<Vec<_>>>()
            .map(try_sum)
            .transpose()
            .map_err(Into::into)
    };

    Ok(SelectionReferencePrices {
        dropship_price_gross: sum(|price| price.dropship_price_gross)?,
        bulk_price_gross: sum(|price| price.bulk_price_gross)?,
        market_price: sum(|price| price.market_price)?,
    })
}

fn apply_comparisons(
    views: &mut [DisplayItemView],
    items: &[SalesSelectionDisplayItem],
    originals: &OriginalPrices,
) -> Result<()> {
    let items = items.iter().map(|item| (item.base.id.as_str(), item)).collect::<BTreeMap<_, _>>();
    for view in views {
        let Some(item) = items.get(view.id.as_str()) else {
            continue;
        };
        let references = match &item.kind {
            DisplayKind::SingleSku { sku } => reference_prices(sku, originals),
            DisplayKind::Package { members, .. } => {
                for member in &mut view.members {
                    member.reference_prices = members
                        .iter()
                        .find(|sku| sku.sku_id_str() == member.sku_id)
                        .and_then(|sku| reference_prices(sku, originals));
                    member.price_comparison = SelectionPriceComparison::new(
                        member.price,
                        member.reference_prices.as_ref().and_then(|prices| prices.bulk_price_gross),
                    );
                }
                Some(package_references(members, originals)?)
            },
        };

        view.price_comparison = SelectionPriceComparison::new(
            view.price,
            references.as_ref().and_then(|prices| prices.bulk_price_gross),
        );
        view.reference_prices = references;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::slice;

    use erp_core::ids::{
        ProductId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SkuId, SkuRevisionId,
    };
    use erp_sales::dto::sales_selection::DisplayMemberView;
    use erp_sales::entity::sales_selection::PackageCoverRef;

    use super::*;

    fn prices(bulk: &str) -> SelectionReferencePrices {
        SelectionReferencePrices {
            bulk_price_gross: Some(bulk.parse().unwrap()),
            dropship_price_gross: Some("150".parse().unwrap()),
            market_price: Some("200".parse().unwrap()),
        }
    }

    fn sku(id: &str, price: &str) -> SkuSnapshot {
        SkuSnapshot {
            sku_id: SkuId::new(id),
            sku_revision_id: SkuRevisionId::new(format!("rev-{id}")),
            product_id: ProductId::new("product"),
            product_kind: "PHYSICAL".into(),
            category_id: None,
            name: id.into(),
            specification_attributes: vec![],
            unit: "件".into(),
            image: None,
            reference_prices: None,
            sales_visible_price_gross: price.parse().unwrap(),
        }
    }

    fn view(item: &SalesSelectionDisplayItem) -> DisplayItemView {
        let members = match &item.kind {
            DisplayKind::SingleSku { .. } => vec![],
            DisplayKind::Package { members, .. } => members
                .iter()
                .map(|sku| DisplayMemberView {
                    reference_prices: None,
                    price_comparison: Default::default(),
                    sku_id: sku.sku_id.to_string(),
                    name: sku.name.clone(),
                    specification: vec![],
                    unit: sku.unit.clone(),
                    price: sku.sales_visible_price_gross,
                })
                .collect(),
        };
        DisplayItemView {
            reference_prices: None,
            price_comparison: Default::default(),
            sku_id: None,
            id: item.base.id.clone(),
            item_id: item.base.id.clone(),
            kind: "PACKAGE".into(),
            removed: false,
            tier_id: None,
            tier_name: None,
            name: "陈列".into(),
            specification: vec![],
            spec_label: String::new(),
            price: item.price(),
            price_gross: item.price(),
            target_delta: None,
            cover_asset_id: None,
            cover_image: None,
            unit: None,
            members,
            missing_image: true,
        }
    }

    #[test]
    fn package_totals_preserve_zero_and_keep_only_missing_columns_unknown() {
        let mut first = sku("a", "10");
        let mut second = sku("b", "20");
        first.reference_prices = Some(SelectionReferencePrices {
            dropship_price_gross: Some("2".parse().unwrap()),
            bulk_price_gross: Some(Amount::zero()),
            market_price: Some("10".parse().unwrap()),
        });
        second.reference_prices = Some(SelectionReferencePrices {
            dropship_price_gross: Some("3".parse().unwrap()),
            bulk_price_gross: Some(Amount::zero()),
            market_price: None,
        });

        let totals = package_references(&[first.clone(), second.clone()], &BTreeMap::new()).unwrap();
        assert_eq!(totals.dropship_price_gross, Some("5".parse().unwrap()));
        assert_eq!(totals.bulk_price_gross, Some(Amount::zero()));
        assert!(totals.market_price.is_none());

        first.reference_prices.as_mut().unwrap().bulk_price_gross =
            Some("79228162514264337593543950335".parse().unwrap());
        second.reference_prices.as_mut().unwrap().bulk_price_gross = Some("1".parse().unwrap());
        assert!(package_references(&[first, second], &BTreeMap::new()).is_err());
    }

    #[test]
    fn frozen_references_survive_storage_and_override_revision_prices() {
        let mut frozen = sku("a", "121");
        frozen.reference_prices = Some(prices("100"));
        let frozen: SkuSnapshot = serde_json::from_value(serde_json::to_value(frozen).unwrap()).unwrap();
        let item = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("single"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            frozen,
        )
        .unwrap();
        let mut views = vec![view(&item)];
        let changed = BTreeMap::from([(("a".into(), "rev-a".into()), prices("500"))]);

        apply_comparisons(&mut views, slice::from_ref(&item), &changed).unwrap();
        assert_eq!(views[0].reference_prices, Some(prices("100")));
        assert_eq!(views[0].price_comparison.markup_percent, Some("21".parse().unwrap()));

        apply_comparisons(&mut views, &[item], &BTreeMap::new()).unwrap();
        assert_eq!(views[0].reference_prices, Some(prices("100")));
    }

    #[test]
    fn frozen_missing_prices_stay_missing_and_legacy_snapshot_deserializes() {
        let mut frozen = sku("a", "121");
        let mut legacy = serde_json::to_value(&frozen).unwrap();
        legacy.as_object_mut().unwrap().remove("reference_prices");
        assert!(serde_json::from_value::<SkuSnapshot>(legacy).unwrap().reference_prices.is_none());

        frozen.reference_prices = Some(SelectionReferencePrices::default());
        let item = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("single"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            frozen,
        )
        .unwrap();
        let mut views = vec![view(&item)];
        let changed = BTreeMap::from([(("a".into(), "rev-a".into()), prices("500"))]);
        apply_comparisons(&mut views, &[item], &changed).unwrap();
        assert_eq!(views[0].reference_prices, Some(SelectionReferencePrices::default()));
        assert_eq!(views[0].price_comparison, SelectionPriceComparison::default());
    }

    #[test]
    fn original_uses_exact_frozen_revision_and_never_the_current_price() {
        let item = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("single"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            sku("a", "121"),
        )
        .unwrap();
        let originals = BTreeMap::from([
            (("a".into(), "rev-a".into()), prices("100")),
            (("a".into(), "new-rev-a".into()), prices("500")),
        ]);
        let mut views = vec![view(&item)];

        apply_comparisons(&mut views, &[item], &originals).unwrap();
        assert_eq!(views[0].price_comparison.original_price, Some("100".parse().unwrap()));
        assert_eq!(views[0].price_comparison.markup_percent, Some("21".parse().unwrap()));
        assert_eq!(views[0].price, "121".parse().unwrap());

        // 旧响应没有新增对比字段时仍可恢复。
        let mut legacy = serde_json::to_value(&views[0]).unwrap();
        legacy.as_object_mut().unwrap().remove("original_price");
        legacy.as_object_mut().unwrap().remove("markup_percent");
        let restored: DisplayItemView = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.price_comparison, SelectionPriceComparison::default());
    }

    #[test]
    fn package_comparison_sums_member_originals_and_preserves_member_quotes() {
        let item = SalesSelectionDisplayItem::package(
            SalesSelectionDisplayItemId::new("package"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            "tier".into(),
            vec![sku("a", "110"), sku("b", "240")],
            PackageCoverRef {
                file_asset_id: "cover".into(),
                content_checksum: "checksum".into(),
                storage_object_key: "cover".into(),
                generator_version: "v1".into(),
            },
        )
        .unwrap();
        let originals = BTreeMap::from([
            (("a".into(), "rev-a".into()), prices("100")),
            (("b".into(), "rev-b".into()), prices("200")),
        ]);
        let mut views = vec![view(&item)];

        apply_comparisons(&mut views, slice::from_ref(&item), &originals).unwrap();
        assert_eq!(views[0].price_comparison.original_price, Some("300".parse().unwrap()));
        assert_eq!(views[0].price_comparison.markup_percent, Some("16.67".parse().unwrap()));
        assert_eq!(views[0].members[0].price_comparison.markup_percent, Some("10".parse().unwrap()));
        assert_eq!(views[0].members[1].price_comparison.markup_percent, Some("20".parse().unwrap()));
        assert_eq!(views[0].members[1].price, "240".parse().unwrap());

        let incomplete = BTreeMap::from([(("other-sku".into(), "rev-b".into()), prices("200"))]);
        apply_comparisons(&mut views, &[item], &incomplete).unwrap();
        assert_eq!(views[0].price_comparison, SelectionPriceComparison::default());
        assert_eq!(views[0].members[1].price_comparison, SelectionPriceComparison::default());
    }
}
