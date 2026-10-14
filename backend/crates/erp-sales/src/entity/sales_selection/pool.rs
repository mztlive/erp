//! 商品池来源快照：筛选条件或勾选身份，准备时冻结。

use erp_core::delivery_coverage::{DeliveryCoverageQuery, DeliveryExtent};
use erp_core::ids::SkuId;
use erp_core::money::Amount;
use erp_core::region::RegionCode;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::limits::{POOL_SKU_MAX, POOL_SKU_MIN};
use super::types::PoolSourceKind;

/// 选品来源的单一价格筛选口径，由目录适配器映射到目录价格。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolPriceKind {
    /// 一件代发价；旧快照默认采用此口径。
    #[default]
    Dropship,
    /// 集采价。
    Bulk,
    /// 市场价。
    Market,
}

/// 与公司商品池列表对齐的筛选条件；不含分页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PoolFilterSnapshot {
    /// 全国可供快捷视图，与其他区域条件同时成立。
    #[serde(default)]
    pub nationwide_only: bool,
    /// 关键字。
    pub q: Option<String>,
    /// 商品类型。
    pub product_kind: Option<String>,
    /// 分类。
    pub category_id: Option<String>,
    /// 品牌。
    pub brand_id: Option<String>,
    /// 供应商（仅筛选，快照不保存供应商身份到陈列）。
    pub supplier_id: Option<String>,
    /// 可供区域。
    pub supply_region: Option<String>,
    /// 去重供应商数量上限。
    pub max_supplier_count: Option<u32>,
    /// 所选价格口径；与上下限一同冻结。
    #[serde(default)]
    pub price_kind: PoolPriceKind,
    /// 售价下限。
    pub sales_price_min: Option<Amount>,
    /// 售价上限。
    pub sales_price_max: Option<Amount>,
    /// 配送地区代码（省、市或区县）；空表示不按配送地区筛选。
    #[serde(default)]
    pub delivery_region_code: Option<String>,
    /// 配送地区范围；空表示全部可送。
    #[serde(default)]
    pub delivery_extent: Option<DeliveryExtent>,
    /// 是否把需另付运费的地区算作可送。
    #[serde(default)]
    pub include_freight_extra: bool,
}

impl PoolFilterSnapshot {
    /// 规范化筛选条件。
    ///
    /// # 参数
    /// * `self` - 原始筛选
    ///
    /// # 返回
    /// 空白字段变为 `None`；分页参数不得进入本结构。
    ///
    /// # 错误
    /// 售价为负、下限大于上限或配送地区代码不存在时拒绝。
    pub fn normalize(self) -> Result<Self> {
        if self.sales_price_min.is_some_and(|value| value < Amount::zero())
            || self.sales_price_max.is_some_and(|value| value < Amount::zero())
        {
            return Err(Error::from("销售价不能为负数"));
        }
        if let (Some(min), Some(max)) = (self.sales_price_min, self.sales_price_max)
            && min > max
        {
            return Err(Error::from("销售价下限不能大于上限"));
        }
        let delivery_region_code = blank_to_none(self.delivery_region_code)
            .map(|code| RegionCode::parse(&code).map(String::from))
            .transpose()?;
        let delivery_selected = delivery_region_code.is_some();
        Ok(Self {
            nationwide_only: self.nationwide_only,
            q: blank_to_none(self.q),
            product_kind: blank_to_none(self.product_kind),
            category_id: blank_to_none(self.category_id),
            brand_id: blank_to_none(self.brand_id),
            supplier_id: blank_to_none(self.supplier_id),
            supply_region: blank_to_none(self.supply_region),
            max_supplier_count: self.max_supplier_count,
            price_kind: self.price_kind,
            sales_price_min: self.sales_price_min,
            sales_price_max: self.sales_price_max,
            delivery_region_code,
            delivery_extent: self.delivery_extent.filter(|_| delivery_selected),
            include_freight_extra: delivery_selected && self.include_freight_extra,
        })
    }

    /// 转为配送地区筛选条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 未选配送地区时返回 `None`；范围默认全部可送。
    ///
    /// # 错误
    /// 冻结的地区代码已不在内置区划中时拒绝。
    pub fn delivery_query(&self) -> Result<Option<DeliveryCoverageQuery>> {
        let Some(code) = self.delivery_region_code.as_deref() else {
            return Ok(None);
        };
        Ok(Some(DeliveryCoverageQuery {
            region: RegionCode::parse(code)?,
            extent: self.delivery_extent.unwrap_or_default(),
            include_freight_extra: self.include_freight_extra,
        }))
    }
}

/// 创建时写入的商品池来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolSource {
    /// 来源类型，创建后不可改。
    pub kind: PoolSourceKind,
    /// 筛选来源的条件。
    pub filter: Option<PoolFilterSnapshot>,
    /// 勾选来源的稳定 SKU 身份，已去重并升序。
    pub sku_ids: Option<Vec<SkuId>>,
}

