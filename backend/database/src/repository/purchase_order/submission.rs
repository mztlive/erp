//! `purchase_order_submission`(+line) 仓储：财务审核队列列表投影与明细批量取回。
//!
//! 提交是不可变采购内容快照（§6.6）：财务审批与工作任务必须引用具体提交，
//! 不得审批可变采购主表。提交与明细**不提供软删除方法**。

use entities::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SupplierAccountId};
use entities::purchase_order::{PurchaseOrderSubmission, PurchaseOrderSubmissionLine, SubmissionStatus};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::common::in_filter;
use super::{PurchaseOrderRepository, PURCHASE_ORDER_SUBMISSIONS, PURCHASE_ORDER_SUBMISSION_LINES};
use crate::executor::Executor;
use crate::repository::{Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// 采购提交列表筛选条件（财务审核队列）。
#[derive(Debug, Clone)]
pub struct PurchaseOrderSubmissionFilter {
    /// 所属采购单；`None` 表示不筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 供应商（拆单维度）；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 提交状态；`None` 表示不筛选。
    pub status: Option<SubmissionStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内取值，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PurchaseOrderSubmissionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(purchase_order_id) = &self.purchase_order_id {
            filter.insert("purchase_order_id", purchase_order_id.to_string());
        }
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PurchaseOrderSubmissionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> PurchaseOrderRepository<'a> {
    /// 按采购单读取全部提交，并按提交序号升序返回。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的采购提交。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_submissions_by_order(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderSubmission>> {
        let options = FindOptions::builder()
            .sort(doc! { "submission_no": 1, "id": 1 })
            .build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseOrderSubmission>(PURCHASE_ORDER_SUBMISSIONS),
            doc! { "purchase_order_id": purchase_order_id.to_string() },
            options,
            executor,
        )
        .await
    }

    /// 批量读取采购提交头。
    ///
    /// # 参数
    /// * `submission_ids` - 采购提交稳定身份集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已存在的采购提交头；空输入直接返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_submissions_by_ids(
        &self,
        submission_ids: &[PurchaseOrderSubmissionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderSubmission>> {
        if submission_ids.is_empty() {
            return Ok(Vec::new());
        }
        let options = FindOptions::builder().sort(doc! { "id": 1 }).build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseOrderSubmission>(PURCHASE_ORDER_SUBMISSIONS),
            in_filter("id", submission_ids.iter().map(ToString::to_string)),
            options,
            executor,
        )
        .await
    }

    /// 按采购提交读取全部明细，并按行号升序返回。
    ///
    /// # 参数
    /// * `submission_id` - 采购提交稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的采购提交行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_submission_lines(
        &self,
        submission_id: &PurchaseOrderSubmissionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderSubmissionLine>> {
        let options = FindOptions::builder()
            .sort(doc! { "line_no": 1, "id": 1 })
            .build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseOrderSubmissionLine>(PURCHASE_ORDER_SUBMISSION_LINES),
            doc! { "purchase_order_submission_id": submission_id.to_string() },
            options,
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, PurchaseOrderSubmission> {
    /// 按「采购单 + 提交序号」查找唯一提交。
    ///
    /// 唯一性由 `uk_purchase_order_submissions_order_no` 唯一索引保证。
    ///
    /// # 参数
    /// * `purchase_order_id` - 所属采购单
    /// * `submission_no` - 提交序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的提交；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_order_and_submission_no(
        &self,
        purchase_order_id: &PurchaseOrderId,
        submission_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseOrderSubmission>> {
        self.find_one(
            doc! {
                "purchase_order_id": purchase_order_id.to_string(),
                "submission_no": submission_no,
            },
            executor,
        )
        .await
    }

    /// 按稳定 ID 读取采购审核岗位分离使用的提交事实。
    ///
    /// 工作项入口的历史名称；纯主键读取，直接委托基类单条查询。
    ///
    /// # 参数
    /// * `id` - 采购提交 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除采购提交；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的采购提交集合，不访问采购单主表。
    pub async fn find_work_item_purchase_submission(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseOrderSubmission>> {
        self.find_by_id(id, executor).await
    }

    /// 批量读取采购单关联的全部简报提交。
    ///
    /// 工作项简报 hydration 入口：按采购单 `$in` 一次取回全部提交，禁止 N+1。
    ///
    /// # 参数
    /// * `order_ids` - 采购单稳定 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回采购单命中的全部提交。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的采购提交集合，按所属采购单引用过滤，不访问采购单集合。
    pub async fn list_work_item_brief_submissions_by_orders(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderSubmission>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "purchase_order_id": { "$in": order_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, PurchaseOrderSubmissionLine> {
    /// 批量取回多个提交的全部明细（`$in`，禁止 N+1）。
    ///
    /// 用于提交详情页一次取回行集合；空集合直接返回空结果。
    ///
    /// # 参数
    /// * `submission_ids` - 提交 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的提交明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_lines_by_submission_ids(
        &self,
        submission_ids: &[PurchaseOrderSubmissionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderSubmissionLine>> {
        if submission_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "purchase_order_submission_id",
                submission_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{PurchaseOrderSubmissionFilter, QueryFilter};
    use entities::ids::PurchaseOrderId;
    use entities::purchase_order::SubmissionStatus;
    use mongodb::bson::doc;

    #[test]
    fn submission_filter_applies_order_supplier_and_status() {
        let filter = PurchaseOrderSubmissionFilter {
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            supplier_id: None,
            status: Some(SubmissionStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        assert_eq!(
            filter.to_doc(),
            doc! {
                "deleted_at": 0i64,
                "purchase_order_id": "po-1",
                "status": "PENDING",
            }
        );
    }
}
