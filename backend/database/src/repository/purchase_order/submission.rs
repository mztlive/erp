//! `purchase_order_submission`(+line) 仓储：财务审核队列列表投影与明细批量取回。
//!
//! 提交是不可变采购内容快照（§6.6）：财务审批与工作任务必须引用具体提交，
//! 不得审批可变采购主表。提交与明细**不提供软删除方法**。

use entities::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SupplierAccountId};
use entities::money::Amount;
use entities::purchase_order::{PurchaseOrderSubmission, PurchaseOrderSubmissionLine, SubmissionStatus};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::common::{in_filter, sort_doc, SUBMISSION_SORT_FIELDS};
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// 采购提交列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionRow {
    /// 实体主键。
    pub id: String,
    /// 所属采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 提交序号（聚合内唯一）。
    pub submission_no: String,
    /// 供应商（拆单维度）。
    pub supplier_id: SupplierAccountId,
    /// 提交状态。
    pub status: SubmissionStatus,
    /// 含税行汇总（Decimal128）。
    pub gross_amount: Amount,
    /// 不含税行汇总（Decimal128）。
    pub net_amount: Amount,
    /// 税额行汇总（Decimal128）。
    pub tax_amount: Amount,
    /// 提交审计时间；与 `submitted_by` 成对出现。
    pub submitted_at: Option<entities::common::time::Instant>,
    /// 提交审计人；与 `submitted_at` 成对出现。
    pub submitted_by: Option<String>,
    /// 乐观锁版本（草稿自动保存并发版本）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

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

impl<'a> Repository<'a, PurchaseOrderSubmission> {
    /// 分页检索采购提交列表（投影查询）。
    ///
    /// 只返回 [`PurchaseOrderSubmissionRow`] 所需的列表字段，不加载整文档
    /// （供应商快照与付款条件门禁快照不进入列表投影）；排序字段经白名单校验
    /// （`created_at`/`submission_no`/`status`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_purchase_order_submissions(
        &self,
        filter: &PurchaseOrderSubmissionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseOrderSubmissionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                SUBMISSION_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(purchase_order_submission_projection())
            .build();
        let collection = self.collection().clone_with_type::<PurchaseOrderSubmissionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

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

/// 采购提交列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn purchase_order_submission_projection() -> Document {
    doc! {
        "id": 1,
        "purchase_order_id": 1,
        "submission_no": 1,
        "supplier_id": 1,
        "status": 1,
        "gross_amount": 1,
        "net_amount": 1,
        "tax_amount": 1,
        "submitted_at": 1,
        "submitted_by": 1,
        "version": 1,
        "created_at": 1,
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
