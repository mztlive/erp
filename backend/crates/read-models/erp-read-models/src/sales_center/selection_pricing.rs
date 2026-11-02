//! 按已授权选品册的精确商品修订回读原价；不使用当前商品价格或更新业务快照。

use std::collections::{BTreeMap, BTreeSet};

use erp_catalog::repository::{CatalogExt, SkuRevisionRepositoryExt};
use erp_core::ids::SkuRevisionId;
use erp_core::money::Amount;
use erp_sales::dto::sales_selection::{DisplayItemView, SalesSelectionBookletView};
use erp_sales::entity::sales_selection::{
    DisplayKind, SalesSelectionDisplayItem, SelectionPriceComparison, SkuSnapshot, try_sum,
};
use erp_sales::repository::sales_selection::SalesSelectionDomainRepository;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

type OriginalPrices = BTreeMap<(String, String), Amount>;

/// 为已授权选品册批量补齐原价与累计加价比例。
///
/// # 参数
/// `db` 为数据库，`view` 必须来自已完成册权限校验的详情，`executor` 为调用方读取边界。
///
/// # 返回
/// 原地补齐管理端价格对比；历史册可按原修订恢复原价，无需迁移。
///
/// # 错误
/// 批次读取、修订读取或套餐原价求和失败时返回错误。
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
        .flat_map(SalesSelectionDisplayItem::sellable_refs)
        .map(|(_, revision)| revision)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(SkuRevisionId::new)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let originals = db
        .sku_revisions()
        .find_by_ids(&ids, executor)
        .await?
        .into_iter()
        .filter_map(|revision| {
            revision
                .sales_visible_price_gross
                .map(|price| ((revision.sku_id.to_string(), revision.base.id), price))
        })
        .collect();
    apply_comparisons(&mut view.items, &items, &originals)
}

fn original_price(sku: &SkuSnapshot, originals: &OriginalPrices) -> Option<Amount> {
    originals
        .get(&(sku.sku_id.to_string(), sku.sku_revision_id.to_string()))
        .copied()
        .filter(|price| *price > Amount::zero())
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
        let original = match &item.kind {
            DisplayKind::SingleSku { sku } => original_price(sku, originals),
            DisplayKind::Package { members, .. } => {
                for member in &mut view.members {
                    let original = members
                        .iter()
                        .find(|sku| sku.sku_id_str() == member.sku_id)
                        .and_then(|sku| original_price(sku, originals));
                    member.price_comparison = SelectionPriceComparison::new(member.price, original);
                }
                members
                    .iter()
                    .map(|sku| original_price(sku, originals))
                    .collect::<Option<Vec<_>>>()
                    .map(try_sum)
                    .transpose()?
            },
        };
        view.price_comparison = SelectionPriceComparison::new(view.price, original);
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
            sales_visible_price_gross: price.parse().unwrap(),
        }
    }

    fn view(item: &SalesSelectionDisplayItem) -> DisplayItemView {
        let members = match &item.kind {
            DisplayKind::SingleSku { .. } => vec![],
            DisplayKind::Package { members, .. } => members
                .iter()
                .map(|sku| DisplayMemberView {
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
    fn original_uses_exact_frozen_revision_and_never_the_current_price() {
        let item = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("single"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            sku("a", "121"),
        )
        .unwrap();
        let originals = BTreeMap::from([
            (("a".into(), "rev-a".into()), "100".parse().unwrap()),
            (("a".into(), "new-rev-a".into()), "500".parse().unwrap()),
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
            (("a".into(), "rev-a".into()), "100".parse().unwrap()),
            (("b".into(), "rev-b".into()), "200".parse().unwrap()),
        ]);
        let mut views = vec![view(&item)];
        apply_comparisons(&mut views, slice::from_ref(&item), &originals).unwrap();
        assert_eq!(views[0].price_comparison.original_price, Some("300".parse().unwrap()));
        assert_eq!(views[0].price_comparison.markup_percent, Some("16.67".parse().unwrap()));
        assert_eq!(views[0].members[0].price_comparison.markup_percent, Some("10".parse().unwrap()));
        assert_eq!(views[0].members[1].price_comparison.markup_percent, Some("20".parse().unwrap()));
        assert_eq!(views[0].members[1].price, "240".parse().unwrap());
        let incomplete = BTreeMap::from([(("other-sku".into(), "rev-b".into()), "200".parse().unwrap())]);
        apply_comparisons(&mut views, &[item], &incomplete).unwrap();
        assert_eq!(views[0].price_comparison, SelectionPriceComparison::default());
        assert_eq!(views[0].members[1].price_comparison, SelectionPriceComparison::default());
    }
}
