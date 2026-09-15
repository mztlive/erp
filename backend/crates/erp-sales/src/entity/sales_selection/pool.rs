//! 商品池来源快照：筛选条件或勾选身份，准备时冻结。

use erp_core::ids::SkuId;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::limits::{POOL_SKU_MAX, POOL_SKU_MIN};
use super::types::PoolSourceKind;

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
    /// 售价下限。
    pub sales_price_min: Option<Amount>,
    /// 售价上限。
    pub sales_price_max: Option<Amount>,
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
    /// 售价下限大于上限时拒绝。
    pub fn normalize(self) -> Result<Self> {
        if let (Some(min), Some(max)) = (self.sales_price_min, self.sales_price_max)
            && min > max
        {
            return Err(Error::from("销售价下限不能大于上限"));
        }
        Ok(Self {
            nationwide_only: self.nationwide_only,
            q: blank_to_none(self.q),
            product_kind: blank_to_none(self.product_kind),
            category_id: blank_to_none(self.category_id),
            brand_id: blank_to_none(self.brand_id),
            supplier_id: blank_to_none(self.supplier_id),
            supply_region: blank_to_none(self.supply_region),
            max_supplier_count: self.max_supplier_count,
            sales_price_min: self.sales_price_min,
            sales_price_max: self.sales_price_max,
        })
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
    /// 类型与载荷不一致、勾选为空或超限时拒绝。
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
