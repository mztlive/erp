//! 在库存分页前解析商品关键词，保留显式 SKU 条件的交集。

use erp_core::ids::SkuId;
use mongodb::Database;
use persistence_core::Executor;

use crate::dto::StockAvailability;
use crate::error::Result;
use crate::ports::CatalogFactsPort;
use crate::repository::{InventoryExt, InventorySearch};

/// 关键词无命中必须形成空集合条件，不能退回不筛选。
///
/// # 参数
/// * `catalog` - SKU 字面量匹配端口。
/// * `q` - 关键词；`None` 时不调用端口，`Some` 时原样交给端口。
/// * `sku` - 显式 SKU；`None` 表示未指定。
/// * `executor` - 调用方选择的数据访问执行器。
///
/// # 返回
/// 返回关键词命中集合与显式 SKU 的交集搜索条件。
///
/// # 错误
/// 商品匹配端口失败时返回对应错误。
pub(super) async fn sku_filter(
    catalog: &dyn CatalogFactsPort,
    q: Option<&str>,
    sku: Option<&SkuId>,
    executor: &mut dyn Executor,
) -> Result<InventorySearch> {
    let Some(q) = q else {
        return Ok(InventorySearch::skus(None, sku));
    };
    let ids = catalog.matching_sku_ids(q, executor).await?;
    Ok(InventorySearch::skus(Some(ids), sku))
}

/// 余额数量与定位条件由拥有库存集合的仓储解析。
///
/// # 参数
/// * `db` - 库存数据库。
/// * `search` - 已求交的 SKU 搜索条件。
/// * `id` - 精确余额主键；`None` 表示不按主键收窄。
/// * `availability` - 可用量条件。
/// * `executor` - 调用方选择的数据访问执行器。
///
/// # 返回
/// 返回仓储解析后的余额搜索条件。
///
/// # 错误
/// 读取有效预占维度失败时返回仓储错误。
pub(super) async fn balance_filter(
    db: &Database,
    search: InventorySearch,
    id: Option<&str>,
    availability: Option<StockAvailability>,
    executor: &mut dyn Executor,
) -> Result<InventorySearch> {
    Ok(db.inventory().balance_search(search, id, availability, executor).await?)
}

/// 调整单按全部明细匹配 SKU，不能只检查列表首行。
///
/// # 参数
/// * `db` - 库存数据库。
/// * `search` - 已求交的 SKU 搜索条件。
/// * `id` - 精确调整单主键；`None` 表示不按主键收窄。
/// * `executor` - 调用方选择的数据访问执行器。
///
/// # 返回
/// 返回解析到调整单主键集合后的搜索条件。
///
/// # 错误
/// 读取调整明细失败时返回仓储错误。
pub(super) async fn adjustment_filter(
    db: &Database,
    search: InventorySearch,
    id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<InventorySearch> {
    Ok(db.inventory().adjustment_search(search, id, executor).await?)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;

    use super::*;
    use crate::ports::{SkuFact, SkuRevisionFact};

    struct Catalog;
    #[async_trait]
    impl CatalogFactsPort for Catalog {
        async fn matching_sku_ids(&self, q: &str, _: &mut dyn Executor) -> Result<Vec<SkuId>> {
            assert_eq!(q, "杯.[x]");
            Ok(vec![SkuId::new("sku-101")])
        }
        async fn skus_by_ids(&self, _: &[String], _: &mut dyn Executor) -> Result<HashMap<String, SkuFact>> {
            unreachable!()
        }
        async fn sku_revisions_by_ids(
            &self,
            _: &[String],
            _: &mut dyn Executor,
        ) -> Result<HashMap<String, SkuRevisionFact>> {
            unreachable!()
        }
    }

    /// 商品关键词和显式 SKU 求交；冲突时保持空集合而非扩大范围。
    #[tokio::test]
    async fn keyword_intersects_explicit_sku_without_page_truncation() {
        let mut executor = persistence_core::NoTransaction;
        assert_eq!(
            sku_filter(&Catalog, Some("杯.[x]"), None, &mut executor).await.unwrap(),
            InventorySearch::skus(Some(vec![SkuId::new("sku-101")]), None)
        );
        assert_eq!(
            sku_filter(&Catalog, Some("杯.[x]"), Some(&SkuId::new("other")), &mut executor).await.unwrap(),
            InventorySearch::skus(Some(vec![]), None)
        );
        assert_eq!(
            sku_filter(&Catalog, None, Some(&SkuId::new("only")), &mut executor).await.unwrap(),
            InventorySearch::skus(None, Some(&SkuId::new("only")))
        );
    }
}
