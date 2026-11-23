//! 册内客户售价调整；仅修改准备批次快照与陈列。

use std::collections::BTreeMap;
use std::slice;

use erp_core::money::{Amount, round_to_cent};
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::sku_snapshot::package_price;
use super::{BookletStatus, DisplayKind, SalesSelectionDisplayItem, SalesSelectionPoolMember};

/// 客户售价调整方式；百分比以 10 表示加价 10%。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum SelectionPriceChange {
    /// 本册同一商品的所有引用使用指定含税售价。
    Single { sku_id: String, price: Amount },
    /// 当前批次所有商品按当前售价统一加价。
    Markup { percent: Amount },
}

impl SelectionPriceChange {
    /// 原子计算册内客户售价与套餐合计。
    /// 成功后整体替换内存快照；商品身份、修订及图片保持原值。
    /// 失败条件：非待发布、商品不存在、价格非正或溢出时拒绝，输入保持原值。
    ///
    /// 输入约定：`status` 为册状态，`pool` 为本批次商品池，`items` 为本批次有效陈列。
    pub fn apply(
        &self,
        status: BookletStatus,
        pool: &mut [SalesSelectionPoolMember],
        items: &mut [SalesSelectionDisplayItem],
    ) -> Result<()> {
        if status != BookletStatus::PendingPublish {
            return Err(Error::from("只有待发布的选品册可以改价"));
        }

        let prices = self.prices(pool)?;
        let mut updated = items.to_vec();
        for item in &mut updated {
            item.apply_prices(&prices)?;
        }

        for member in pool {
            if let Some(price) = prices.get(member.sku.sku_id_str()) {
                member.sku.sales_visible_price_gross = *price;
            }
        }
        items.clone_from_slice(&updated);
        Ok(())
    }

    fn prices(&self, pool: &[SalesSelectionPoolMember]) -> Result<BTreeMap<String, Amount>> {
        if pool.is_empty() {
            return Err(Error::from("当前准备批次没有商品，请重新准备"));
        }
        match self {
            Self::Single { sku_id, price } => {
                if *price <= Amount::zero() {
                    return Err(Error::from("客户售价必须大于 0"));
                }
                if !pool.iter().any(|member| member.sku.sku_id_str() == sku_id) {
                    return Err(Error::from("商品不属于当前准备批次"));
                }
                Ok(BTreeMap::from([(sku_id.clone(), *price)]))
            },
            Self::Markup { percent } => {
                if *percent <= Amount::zero() {
                    return Err(Error::from("加价百分比必须大于 0"));
                }
                pool.iter()
                    .map(|member| {
                        let price = marked_up(member.sku.sales_visible_price_gross, *percent)?;
                        Ok((member.sku.sku_id_str().to_string(), price))
                    })
                    .collect()
            },
        }
    }
}

impl SalesSelectionDisplayItem {
    fn apply_prices(&mut self, prices: &BTreeMap<String, Amount>) -> Result<()> {
        let (members, package_total) = match &mut self.kind {
            DisplayKind::SingleSku { sku } => (slice::from_mut(sku), None),
            DisplayKind::Package { members, price, .. } => (members.as_mut_slice(), Some(price)),
        };
        for sku in members.iter_mut() {
            if let Some(price) = prices.get(sku.sku_id_str()) {
                sku.sales_visible_price_gross = *price;
            }
        }

        if let Some(price) = package_total {
            *price = package_price(members)?;
        }
        Ok(())
    }
}

