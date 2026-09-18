//! 销售当前版本行提供方；保持原销售单、版本指针、版本行读取顺序。
use async_trait::async_trait;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_procurement::entity::purchase_order::CurrentSalesAllocationLine;
use erp_procurement::ports::purchase_order::SalesAllocationPort;
use erp_procurement::{Error, Result};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;
pub(crate) struct SalesAllocationAdapter(pub Database);
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
        let lines = self
            .0
            .sales_order_revision_lines()
            .list_lines_by_revision(&SalesOrderRevisionId::new(revision_id.clone()), executor)
            .await?;
        Ok(lines
            .into_iter()
            .map(|line| CurrentSalesAllocationLine {
                sales_order_line_id: line.sales_order_line_id,
                sales_order_revision_line_id: SalesOrderRevisionLineId::new(line.base.id),
            })
            .collect())
    }
}
