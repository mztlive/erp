//! 公司商品池分页：当天查询按预算投影，其余按实时资格聚合；配送地区与勾选范围两路共用。

use std::collections::BTreeSet;

use erp_catalog::repository::{CatalogExt, SellableSkuFilter, SellableSkuRow};
use erp_core::region::RegionCode;
use erp_supply::repository::SupplierOfferingExt;
use persistence_core::{Executor, PageResult, Result};

use super::CatalogSupplyRepository;

impl CatalogSupplyRepository<'_> {
    /// 分页查询销售可用的公司 SKU。
    ///
    /// 资格业务日为今天且预算投影已就绪时，先追平少量待处理变化再按投影查询；
    /// 否则按实时资格聚合查询。两路筛选、排序与分页口径一致。
    /// 投影追平由写入编排层在调用前执行，本读取入口不修改投影。
    ///
    /// # 参数
    /// * `filter` - 业务筛选、配送地区、勾选范围、排序与分页
    /// * `restrict` - 调用方先加载的配送与勾选交集；为空集合时直接返回空页。
    /// * `projection_ready` - 调用方已确认当日投影就绪并完成追平。
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页可售行及总数；按配送地区且含运费另计时标出需另付运费的行。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        restrict: Option<&[String]>,
        projection_ready: bool,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>> {
        if restrict.is_some_and(<[String]>::is_empty) {
            return Ok(PageResult { items: Vec::new(), total: 0 });
        }
        let mut page = if projection_ready {
            self.db.sellable_sku_projections().search(filter, restrict, executor).await?
        } else {
            self.search_live(filter, restrict, executor).await?
        };
        if let Some(query) = filter.delivery.as_ref().filter(|query| query.include_freight_extra) {
            self.mark_freight_extra(&mut page.items, &query.region, executor).await?;
        }
        Ok(page)
    }

    /// 读取配送地区命中与勾选范围的交集；保持读取前的短路和错误顺序。
    ///
    /// # 参数
    /// * `filter` - 已规范化的配送与勾选筛选。
    /// * `executor` - 调用方读取快照。
    ///
    /// # 返回
    /// 两侧都未指定时为 `None`；命中为空时为 `Some(Vec::new())`。
    ///
    /// # 错误
    /// 配送覆盖事实读取失败时返回错误。
    pub async fn sellable_sku_scope(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        let delivery = match &filter.delivery {
            Some(query) => Some(self.db.sku_delivery_coverages().matching_sku_ids(query, executor).await?),
            None => None,
        };
        Ok(intersect(filter.sku_ids.as_deref(), delivery))
    }

    /// 标出所选地区内有需另付运费才能送到的地方的当前页商品。
    async fn mark_freight_extra(
        &self,
        items: &mut [SellableSkuRow],
        region: &RegionCode,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let sku_ids = items.iter().map(|row| row.sku_id.clone()).collect::<Vec<_>>();
        let records = self.db.sku_delivery_coverages().find_by_sku_ids(&sku_ids, executor).await?;
        for row in items.iter_mut() {
            row.delivery_freight_extra = records
                .iter()
                .find(|record| record.sku_id == row.sku_id)
                .is_some_and(|record| record.coverage.freight_extra_within(region));
        }
        Ok(())
    }
}

/// 勾选范围与配送地区命中的交集，保持勾选顺序；只指定一侧时取该侧。
fn intersect(requested: Option<&[String]>, delivery: Option<Vec<String>>) -> Option<Vec<String>> {
    match (requested, delivery) {
        (None, None) => None,
        (Some(requested), None) => Some(requested.to_vec()),
        (None, Some(delivery)) => Some(delivery),
        (Some(requested), Some(delivery)) => {
            let delivery = delivery.into_iter().collect::<BTreeSet<_>>();
            Some(requested.iter().filter(|id| delivery.contains(*id)).cloned().collect())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::intersect;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn restriction_combines_selection_and_delivery() {
        assert_eq!(intersect(None, None), None);
        assert_eq!(intersect(Some(&ids(&["a", "b"])), None), Some(ids(&["a", "b"])));
        assert_eq!(intersect(None, Some(ids(&["c"]))), Some(ids(&["c"])));
        assert_eq!(
            intersect(Some(&ids(&["b", "a", "x"])), Some(ids(&["a", "b", "c"]))),
            Some(ids(&["b", "a"]))
        );
        assert_eq!(intersect(Some(&ids(&["x"])), Some(ids(&["a"]))), Some(Vec::new()));
    }
}
