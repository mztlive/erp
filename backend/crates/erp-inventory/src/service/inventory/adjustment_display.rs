use std::collections::{BTreeSet, HashMap};

use persistence_core::NoTransaction;

use super::InventoryService;
use crate::dto::{StockAdjustmentLineView, StockAdjustmentView};
use crate::error::Result;

impl InventoryService {
    /// 批量补齐已授权调整单表头引用的当前人员姓名。
    ///
    /// # 参数
    /// * `adjustments` - 已完成对象授权的当前页或详情表头；此方法不授予读取权限。
    ///
    /// # 返回
    /// 无返回值；缺失或不可读姓名保持为空。
    ///
    /// # 错误
    /// 人员事实端口未接线或查询失败时返回对应错误。
    pub async fn enrich_adjustment_people_names(
        &self,
        adjustments: &mut [StockAdjustmentView],
    ) -> Result<()> {
        let ids = adjustments
            .iter()
            .flat_map(|adjustment| {
                Some(&adjustment.prepared_by)
                    .into_iter()
                    .chain(adjustment.submitted_by.as_ref())
                    .chain(adjustment.current_assignee.as_ref())
            })
            .filter(|id| !id.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let names = if ids.is_empty() {
            HashMap::new()
        } else {
            self.people_facts.names_by_ids(&ids, &mut NoTransaction).await?
        };
        for adjustment in adjustments {
            adjustment.apply_names(&names);
        }
        Ok(())
    }

    /// 批量补齐已授权调整单明细引用的当前 SKU 编码及修订名称。
    ///
    /// # 参数
    /// * `lines` - 已完成所属调整单授权的明细；此方法不授予读取权限。
    ///
    /// # 返回
    /// 无返回值；缺失关联或不可读编码、名称保持为空。
    ///
    /// # 错误
    /// 商品事实端口未接线或查询失败时返回对应错误。
    pub async fn enrich_adjustment_line_names(&self, lines: &mut [StockAdjustmentLineView]) -> Result<()> {
        let ids = lines
            .iter()
            .map(|line| &line.sku_id)
            .filter(|id| !id.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let skus = if ids.is_empty() {
            HashMap::new()
        } else {
            self.catalog_facts.skus_by_ids(&ids, &mut NoTransaction).await?
        };
        let revision_ids = skus
            .values()
            .filter_map(|sku| sku.current_revision_id.as_ref())
            .filter(|id| !id.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let revisions = if revision_ids.is_empty() {
            HashMap::new()
        } else {
            self.catalog_facts.sku_revisions_by_ids(&revision_ids, &mut NoTransaction).await?
        };
        for line in lines {
            line.apply_sku_names(&skus, &revisions);
        }
        Ok(())
    }
}
