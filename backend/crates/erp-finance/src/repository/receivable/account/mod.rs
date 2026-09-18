use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::stable::StableBase;
use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId, SalesOrderId};
use erp_core::money::Amount;
use mongodb::bson::{Document, doc};
use persistence_core::{Pagination, QueryFilter, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use crate::entity::receivable::ReceivableAccountStatus;

mod invoicing;
mod query;
mod settlement;
mod write;

pub use invoicing::ReceivableAccountInvoicingExt;
pub use query::ReceivableAccountRepositoryExt;
pub use settlement::ReceivableAccountSettlementExt;
pub(crate) use write::ReceivableAccountWriteExt;
pub(super) use write::{amount_bson, progress_pipeline};

#[cfg(test)]
mod tests;

/// 应收往来子账列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivableAccountRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<ReceivableAccountStatus>,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 企业客户经营归属。
    pub customer_id: String,
    /// 收款和开票往来主体。
    pub counterparty_party_id: String,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额。
    pub open_total: Amount,
    /// 可开票含税总额。
    pub invoiceable_total: Amount,
    /// 净已开含税总额。
    pub invoiced_total: Amount,
    /// 剩余可开票含税额度。
    pub open_invoiceable_total: Amount,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 批量条件开票结果：按账户逐个报告命中情况，由 Service 转译业务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoicingBatchResult {
    /// 条件命中并完成开票的账户（输入顺序）。
    pub applied: Vec<ReceivableAccountId>,
    /// 条件未命中（超过剩余可开票额度）被拒绝的账户（输入顺序）。
    pub rejected: Vec<ReceivableAccountId>,
}

/// 批量条件核销结果：按账户逐个报告命中情况，由 Service 转译业务错误。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SettlementBatchResult {
    /// 条件命中并完成核销的账户（输入顺序）。
    pub applied: Vec<ReceivableAccountId>,
    /// 条件未命中（超过剩余开放余额）被拒绝的账户（输入顺序）。
    pub rejected: Vec<ReceivableAccountId>,
}

/// 应收往来子账列表筛选条件。
#[derive(Debug, Clone)]
pub struct ReceivableAccountFilter {
    /// 关键词的完整命中集合；空集合匹配零行，None 不筛选。
    pub keyword_ids: Option<Vec<String>>,
    /// 主键、销售单、客户或往来主体关键字；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 关键词命中的来源销售单身份；由组合查询解析。
    pub keyword_sales_order_ids: Vec<SalesOrderId>,
    /// 关键词命中的往来主体身份；由组合查询解析。
    pub keyword_party_ids: Vec<PartyId>,
    /// 子账主键；`None` 表示不筛选。
    pub account_id: Option<ReceivableAccountId>,
    /// 企业客户经营归属；`None` 表示不筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 收款和开票往来主体；`None` 表示不筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 子账状态；`None` 表示不筛选。
    pub status: Option<ReceivableAccountStatus>,
    /// 来源销售单；`None` 表示不筛选。
    pub sales_order_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for ReceivableAccountFilter {
    /// 缺省分页从第一页、每页二十条开始，其余筛选保持空条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回第 1 页、每页 20 条的空筛选条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            keyword_ids: None,
            keyword: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_party_ids: Vec::new(),
            account_id: None,
            customer_id: None,
            counterparty_party_id: None,
            status: None,
            sales_order_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for ReceivableAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(keyword) = self.keyword.as_deref() {
            let mut alternatives = ["id", "sales_order_id", "customer_id", "counterparty_party_id"]
                .into_iter()
                .map(|field| {
                    let mut alternative = Document::new();
                    insert_literal_regex_filter(&mut alternative, field, Some(keyword));
                    alternative
                })
                .collect::<Vec<_>>();
            if !self.keyword_sales_order_ids.is_empty() {
                alternatives.push(doc! { "sales_order_id": { "$in": self.keyword_sales_order_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } });
            }
            if !self.keyword_party_ids.is_empty() {
                alternatives.push(doc! { "counterparty_party_id": { "$in": self.keyword_party_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } });
            }
            filter.insert("$or", alternatives);
        }
        if let Some(account_id) = &self.account_id {
            filter.insert("id", account_id.to_string());
        }
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id.to_string());
        }
        if let Some(counterparty_party_id) = &self.counterparty_party_id {
            filter.insert("counterparty_party_id", counterparty_party_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(ids) = &self.keyword_ids {
            let condition = doc! { "id": { "$in": ids } };
            return doc! { "$and": [filter, condition] };
        }
        filter
    }
}

impl Pagination for ReceivableAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

#[cfg(test)]
mod keyword_regression_tests {
    use super::*;
    #[test]
    fn keyword_preserves_structural_scope() {
        let mut filter = ReceivableAccountFilter {
            keyword_ids: None,
            keyword: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_party_ids: Vec::new(),
            account_id: None,
            customer_id: None,
            counterparty_party_id: None,
            status: None,
            sales_order_id: None,
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