fn marked_up(price: Amount, percent: Amount) -> Result<Amount> {
    let factor = percent
        .to_decimal()
        .checked_div(Decimal::from(100))
        .and_then(|rate| Decimal::ONE.checked_add(rate))
        .ok_or_else(|| Error::from("加价后金额超出可表示范围"))?;
    let price =
        price.to_decimal().checked_mul(factor).ok_or_else(|| Error::from("加价后金额超出可表示范围"))?;
    Amount::try_from(round_to_cent(price))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        ProductId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionPoolMemberId,
        SalesSelectionProposalId, SkuId, SkuRevisionId,
    };

    use super::*;
    use crate::entity::sales_selection::{
        PackageCoverRef, SelectionReferencePrices, SessionChoice, SkuSnapshot, build_proposal_lines,
    };

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn fixture() -> (Vec<SalesSelectionPoolMember>, Vec<SalesSelectionDisplayItem>) {
        let skus: Vec<_> = [("a", "10.05"), ("b", "20.15")]
            .into_iter()
            .map(|(id, price)| SkuSnapshot {
                sku_id: SkuId::new(id),
                sku_revision_id: SkuRevisionId::new(format!("rev-{id}")),
                product_id: ProductId::new(format!("p-{id}")),
                product_kind: "PHYSICAL".into(),
                category_id: None,
                name: id.into(),
                specification_attributes: vec![],
                unit: "件".into(),
                image: None,
                reference_prices: Some(SelectionReferencePrices {
                    dropship_price_gross: Some(amount(price)),
                    bulk_price_gross: Some(amount("8")),
                    market_price: None,
                }),
                sales_visible_price_gross: amount(price),
            })
            .collect();

        let pool = skus
            .iter()
            .map(|sku| {
                SalesSelectionPoolMember::new(
                    SalesSelectionPoolMemberId::new(sku.sku_id.to_string()),
                    SalesSelectionBookletId::new("book"),
                    "batch".into(),
                    sku.clone(),
                )
            })
            .collect();

        let single = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("single"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            skus[0].clone(),
        )
        .unwrap();
        let package = SalesSelectionDisplayItem::package(
            SalesSelectionDisplayItemId::new("package"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            "tier".into(),
            skus,
            PackageCoverRef {
                file_asset_id: "cover".into(),
                content_checksum: "checksum".into(),
                storage_object_key: "cover".into(),
                generator_version: "v1".into(),
            },
        )
        .unwrap();
        (pool, vec![single, package])
    }

    #[test]
    fn single_edit_updates_all_occurrences_and_keeps_source_snapshot_unchanged() {
        let (mut pool, mut items) = fixture();
        let source = pool[0].sku.clone();
        let refs = items[1].sellable_refs();
        let cover = items[1].cover_asset_id().unwrap().to_string();

        SelectionPriceChange::Single { sku_id: "a".into(), price: amount("9.99") }
            .apply(BookletStatus::PendingPublish, &mut pool, &mut items)
            .unwrap();

        assert_eq!(source.sales_visible_price_gross, amount("10.05"));
        assert_eq!(pool[0].sku.sales_visible_price_gross, amount("9.99"));
        assert_eq!(pool[0].sku.reference_prices, source.reference_prices);
        let DisplayKind::SingleSku { sku } = &items[0].kind else { panic!("expected single") };
        assert_eq!(sku.reference_prices, source.reference_prices);
        let DisplayKind::Package { members, .. } = &items[1].kind else { panic!("expected package") };
        assert_eq!(members[0].reference_prices, source.reference_prices);
        assert_eq!(pool[1].sku.sales_visible_price_gross, amount("20.15"));
        assert_eq!(items[0].price(), amount("9.99"));
        assert_eq!(items[1].price(), amount("30.14"));

        assert_eq!(items[1].sellable_refs(), refs);
        assert_eq!(items[1].cover_asset_id(), Some(cover.as_str()));
    }

    #[test]
    fn single_change_keeps_unmatched_display_unchanged() {
        let (mut pool, mut items) = fixture();
        let original_single = items[0].clone();
        let original_member = pool[0].clone();

        SelectionPriceChange::Single { sku_id: "b".into(), price: amount("25") }
            .apply(BookletStatus::PendingPublish, &mut pool, &mut items)
            .unwrap();

        assert_eq!(items[0], original_single);
        assert_eq!(pool[0], original_member);
        assert_eq!(pool[1].sku.sales_visible_price_gross, amount("25"));
        assert_eq!(items[1].price(), amount("35.05"));
        let DisplayKind::Package { members, .. } = &items[1].kind else { panic!("expected package") };
        assert_eq!(members[0], pool[0].sku);
        assert_eq!(members[1], pool[1].sku);
    }

    #[test]
    fn empty_price_map_still_recalculates_package_total() {
        let (_, mut items) = fixture();
        let original_single = items[0].clone();
        let DisplayKind::Package { members, price, .. } = &mut items[1].kind else {
            panic!("expected package")
        };
        let original_members = members.clone();
        *price = amount("1");

        for item in &mut items {
            item.apply_prices(&BTreeMap::new()).unwrap();
        }

        assert_eq!(items[0], original_single);
        let DisplayKind::Package { members, price, .. } = &items[1].kind else { panic!("expected package") };
        assert_eq!(members, &original_members);
        assert_eq!(*price, amount("30.20"));
    }

    #[test]
    fn empty_package_rejects_change_without_committing_prior_item_updates() {
        let (mut pool, mut items) = fixture();
        let DisplayKind::Package { members, .. } = &mut items[1].kind else { panic!("expected package") };
        members.clear();
        let original_pool = pool.clone();
        let original_items = items.clone();

        let error = SelectionPriceChange::Single { sku_id: "a".into(), price: amount("15") }
            .apply(BookletStatus::PendingPublish, &mut pool, &mut items)
            .unwrap_err();

        assert_eq!(error.to_string(), "套餐必须包含成员 SKU");
        assert!(matches!(error, Error::LogicError(_)));
        assert_eq!(pool, original_pool);
        assert_eq!(items, original_items);
    }

    #[test]
    fn markup_rounds_each_sku_to_cent_before_summing_packages() {
        let (mut pool, mut items) = fixture();
        let change = SelectionPriceChange::Markup { percent: amount("10") };
        change.apply(BookletStatus::PendingPublish, &mut pool, &mut items).unwrap();
        // 11.055 -> 11.06，22.165 -> 22.16，使用系统银行家舍入。
        assert_eq!(items[0].price(), amount("11.06"));
        assert_eq!(pool[1].sku.sales_visible_price_gross, amount("22.16"));
        assert_eq!(items[1].price(), amount("33.22"));

        change.apply(BookletStatus::PendingPublish, &mut pool, &mut items).unwrap();
        assert_eq!(items[0].price(), amount("12.17"));
        assert_eq!(items[1].price(), amount("36.55"));

        // 重生成从本批次池取出的快照仍使用新客户售价。
        let regenerated = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("regenerated"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            pool[0].sku.clone(),
        )
        .unwrap();
        assert_eq!(regenerated.price(), amount("12.17"));
    }

    #[test]
    fn proposals_use_customer_prices_and_conserve_line_totals() {
        let (mut pool, mut items) = fixture();
        SelectionPriceChange::Single { sku_id: "a".into(), price: amount("15") }
            .apply(BookletStatus::PendingPublish, &mut pool, &mut items)
            .unwrap();

        let choices = vec![SessionChoice {
            display_item_id: SalesSelectionDisplayItemId::new("package"),
            quantity: Some(2),
        }];
        let (display, sku, total) =
            build_proposal_lines(&SalesSelectionProposalId::new("proposal"), &choices, &items, || {
                "line".into()
            })
            .unwrap();
        assert_eq!(total, amount("70.30"));
        assert_eq!(display[0].unit_price, amount("35.15"));
        assert_eq!(sku[0].unit_price, amount("15"));
        assert_eq!(sku[0].line_amount, Some(amount("30")));
    }

    #[test]
    fn non_pending_states_and_invalid_changes_leave_every_price_unchanged() {
        let (pool, items) = fixture();
        let valid = SelectionPriceChange::Markup { percent: amount("10") };
        for status in [
            BookletStatus::Draft,
            BookletStatus::Preparing,
            BookletStatus::Published,
            BookletStatus::Submitted,
            BookletStatus::Closed,
            BookletStatus::Voided,
        ] {
            assert!(valid.apply(status, &mut pool.clone(), &mut items.clone()).is_err());
        }

        for invalid in [
            SelectionPriceChange::Single { sku_id: "missing".into(), price: amount("1") },
            SelectionPriceChange::Single { sku_id: "a".into(), price: amount("0") },
            SelectionPriceChange::Single { sku_id: "a".into(), price: amount("-1") },
            SelectionPriceChange::Markup { percent: amount("0") },
            SelectionPriceChange::Markup { percent: amount("-10") },
        ] {
            let (mut next_pool, mut next_items) = (pool.clone(), items.clone());
            assert!(invalid.apply(BookletStatus::PendingPublish, &mut next_pool, &mut next_items).is_err());
            assert_eq!(next_pool, pool);
            assert_eq!(next_items, items);
        }

        assert!(valid.apply(BookletStatus::PendingPublish, &mut [], &mut []).is_err());
    }

    #[test]
    fn overflow_rejects_entire_change_including_package_sum() {
        let (mut pool, mut items) = fixture();
        let huge = Amount::try_from(Decimal::MAX).unwrap();
        let original_pool = pool.clone();
        let original_items = items.clone();
        let error = SelectionPriceChange::Single { sku_id: "a".into(), price: huge }
            .apply(BookletStatus::PendingPublish, &mut pool, &mut items)
            .unwrap_err();
        assert_eq!(error.to_string(), "金额合计超出可表示范围");
        assert!(matches!(error, Error::LogicError(_)));
        assert_eq!(pool, original_pool);
        assert_eq!(items, original_items);

        pool[0].sku.sales_visible_price_gross = huge;
        let original_pool = pool.clone();
        assert!(
            SelectionPriceChange::Markup { percent: amount("100") }
                .apply(BookletStatus::PendingPublish, &mut pool, &mut items)
                .is_err()
        );
        assert_eq!(pool, original_pool);
        assert_eq!(items, original_items);
    }
}
