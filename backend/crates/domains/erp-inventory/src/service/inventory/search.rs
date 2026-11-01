//! 在库存分页前解析商品关键词，保留显式 SKU 条件的交集。

use erp_core::ids::SkuId;
use persistence_core::Executor;

use crate::error::Result;
use crate::ports::CatalogFactsPort;
use crate::repository::InventorySearch;

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
