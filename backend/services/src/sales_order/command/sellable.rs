use std::collections::HashSet;

use entities::sales_order::SalesOrderWorkingCopyLine;
use erp_catalog::CatalogExt;
use erp_core::common::time::BusinessDate;
use persistence_core::{Executor, NoTransaction};

use super::super::dto::SalesOrderDraftLineRequest;
use super::super::SalesOrderService;
use crate::errors::{Error, Result};

impl SalesOrderService {
    /// 校验请求草稿中的实物及服务行仍引用公司商品池内的精确 SKU 修订。
    ///
    /// # 参数
    /// * `lines` - 草稿行请求
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一 `sku_id + sku_revision_id` 不再可销售时返回校验错误。
    pub(super) async fn ensure_sellable_draft_lines(
        &self,
        lines: &[SalesOrderDraftLineRequest],
    ) -> Result<()> {
        let refs = lines
            .iter()
            .filter_map(|line| line.goods.as_ref())
            .map(|goods| (goods.sku_id.to_string(), goods.sku_revision_id.to_string()))
            .collect::<Vec<_>>();
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 提交前重新校验已保存工作副本的精确 SKU 修订资格。
    ///
    /// # 参数
    /// * `lines` - 已保存工作副本行
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 缺 SKU/修订或引用失效时返回校验错误。
    pub(super) async fn ensure_sellable_working_copy_lines(
        &self,
        lines: &[SalesOrderWorkingCopyLine],
    ) -> Result<()> {
        let refs = Self::sellable_working_copy_refs(lines)?;
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 从工作副本行提取必须成对存在的销售 SKU 与修订引用。
    ///
    /// # 参数
    /// * `lines` - 工作副本行
    ///
    /// # 返回
    /// 返回 `(sku_id, sku_revision_id)` 列表。
    ///
    /// # 错误
    /// 实物行缺少 SKU 或修订身份时返回校验错误。
    pub(in crate::sales_order) fn sellable_working_copy_refs(
        lines: &[SalesOrderWorkingCopyLine],
    ) -> Result<Vec<(String, String)>> {
        lines
            .iter()
            .filter_map(|line| match line.sellable_sku_ref() {
                Ok(Some((sku_id, revision_id))) => Some(Ok((sku_id.to_string(), revision_id.to_string()))),
                Ok(None) => None,
                Err(error) => Some(Err(Error::ValidationError(error.to_string()))),
            })
            .collect()
    }

    /// 批量执行公司商品池资格校验并对缺失引用 fail-closed。
    ///
    /// # 参数
    /// * `refs` - `(sku_id, sku_revision_id)` 列表
    /// * `executor` - 事务会话或 `NoTransaction`
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一引用不在当日可销售集合中时返回校验错误。
    pub(in crate::sales_order) async fn ensure_sellable_refs(
        &self,
        refs: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if refs.is_empty() {
            return Ok(());
        }
        let expected = refs.iter().cloned().collect::<HashSet<_>>();
        let qualified = self
            .db
            .catalog()
            .find_sellable_sku_refs(refs, BusinessDate::today(), executor)
            .await?
            .into_iter()
            .map(|row| (row.sku_id, row.sku_revision_id))
            .collect::<HashSet<_>>();
        let mut invalid = expected
            .difference(&qualified)
            .map(|(sku_id, _)| sku_id.clone())
            .collect::<Vec<_>>();
        invalid.sort();
        if invalid.is_empty() {
            Ok(())
        } else {
            Err(erp_catalog::sellable_sku_invalid_error(&invalid).into())
        }
    }
}
