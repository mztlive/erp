use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, PartyId, ReceivableEntryId};
use erp_core::money::Amount;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::sort_doc;
use crate::entity::receivable::{
    CustomerReceipt, CustomerReceiptStatus, PendingReceiptAllocation, ReceiptAllocation,
};
use crate::repository::owned::{CustomerReceiptRepository, ReceiptAllocationRepository};

/// 客户回款单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerReceiptRow {
    /// 实体主键。
    pub id: String,
    /// 回款单状态。
    pub status: CustomerReceiptStatus,
    /// 回款单号。
    pub receipt_no: String,
    /// 实际付款往来主体。
    pub counterparty_party_id: String,
    /// 可选经营归属提示。
    pub customer_id: Option<String>,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 审批提交时冻结的拟核销分配，旧记录缺省为空。
    #[serde(default)]
    pub pending_allocations: Vec<PendingReceiptAllocation>,
}

/// 客户回款单列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerReceiptFilter {
    /// 关键词的完整命中集合；空集合匹配零行，None 不筛选。
    pub keyword_ids: Option<Vec<String>>,
    /// 服务端关联投影解析出的回款单主键集合；`None` 表示不筛选。
    pub receipt_ids: Option<Vec<String>>,
    /// 与已核销回款取并集的本单应收分录；仅供账户范围查询。
    pub pending_entry_ids: Vec<String>,
    /// 回款单号模糊匹配；`None` 表示不筛选。
    pub receipt_no: Option<String>,
    /// 实际付款往来主体；`None` 表示不筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 回款单状态；`None` 表示不筛选。
    pub status: Option<CustomerReceiptStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CustomerReceiptFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(receipt_ids) = &self.receipt_ids {
            if self.pending_entry_ids.is_empty() {
                filter.insert("id", doc! { "$in": receipt_ids });
            } else {
                filter.insert("$or", vec![
                    doc! { "id": { "$in": receipt_ids } },
                    doc! { "pending_allocations.receivable_entry_id": { "$in": &self.pending_entry_ids } },
                ]);
            }
        }
        insert_literal_regex_filter(&mut filter, "receipt_no", self.receipt_no.as_deref());
        if let Some(counterparty_party_id) = &self.counterparty_party_id {
            filter.insert("counterparty_party_id", counterparty_party_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(ids) = &self.keyword_ids {
            let condition = doc! { "id": { "$in": ids } };
            return doc! { "$and": [filter, condition] };
        }
        filter
    }
}

impl Pagination for CustomerReceiptFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> CustomerReceiptRepository<'a> {
    /// 分页检索客户回款单列表（投影查询）。
    ///
    /// 只返回 [`CustomerReceiptRow`] 所需的列表字段；回款单号支持字面量
    /// 模糊匹配（复用 `regex_filter`，禁止自拼正则）。
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
    pub async fn search_customer_receipts(
        &self,
        filter: &CustomerReceiptFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerReceiptRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["received_at", "amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(customer_receipt_projection())
            .build();
        let collection = self.collection().clone_with_type::<CustomerReceiptRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    /// 按回款单号精确查找（单号全局唯一，`uk_customer_receipts_no` 保证）。
    ///
    /// # 参数
    /// * `receipt_no` - 回款单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的回款单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_receipt_no(
        &self,
        receipt_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerReceipt>> {
        self.find_one_by_field("receipt_no", receipt_no, executor).await
    }

    /// 批量按回款单 ID 读取活跃回款事实。
    ///
    /// # 参数
    /// * `receipt_ids` - 回款单 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的回款单；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_receipts_by_ids(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerReceipt>> {
        if receipt_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.find_many(doc! { "id": { "$in": receipt_ids } }, executor).await
    }
}

impl<'a> ReceiptAllocationRepository<'a> {
    /// 批量按回款单集合取回核销分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `receipt_ids` - 回款单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_receipts(
        &self,
        receipt_ids: &[CustomerReceiptId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceiptAllocation>> {
        if receipt_ids.is_empty() {
            return Ok(Vec::new());
        }
        let receipt_ids: Vec<String> = receipt_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "customer_receipt_id": { "$in": receipt_ids } }, executor).await
    }

    /// 批量按应收分录集合取回核销分配（`$in`，用于反向核销锁定）。
    ///
    /// # 参数
    /// * `entry_ids` - 应收分录 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_entries(
        &self,
        entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceiptAllocation>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entry_ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "receivable_entry_id": { "$in": entry_ids } }, executor).await
    }
}

/// 客户回款单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_receipt_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "receipt_no": 1,
        "counterparty_party_id": 1,
        "customer_id": 1,
        "received_at": 1,
        "amount": 1,
        "bank_reference": 1,
        "version": 1,
        "created_at": 1,
        "pending_allocations": 1,
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::CustomerReceiptFilter;

    #[test]
    fn receipt_filter_escapes_regex_literals() {
        let filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: None,
            pending_entry_ids: Vec::new(),
            receipt_no: Some("RC-1.2".to_string()),
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let regex = document.get_document("receipt_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), r"RC\-1\.2");
        assert_eq!(regex.get_str("$options").unwrap(), "i");
    }

    #[test]
    fn scope_filters_with_empty_ids_match_nothing() {
        let receipt_filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: Some(Vec::new()),
            pending_entry_ids: Vec::new(),
            receipt_no: None,
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let receipt_doc = receipt_filter.to_doc();
        assert_eq!(receipt_doc.get_document("id").unwrap().get_array("$in").unwrap().len(), 0);
    }
    #[test]
    fn scope_includes_pending_allocations_and_keeps_structural_filters() {
        let filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: Some(vec!["posted-1".into()]),
            pending_entry_ids: vec!["entry-1".into()],
            receipt_no: None,
            counterparty_party_id: Some(erp_core::ids::PartyId::new("party-1")),
            status: Some(crate::entity::receivable::CustomerReceiptStatus::InApproval),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let document = filter.to_doc();
        let alternatives = document.get_array("$or").unwrap();
        assert_eq!(alternatives.len(), 2);
        assert_eq!(
            alternatives[0].as_document().unwrap(),
            &mongodb::bson::doc! { "id": { "$in": ["posted-1"] } }
        );
        assert_eq!(
            alternatives[1].as_document().unwrap(),
            &mongodb::bson::doc! { "pending_allocations.receivable_entry_id": { "$in": ["entry-1"] } }
        );
        assert_eq!(document.get_str("counterparty_party_id").unwrap(), "party-1");
        assert_eq!(document.get_str("status").unwrap(), "IN_APPROVAL");
        assert!(document.contains_key("deleted_at"));
    }
}

#[cfg(test)]
mod keyword_regression_tests {
    use super::*;
    #[test]
    fn keyword_preserves_structural_scope() {
        let mut filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: None,
            pending_entry_ids: Vec::new(),
            receipt_no: None,
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        filter.keyword_ids = Some(Vec::new());
        let query = filter.to_doc();
        let clauses = query.get_array("$and").unwrap();
        let ids = clauses[1].as_document().unwrap().get_document("id").unwrap().get_array("$in").unwrap();
        assert!(ids.is_empty(), "空关键词命中必须保持零结果");
        assert!(clauses[0].as_document().unwrap().contains_key("deleted_at"));
    }
}
