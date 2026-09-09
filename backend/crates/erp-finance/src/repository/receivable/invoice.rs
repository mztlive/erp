use crate::entity::receivable::{
    Invoice, InvoiceDirection, InvoiceKind, InvoiceStatus, SalesInvoiceAllocation,
};
use crate::repository::owned::{InvoiceRepository, SalesInvoiceAllocationRepository};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::{stable::StableBase, time::BusinessDate};
use erp_core::ids::{InvoiceId, PartyId, ReceivableAccountId};
use erp_core::money::Amount;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::sort_doc;
use persistence_core::insert_literal_regex_filter;
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};
use persistence_core::{PageResult, Pagination, QueryFilter};

/// 发票列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRow {
    /// 该销项发票消耗的批准申请。
    #[serde(default)]
    pub sales_invoice_request_id: Option<String>,
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<InvoiceStatus>,
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 客户或供应商。
    pub party_id: String,
    /// 发票代码。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: String,
    /// 开票日期（YYYY-MM-DD）。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差。
    pub rounding_adjustment_amount: Amount,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
    /// 红票原蓝票。
    pub original_invoice_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发票列表筛选条件。
#[derive(Debug, Clone)]
pub struct InvoiceFilter {
    /// 关键词的完整命中集合；空集合匹配零行，None 不筛选。
    pub keyword_ids: Option<Vec<String>>,
    /// 服务端关联投影解析出的发票主键集合；`None` 表示不筛选。
    pub invoice_ids: Option<Vec<String>>,
    /// 发票方向；`None` 表示不筛选。
    pub invoice_direction: Option<InvoiceDirection>,
    /// 蓝红类型；`None` 表示不筛选。
    pub invoice_kind: Option<InvoiceKind>,
    /// 客户或供应商；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 发票号码模糊匹配；`None` 表示不筛选。
    pub invoice_no: Option<String>,
    /// 发票状态；`None` 表示不筛选。
    pub status: Option<InvoiceStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for InvoiceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(invoice_ids) = &self.invoice_ids {
            filter.insert("id", doc! { "$in": invoice_ids });
        }
        if let Some(invoice_direction) = self.invoice_direction {
            filter.insert("invoice_direction", invoice_direction.as_str());
        }
        if let Some(invoice_kind) = self.invoice_kind {
            filter.insert("invoice_kind", invoice_kind.as_str());
        }
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "invoice_no", self.invoice_no.as_deref());
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

impl Pagination for InvoiceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> InvoiceRepository<'a> {
    /// 分页检索发票列表（投影查询）。
    ///
    /// 只返回 [`InvoiceRow`] 所需的列表字段；发票号码支持字面量模糊匹配。
    /// D19 的进项发票查询经本方法按 `invoice_direction = Purchase` 复用。
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
    pub async fn search_invoices(
        &self,
        filter: &InvoiceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<InvoiceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["invoice_date", "gross_amount", "net_amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(invoice_projection())
            .build();
        let collection = self.collection().clone_with_type::<InvoiceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「方向 + 规范化号码」查找发票（无代码数电票唯一键）。
    ///
    /// 唯一性由 `uk_invoices_uncoded` 部分唯一索引保证（有代码发票走
    /// `uk_invoices_coded`）；本方法用于登记前幂等判定与 D19 进项发票引用，
    /// 服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `invoice_direction` - 发票方向
    /// * `normalized_no` - 规范化发票号码（去空白转大写）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的发票；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_direction_and_normalized_no(
        &self,
        invoice_direction: InvoiceDirection,
        normalized_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Invoice>> {
        self.find_one(
            doc! {
                "invoice_direction": invoice_direction.as_str(),
                "normalized_no": normalized_no,
            },
            executor,
        )
        .await
    }

    /// 批量按发票 ID 读取活跃发票事实。
    ///
    /// # 参数
    /// * `invoice_ids` - 发票 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的发票；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_invoices_by_ids(
        &self,
        invoice_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Invoice>> {
        if invoice_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.find_many(doc! { "id": { "$in": invoice_ids } }, executor)
            .await
    }
}

impl<'a> SalesInvoiceAllocationRepository<'a> {
    /// 批量按发票集合取回销项发票分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `invoice_ids` - 发票 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_invoices(
        &self,
        invoice_ids: &[InvoiceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceAllocation>> {
        if invoice_ids.is_empty() {
            return Ok(Vec::new());
        }
        let invoice_ids: Vec<String> = invoice_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "invoice_id": { "$in": invoice_ids } }, executor)
            .await
    }

    /// 批量按应收子账集合取回销项发票分配（`$in`，用于开票进度校验）。
    ///
    /// # 参数
    /// * `account_ids` - 应收往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceAllocation>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "receivable_account_id": { "$in": account_ids } }, executor)
            .await
    }
}

/// 发票列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn invoice_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "invoice_direction": 1,
        "invoice_kind": 1,
        "party_id": 1,
        "invoice_code": 1,
        "invoice_no": 1,
        "invoice_date": 1,
        "gross_amount": 1,
        "net_amount": 1,
        "tax_amount": 1,
        "rounding_adjustment_amount": 1,
        "rounding_reason": 1,
        "sales_invoice_request_id": 1,
        "original_invoice_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::InvoiceFilter;
    use persistence_core::QueryFilter;

    #[test]
    fn scope_filters_with_empty_ids_match_nothing() {
        let invoice_filter = InvoiceFilter {
            keyword_ids: None,
            invoice_ids: Some(Vec::new()),
            invoice_direction: None,
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let invoice_doc = invoice_filter.to_doc();
        assert_eq!(
            invoice_doc
                .get_document("id")
                .unwrap()
                .get_array("$in")
                .unwrap()
                .len(),
            0
        );
    }
}

#[cfg(test)]
mod keyword_regression_tests {
    use super::*;
    #[test]
    fn keyword_preserves_structural_scope() {
        let mut filter = InvoiceFilter {
            keyword_ids: None,
            invoice_ids: None,
            invoice_direction: None,
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        filter.keyword_ids = Some(Vec::new());
        let query = filter.to_doc();
        let clauses = query.get_array("$and").unwrap();
        let ids = clauses[1]
            .as_document()
            .unwrap()
            .get_document("id")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert!(ids.is_empty(), "空关键词命中必须保持零结果");
        assert!(clauses[0].as_document().unwrap().contains_key("deleted_at"));
    }
}
