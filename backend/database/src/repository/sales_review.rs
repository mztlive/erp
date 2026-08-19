//! 域 D14 `sales_review` 仓储：sales_change_order、sales_change_submission(+_line)。
//!
//! 旧采购确认、低毛利确认、卡券审批记录与变更复核集合已删除。

use entities::ids::{SalesChangeOrderId, SalesChangeSubmissionId, SalesOrderId};
use entities::sales_review::{
    SalesChangeOrder, SalesChangeOrderStatus, SalesChangeSubmission, SalesChangeSubmissionLine,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::SalesReviewExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `sales_change_order` 集合名。
const SALES_CHANGE_ORDERS: &str = <mongodb::Database as SalesReviewExt>::SALES_CHANGE_ORDERS;
/// `sales_change_submission` 集合名。
const SALES_CHANGE_SUBMISSIONS: &str = <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSIONS;
/// `sales_change_submission_line` 集合名。
const SALES_CHANGE_SUBMISSION_LINES: &str =
    <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSION_LINES;

/// 销售变更单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeOrderRow {
    /// 实体主键。
    pub id: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 发起时当前版本。
    pub base_revision_id: String,
    /// 变更类型。
    pub change_type: entities::sales_review::SalesChangeType,
    /// 变更状态。
    pub status: SalesChangeOrderStatus,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 销售变更单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesChangeOrderFilter {
    /// 原销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 变更状态；`None` 表示不筛选。
    pub status: Option<SalesChangeOrderStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesChangeOrderFilter {
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

impl Pagination for SalesChangeOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesChangeOrder> {
    /// 分页检索销售变更单（投影查询）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_sales_change_orders(
        &self,
        filter: &SalesChangeOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesChangeOrderRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_change_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<SalesChangeOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「销售单 + 基准版本」查找进行中变更单。
    ///
    /// # 参数
    /// * `sales_order_id` - 原销售单
    /// * `base_revision_id` - 发起时当前版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回进行中变更单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_in_progress_by_order_and_base(
        &self,
        sales_order_id: &SalesOrderId,
        base_revision_id: &entities::sales_order::SalesOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeOrder>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "base_revision_id": base_revision_id.to_string(),
                "status": {
                    "$in": [
                        SalesChangeOrderStatus::Draft.as_str(),
                        SalesChangeOrderStatus::PendingImpactConfirmation.as_str(),
                        SalesChangeOrderStatus::PendingFinanceReview.as_str(),
                        SalesChangeOrderStatus::Rejected.as_str(),
                    ]
                },
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesChangeSubmission> {
    /// 按「变更单 + 提交序号」查找变更提交。
    ///
    /// # 参数
    /// * `sales_change_order_id` - 所属销售变更单
    /// * `submission_no` - 提交序号
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配的变更提交；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_order_and_no(
        &self,
        sales_change_order_id: &SalesChangeOrderId,
        submission_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeSubmission>> {
        self.find_one(
            doc! {
                "sales_change_order_id": sales_change_order_id.to_string(),
                "submission_no": submission_no as i32,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesChangeSubmissionLine> {
    /// 列出变更提交的全部明细（按行号升序）。
    ///
    /// # 参数
    /// * `submission_id` - 所属变更提交
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按行号升序的变更提交明细列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_lines_by_submission(
        &self,
        submission_id: &SalesChangeSubmissionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesChangeSubmissionLine>> {
        self.find_many_sorted(
            doc! { "sales_change_submission_id": submission_id.to_string() },
            doc! { "line_no": 1 },
            executor,
        )
        .await
    }
}

/// D14 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
pub struct SalesReviewRepository<'a> {
    db: &'a Database,
}

impl<'a> SalesReviewRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 发起销售变更影响确认：写入不可变变更提交并把变更单推进到待影响确认。
    ///
    /// # 参数
    /// * `change_order` - 已迁移到待影响确认的变更单
    /// * `submission` - 不可变变更提交头
    /// * `lines` - 不可变变更提交明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突、乐观锁冲突或 MongoDB 写入失败时返回错误。
    pub async fn submit_sales_change(
        &self,
        change_order: &mut SalesChangeOrder,
        submission: &SalesChangeSubmission,
        lines: &[SalesChangeSubmissionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesChangeSubmission>(SALES_CHANGE_SUBMISSIONS),
            submission,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SalesChangeSubmissionLine>(SALES_CHANGE_SUBMISSION_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        Repository::new(self.db, SALES_CHANGE_ORDERS)
            .update(change_order, executor)
            .await
    }
}

/// 构建排序文档。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { sort_by.unwrap_or("created_at"): direction }
}

/// 销售变更单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_change_order_projection() -> Document {
    doc! {
        "id": 1,
        "sales_order_id": 1,
        "base_revision_id": 1,
        "change_type": 1,
        "status": 1,
        "current_submission_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sales_change_order_filter_applies_order_and_status() {
        let filter = SalesChangeOrderFilter {
            sales_order_id: Some(SalesOrderId::new("o-1")),
            status: Some(SalesChangeOrderStatus::Draft),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("sales_order_id").unwrap(), "o-1");
        assert_eq!(document.get_str("status").unwrap(), "DRAFT");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_descending() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("status"), true), doc! { "status": 1 });
    }
}