impl PoolSource {
    /// 构造并校验商品池来源。
    ///
    /// # 参数
    /// * `kind` - 来源类型
    /// * `filter` - 筛选条件
    /// * `sku_ids` - 勾选身份
    ///
    /// # 返回
    /// 返回与类型匹配的来源。
    ///
    /// # 错误
    /// 筛选条件的售价下限大于上限，或勾选身份为空、超过 500、含空白时拒绝。
    /// 与来源类型不匹配的另一侧载荷直接丢弃，不因此失败。
    pub fn new(
        kind: PoolSourceKind,
        filter: Option<PoolFilterSnapshot>,
        sku_ids: Option<Vec<SkuId>>,
    ) -> Result<Self> {
        match kind {
            PoolSourceKind::Filter => {
                let filter = filter.unwrap_or_default().normalize()?;
                Ok(Self { kind, filter: Some(filter), sku_ids: None })
            },
            PoolSourceKind::Selection => {
                let sku_ids = normalize_selected_sku_ids(sku_ids.unwrap_or_default())?;
                Ok(Self { kind, filter: None, sku_ids: Some(sku_ids) })
            },
        }
    }
}

/// 去重、升序并校验勾选身份。
///
/// # 参数
/// * `sku_ids` - 原始勾选
///
/// # 返回
/// 返回稳定升序、无重复的身份。
///
/// # 错误
/// 为空、超限或含空白身份时拒绝。
pub fn normalize_selected_sku_ids(sku_ids: Vec<SkuId>) -> Result<Vec<SkuId>> {
    let mut unique = std::collections::BTreeSet::new();
    for sku_id in sku_ids {
        let value = sku_id.as_ref().trim();
        if value.is_empty() {
            return Err(Error::from("勾选的商品身份无效"));
        }
        unique.insert(value.to_string());
    }
    if unique.len() < POOL_SKU_MIN || unique.len() > POOL_SKU_MAX {
        return Err(Error::from("勾选商品数量必须在 1 到 500 之间"));
    }
    Ok(unique.into_iter().map(SkuId::new).collect())
}

/// 空白字符串视为未筛选。
///
/// # 参数
/// * `value` - 可选文本
///
/// # 返回
/// 去空白后为空则 `None`。
///
/// # 错误
/// 无。
fn blank_to_none(value: Option<String>) -> Option<String> {
    value.map(|item| item.trim().to_string()).filter(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SkuId;

    use super::{PoolSource, normalize_selected_sku_ids};
    use crate::entity::sales_selection::types::PoolSourceKind;

    #[test]
    fn delivery_filter_is_validated_and_dropped_without_region() {
        use erp_core::delivery_coverage::DeliveryExtent;

        use super::PoolFilterSnapshot;

        let old: PoolFilterSnapshot = serde_json::from_str("{}").unwrap();
        assert!(old.delivery_query().unwrap().is_none());

        let snapshot: PoolFilterSnapshot = serde_json::from_value(serde_json::json!({
            "delivery_region_code": " 440300 ", "delivery_extent": "PARTIAL", "include_freight_extra": true
        }))
        .unwrap();
        let normalized = snapshot.normalize().unwrap();
        assert_eq!(normalized.delivery_region_code.as_deref(), Some("440300"));
        let query = normalized.delivery_query().unwrap().unwrap();
        assert_eq!(query.region.as_str(), "440300");
        assert_eq!(query.extent, DeliveryExtent::Partial);
        assert!(query.include_freight_extra);

        let orphan: PoolFilterSnapshot = serde_json::from_value(serde_json::json!({
            "delivery_region_code": " ", "delivery_extent": "PARTIAL", "include_freight_extra": true
        }))
        .unwrap();
        let orphan = orphan.normalize().unwrap();
        assert_eq!((orphan.delivery_extent, orphan.include_freight_extra), (None, false));

        let unknown: PoolFilterSnapshot =
            serde_json::from_value(serde_json::json!({ "delivery_region_code": "999999" })).unwrap();
        assert!(unknown.normalize().is_err());
    }

    #[test]
    fn price_filter_snapshot_preserves_kind_and_rejects_invalid_ranges() {
        use super::{PoolFilterSnapshot, PoolPriceKind};

        let old: PoolFilterSnapshot = serde_json::from_str("{}").unwrap();
        assert_eq!(old.price_kind, PoolPriceKind::Dropship);
        for kind in ["dropship", "bulk", "market"] {
            let input = serde_json::json!({"price_kind": kind, "sales_price_min": "0.00", "sales_price_max": "90.00"});
            let normalized =
                serde_json::from_value::<PoolFilterSnapshot>(input).unwrap().normalize().unwrap();
            assert_eq!(serde_json::to_value(&normalized).unwrap()["price_kind"], kind);
            for (min, max) in [("-1.00", "10.00"), ("20.00", "10.00"), ("0.00", "-0.01")] {
                let input =
                    serde_json::json!({"price_kind": kind, "sales_price_min": min, "sales_price_max": max});
                assert!(serde_json::from_value::<PoolFilterSnapshot>(input).unwrap().normalize().is_err());
            }
        }
        assert!(
            serde_json::from_value::<PoolFilterSnapshot>(serde_json::json!({"price_kind": "unknown"}))
                .is_err()
        );
    }

    #[test]
    fn selection_dedups_and_sorts() {
        let ids =
            normalize_selected_sku_ids(vec![SkuId::new("b"), SkuId::new("a"), SkuId::new("b")]).unwrap();
        assert_eq!(ids, vec![SkuId::new("a"), SkuId::new("b")]);
    }

    #[test]
    fn filter_source_drops_sku_ids() {
        let source = PoolSource::new(PoolSourceKind::Filter, None, Some(vec![SkuId::new("a")])).unwrap();
        assert!(source.sku_ids.is_none());
        assert!(source.filter.is_some());
    }
}
