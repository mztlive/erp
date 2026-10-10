//! 销售当前版本行提供方；保持原销售单、版本指针、版本行读取顺序。
use std::collections::HashMap;

use async_trait::async_trait;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_procurement::entity::purchase_order::CurrentSalesAllocationLine;
use erp_procurement::ports::purchase_order::SalesAllocationPort;
use erp_procurement::{Error, Result};
use erp_sales::entity::sales_order::LineType;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;
pub(crate) struct SalesAllocationAdapter(pub Database, pub SalesOrderRevisionId);
#[async_trait]
impl SalesAllocationPort for SalesAllocationAdapter {
    async fn current_lines(
        &self,
        id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CurrentSalesAllocationLine>> {
        let sales_order = self
            .0
            .sales_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let revision_id = sales_order
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前版本".to_string()))?;
        if revision_id != self.1.as_ref() {
            return Err(Error::ConflictError("来源销售版本已变化，须先正式变更采购来源后再生效".into()));
        }
        current_revision_lines(&self.0, &self.1, executor).await
    }
}

/// 按精确正式版本连接销售公共商品行与子类型 SKU，不从其他版本猜测。
/// # 参数
/// `db` 为组合数据库；`revision_id` 为正式销售版本；`executor` 沿用调用方执行器。
/// # 返回
/// 返回稳定行、正式版本行和 SKU 身份完整的商品/服务事实。
/// # 错误
/// 商品公共行缺少精确子类型行或仓储查询失败时拒绝。
pub(crate) async fn current_revision_lines(
    db: &Database,
    revision_id: &SalesOrderRevisionId,
    executor: &mut dyn Executor,
) -> Result<Vec<CurrentSalesAllocationLine>> {
    let lines = db.sales_order_revision_lines().list_lines_by_revision(revision_id, executor).await?;
    let lines = lines.into_iter().filter(|line| line.line_type == LineType::GoodsService).collect::<Vec<_>>();
    let ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
    let goods =
        db.sales_order_goods_service_line_revisions().list_by_revision_line_ids(&ids, executor).await?;
    let goods =
        goods.into_iter().map(|line| (line.revision_line_id.to_string(), line)).collect::<HashMap<_, _>>();
    lines
        .into_iter()
        .map(|line| {
            let goods = goods
                .get(&line.base.id)
                .ok_or_else(|| Error::BusinessLogicError("来源销售商品版本行缺少SKU事实".into()))?;
            Ok(CurrentSalesAllocationLine {
                sales_order_line_id: line.sales_order_line_id,
                sales_order_revision_line_id: SalesOrderRevisionLineId::new(line.base.id),
                sku_id: goods.sku_id.clone(),
            })
        })
        .collect()
}
