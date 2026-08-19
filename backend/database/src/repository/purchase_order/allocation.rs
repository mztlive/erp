//! `purchase_line_sales_allocation` 仓储：采购行↔销售行分配双向批量查询。
//!
//! 采购行到销售行的数量归属必须显式分配（§9.2）；入库预占必须沿本分配关系
//! 回到原销售明细（§6.6）。两个方向都由 `$in` 批量取回（禁止 N+1），
//! 分别命中唯一索引与反向查询索引。分配是事实类集合，**不提供软删除方法**。

use entities::ids::PurchaseOrderRevisionLineId;
use entities::purchase_order::PurchaseLineSalesAllocation;

use super::common::in_filter;
use crate::executor::Executor;
use crate::{Repository, Result};

impl<'a> Repository<'a, PurchaseLineSalesAllocation> {
    /// 按采购版本行批量取回分配（`$in`，禁止 N+1）。
    ///
    /// 正向查询：给定采购明细，取回其全部销售分配（入库预占沿本关系回到原
    /// 销售明细）；空集合直接返回空结果。
    ///
    /// # 参数
    /// * `revision_line_ids` - 采购版本行 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的分配明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_purchase_revision_line_ids(
        &self,
        revision_line_ids: &[PurchaseOrderRevisionLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseLineSalesAllocation>> {
        if revision_line_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "purchase_order_revision_line_id",
                revision_line_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}
