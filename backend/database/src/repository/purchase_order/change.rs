//! `purchase_change_order` / `purchase_change_submission`(+line) 仓储。
//!
//! 采购变更单只适用于实物与服务销售单（§6.6）；仓储影响确认与财务复核均
//! 引用不可变变更提交。变更提交/明细**不提供软删除方法**；变更单本身是
//! 可编辑单据草稿（`StableBase`），可软删除与恢复。

use entities::ids::{PurchaseChangeOrderId, PurchaseChangeSubmissionId};
use entities::purchase_order::{PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionLine};
use mongodb::bson::doc;

use super::common::in_filter;
use crate::executor::Executor;
use crate::{Repository, Result};

impl<'a> Repository<'a, PurchaseChangeOrder> {}

impl<'a> Repository<'a, PurchaseChangeSubmission> {
    /// 按「变更单 + 提交序号」查找唯一变更提交。
    ///
    /// 唯一性由 `uk_purchase_change_submissions_order_no` 唯一索引保证。
    ///
    /// # 参数
    /// * `purchase_change_order_id` - 所属采购变更单
    /// * `submission_no` - 提交序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的变更提交；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_order_and_submission_no(
        &self,
        purchase_change_order_id: &PurchaseChangeOrderId,
        submission_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseChangeSubmission>> {
        self.find_one(
            doc! {
                "purchase_change_order_id": purchase_change_order_id.to_string(),
                "submission_no": submission_no,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, PurchaseChangeSubmissionLine> {
    /// 批量取回多个变更提交的全部明细（`$in`，禁止 N+1）。
    ///
    /// 用于变更提交详情页一次取回行集合；空集合直接返回空结果。
    ///
    /// # 参数
    /// * `submission_ids` - 变更提交 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的变更提交明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_lines_by_submission_ids(
        &self,
        submission_ids: &[PurchaseChangeSubmissionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        if submission_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "purchase_change_submission_id",
                submission_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}
