//! `sales_order_submission` 与 `sales_order_submission_line` 仓储。
//!
//! 提交快照是冻结审批对象（事实类），形成后不可修改，**不提供软删除方法**；
//! 被驳回的提交永久保留但不进入经营台账（数据模型 §6.5）。

use entities::sales_order::{
    SalesOrderId, SalesOrderSubmission, SalesOrderSubmissionId, SalesOrderSubmissionLine, SubmissionStatus,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

use super::super::{Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::Result;

/// 提交历史筛选条件。
#[derive(Debug, Clone)]
pub struct SubmissionFilter {
    /// 稳定销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 提交状态；`None` 表示不筛选。
    pub status: Option<SubmissionStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SubmissionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SubmissionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrderSubmission> {
    /// 按销售单与提交序号查找提交快照。
    ///
    /// 唯一性由 `uk_sales_order_submissions_order_submission_no` 唯一索引保证；
    /// 提交头、明细形成后不可修改（数据模型 §6.5）。
    ///
    /// # 参数
    /// * `sales_order_id` - 稳定销售单
    /// * `submission_no` - 提交序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的提交快照；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_order_and_no(
        &self,
        sales_order_id: &SalesOrderId,
        submission_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderSubmission>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "submission_no": submission_no as i32,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesOrderSubmissionLine> {
    /// 按提交 ID 集合批量取回明细（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `submission_ids` - 提交快照 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配明细（未排序，调用方按需分组）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_lines_by_submissions(
        &self,
        submission_ids: &[SalesOrderSubmissionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderSubmissionLine>> {
        if submission_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = submission_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        self.find_many(doc! { "submission_id": { "$in": ids } }, executor)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submission_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SubmissionFilter {
            sales_order_id: Some(entities::sales_order::SalesOrderId::new("o-1")),
            status: Some(SubmissionStatus::InReview),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sales_order_id").unwrap(), "o-1");
        assert_eq!(document.get_str("status").unwrap(), "IN_REVIEW");
    }
}
