//! `purchase_order_revision`(+line) 仓储：生效版本与版本行批量取回。
//!
//! 采购生效版本是不可变修订（§6.6/§4.4）：财务审核通过时由已通过提交原样复制，
//! 修订一经形成不得修改内容。版本与版本行**不提供软删除方法**。

use entities::ids::PurchaseOrderRevisionId;
use entities::purchase_order::{PurchaseOrderRevision, PurchaseOrderRevisionLine};

use super::common::in_filter;
use crate::executor::Executor;
use crate::{Repository, Result};

impl<'a> Repository<'a, PurchaseOrderRevision> {}

impl<'a> Repository<'a, PurchaseOrderRevisionLine> {
    /// 批量取回多个版本的全部明细（`$in`，禁止 N+1）。
    ///
    /// 用于版本详情页一次取回行集合；空集合直接返回空结果。
    ///
    /// # 参数
    /// * `revision_ids` - 生效版本 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的版本明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_lines_by_revision_ids(
        &self,
        revision_ids: &[PurchaseOrderRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderRevisionLine>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "purchase_order_revision_id",
                revision_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}
