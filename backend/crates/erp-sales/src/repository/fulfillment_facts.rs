//! 销售拥有的履约分配关联查询。

use erp_core::ids::{SalesOrderLineId, SalesOrderRevisionLineId};
use persistence_core::{Executor, Result};

use crate::entity::sales_order::SalesOrderRevisionLine;
use crate::repository::owned::SalesOrderRevisionLineRepository;

impl SalesOrderRevisionLineRepository<'_> {
    /// 查询同时匹配版本行主键与销售稳定明细的销售版本行。
    ///
    /// # 参数
    /// * `revision_line_id` - 销售版本行主键
    /// * `sales_order_line_id` - 销售稳定明细主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配记录；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn sales_revision_line_for_allocation(
        &self,
        revision_line_id: &SalesOrderRevisionLineId,
        sales_order_line_id: &SalesOrderLineId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderRevisionLine>> {
        self.find_one(
            mongodb::bson::doc! {
                "id": revision_line_id.to_string(),
                "sales_order_line_id": sales_order_line_id.to_string(),
                "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }
}
