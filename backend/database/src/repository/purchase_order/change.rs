//! `purchase_change_order` / `purchase_change_submission`(+line) 仓储。
//!
//! 采购变更单只适用于实物与服务销售单（§6.6）；仓储影响确认与财务复核均
//! 引用不可变变更提交。变更提交/明细**不提供软删除方法**；变更单本身是
//! 可编辑单据草稿（`StableBase`），可软删除与恢复。

use entities::ids::{PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId};
use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeOrderStatus, PurchaseChangeSubmission, PurchaseChangeSubmissionLine,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::common::in_filter;
use super::{PurchaseOrderRepository, PURCHASE_CHANGE_ORDERS};
use crate::executor::Executor;
use crate::repository::PageResult;
use crate::{mongo_ops, Repository, Result};

impl<'a> PurchaseOrderRepository<'a> {
    /// 分页查询采购变更单，并按创建时间稳定排序。
    ///
    /// # 参数
    /// * `purchase_order_id` - 可选原采购单筛选
    /// * `status` - 可选状态代码筛选
    /// * `page` - 页码，从 1 开始
    /// * `page_size` - 单页条数
    /// * `sort_ascending` - `true` 按创建时间升序，否则降序
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页采购变更单与满足条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、计数或游标读取失败时返回错误。
    pub async fn search_change_orders(
        &self,
        purchase_order_id: Option<&str>,
        status: Option<&str>,
        page: u64,
        page_size: u32,
        sort_ascending: bool,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseChangeOrder>> {
        let filter = change_order_filter(purchase_order_id, status);
        let skip = page.saturating_sub(1).saturating_mul(u64::from(page_size));
        let direction = if sort_ascending { 1 } else { -1 };
        let options = FindOptions::builder()
            .sort(doc! { "created_at": direction })
            .skip(skip)
            .limit(i64::from(page_size))
            .build();
        let collection = self.db.collection::<PurchaseChangeOrder>(PURCHASE_CHANGE_ORDERS);
        let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&collection, filter, executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 判断采购单是否存在草稿或审批中的变更单。
    ///
    /// # 参数
    /// * `purchase_order_id` - 原采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 存在未结束变更时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn has_in_progress_change(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let filter = doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "purchase_order_id": purchase_order_id.to_string(),
            "status": { "$in": [
                PurchaseChangeOrderStatus::Draft.as_str(),
                PurchaseChangeOrderStatus::InApproval.as_str(),
            ] },
        };
        Ok(mongo_ops::count_documents(
            &self.db.collection::<PurchaseChangeOrder>(PURCHASE_CHANGE_ORDERS),
            filter,
            executor,
        )
        .await?
            > 0)
    }

    /// 按原采购单读取全部变更单，并按创建时间升序返回。
    ///
    /// # 参数
    /// * `purchase_order_id` - 原采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的采购变更单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_changes_by_order(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeOrder>> {
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1, "id": 1 })
            .build();
        mongo_ops::find_many(
            &self.db.collection::<PurchaseChangeOrder>(PURCHASE_CHANGE_ORDERS),
            change_order_filter(Some(purchase_order_id.as_ref()), None),
            options,
            executor,
        )
        .await
    }

    /// 按变更单读取全部提交，并按提交序号升序返回。
    ///
    /// # 参数
    /// * `change_order_id` - 采购变更单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的变更提交。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_change_submissions_by_order(
        &self,
        change_order_id: &PurchaseChangeOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmission>> {
        let options = FindOptions::builder()
            .sort(doc! { "submission_no": 1, "id": 1 })
            .build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseChangeSubmission>(super::PURCHASE_CHANGE_SUBMISSIONS),
            doc! { "purchase_change_order_id": change_order_id.to_string() },
            options,
            executor,
        )
        .await
    }

    /// 按变更提交读取全部明细，并按行号升序返回。
    ///
    /// # 参数
    /// * `submission_id` - 采购变更提交稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的变更提交行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_change_submission_lines(
        &self,
        submission_id: &PurchaseChangeSubmissionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        let options = FindOptions::builder()
            .sort(doc! { "line_no": 1, "id": 1 })
            .build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseChangeSubmissionLine>(super::PURCHASE_CHANGE_SUBMISSION_LINES),
            doc! { "purchase_change_submission_id": submission_id.to_string() },
            options,
            executor,
        )
        .await
    }
}

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

/// 构造采购变更单筛选条件并排除软删除记录。
///
/// # 参数
/// * `purchase_order_id` - 可选原采购单筛选
/// * `status` - 可选状态代码筛选
///
/// # 返回
/// 返回 MongoDB 查询条件。
fn change_order_filter(purchase_order_id: Option<&str>, status: Option<&str>) -> Document {
    let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
    if let Some(purchase_order_id) = purchase_order_id {
        filter.insert("purchase_order_id", purchase_order_id);
    }
    if let Some(status) = status {
        filter.insert("status", status);
    }
    filter
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
