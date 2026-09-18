//! 客户退款列表筛选/投影与原事实 `$in` 查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, CustomerReceiptId, ReceivableEntryId};
use erp_core::money::Amount;
use mongodb::bson::{Document, doc};
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter,
};
use serde::{Deserialize, Serialize};

use super::search::{ListSort, originals_filter, search_projected};
use crate::entity::returns::{CustomerRefund, CustomerRefundStatus};

/// 客户退款列表投影行。
///
/// 覆盖客户退款 View 所需退款事实，但不包含审批 View 或 Service DTO。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerRefundRow {
    /// 实体主键。
    pub id: String,
    /// 退款状态。
    pub status: CustomerRefundStatus,
    /// 退款单号。
    pub refund_no: String,
    /// 销售退货/拒收处理单。
    pub sales_return_case_id: Option<String>,
    /// 客户。
    pub customer_id: String,
    /// 原回款。
    pub original_receipt_id: Option<String>,
    /// 原应收分录。
    pub original_receivable_entry_id: Option<String>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 退款金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 实际退款时间。
    pub occurred_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户退款列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerRefundFilter {
    /// 退款单号模糊匹配；`None` 表示不筛选。
    pub refund_no: Option<String>,
    /// 客户；`None` 表示不筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 退款状态；`None` 表示不筛选。
    pub status: Option<CustomerRefundStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for CustomerRefundFilter {
    /// 返回首页空筛选（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回筛选为空、降序的首页过滤条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            refund_no: None,
            customer_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for CustomerRefundFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "refund_no", self.refund_no.as_deref());
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for CustomerRefundFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl ListSort for CustomerRefundFilter {
    fn sort_by(&self) -> Option<&str> {
        self.sort_by.as_deref()
    }

    fn sort_ascending(&self) -> bool {
        self.sort_ascending
    }
}

/// 客户退款集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait CustomerRefundRepositoryExt {
    /// 分页检索客户退款列表（投影查询）。
    ///
    /// 只返回 [`CustomerRefundRow`] 所需的列表字段；退款单号支持字面量模糊匹配。
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
    async fn search_customer_refunds(
        &self,
        filter: &CustomerRefundFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerRefundRow>>;

    /// 批量按原事实取回客户退款（`$in`，用于累计冲正校验）。
    ///
    /// # 参数
    /// * `receipt_ids` - 原回款 ID 集合（可为空）
    /// * `entry_ids` - 原应收分录 ID 集合（可为空）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配退款。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_refunds_by_originals(
        &self,
        receipt_ids: &[CustomerReceiptId],
        entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerRefund>>;
}

impl CustomerRefundRepositoryExt for Repository<'_, CustomerRefund> {
    async fn search_customer_refunds(
        &self,
        filter: &CustomerRefundFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerRefundRow>> {
        search_projected(
            self,
            filter,
            customer_refund_projection(),
            &["occurred_at", "amount", "created_at"],
            executor,
        )
        .await
    }

    async fn find_refunds_by_originals(
        &self,
        receipt_ids: &[CustomerReceiptId],
        entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerRefund>> {
        let filter = customer_refund_originals_filter(receipt_ids, entry_ids);
        self.find_many(filter, executor).await
    }
}

/// 保留原事实筛选：两个来源集合都非空时同时匹配，都为空时交给基础仓储查询全部未删除事实。
fn customer_refund_originals_filter(
    receipt_ids: &[CustomerReceiptId],
    entry_ids: &[ReceivableEntryId],
) -> Document {
    originals_filter(receipt_ids, "original_receipt_id", entry_ids, "original_receivable_entry_id")
}

/// 客户退款列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_refund_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "refund_no": 1,
        "sales_return_case_id": 1,
        "customer_id": 1,
        "original_receipt_id": 1,
        "original_receivable_entry_id": 1,
        "reason_code": 1,
        "reason_text": 1,
        "amount": 1,
        "handled_by": 1,
        "reviewed_by": 1,
        "occurred_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::super::search::sort_doc;
    use super::{CustomerRefundFilter, customer_refund_projection};
    use crate::entity::returns::CustomerRefundStatus;

    #[test]
    fn refund_filter_escapes_regex_and_sort_whitelist_falls_back() {
        let filter = CustomerRefundFilter {
            refund_no: Some("RF-1.1".to_string()),
            status: Some(CustomerRefundStatus::Posted),
            sort_by: Some("handled_by".to_string()),
            ..Default::default()
        };

        let document = filter.to_doc();
        let regex = document.get_document("refund_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), r"RF\-1\.1");
        assert_eq!(
            sort_doc(filter.sort_by.as_deref(), false, &["occurred_at", "amount"]),
            doc! { "created_at": -1, "id": -1 }
        );
        assert_eq!(filter.to_doc().get_str("status").unwrap(), "posted");
    }

    #[test]
    fn customer_refund_projection_covers_view_facts_without_approval() {
        let projection = customer_refund_projection();
        for field in [
            "id",
            "status",
            "refund_no",
            "sales_return_case_id",
            "customer_id",
            "original_receipt_id",
            "original_receivable_entry_id",
            "reason_code",
            "reason_text",
            "amount",
            "handled_by",
            "reviewed_by",
            "occurred_at",
            "version",
            "created_at",
        ] {
            assert_eq!(projection.get_i32(field).unwrap(), 1, "{field}");
        }
        assert!(projection.get("approval").is_none());
        assert!(projection.get("evidence_attachment_id").is_none());
    }
}

#[cfg(test)]
mod original_lookup_contract {
    use erp_core::ids::{CustomerReceiptId, ReceivableEntryId};
    use mongodb::bson::{Document, doc};

    use super::customer_refund_originals_filter;

    #[test]
    fn customer_refund_sources_keep_and_and_empty_lookup() {
        let receipts = [CustomerReceiptId::new("receipt-1")];
        let entries = [ReceivableEntryId::new("entry-1")];
        assert_eq!(customer_refund_originals_filter(&[], &[]), Document::new());
        assert_eq!(
            customer_refund_originals_filter(&receipts, &[]),
            doc! {
                "original_receipt_id": { "$in": ["receipt-1"] },
            }
        );
        assert_eq!(
            customer_refund_originals_filter(&[], &entries),
            doc! {
                "original_receivable_entry_id": { "$in": ["entry-1"] },
            }
        );
        assert_eq!(
            customer_refund_originals_filter(&receipts, &entries),
            doc! {
                "original_receipt_id": { "$in": ["receipt-1"] },
                "original_receivable_entry_id": { "$in": ["entry-1"] },
            }
        );
    }
}
