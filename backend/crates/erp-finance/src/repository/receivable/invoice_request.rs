//! 开票申请查询与额度占用读取。并发申请必须由调用方先写锁应收子账。
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result};

use crate::dto::receivable::invoice_request::InvoiceRequestQuery;
use crate::entity::receivable::SalesInvoiceRequest;
use crate::repository::owned::SalesInvoiceRequestRepository;

/// 经过分页上限约束的申请查询。
struct RequestFilter<'a>(&'a InvoiceRequestQuery);
impl QueryFilter for RequestFilter<'_> {
    fn to_doc(&self) -> Document {
        let mut filter = doc! {"deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON};
        for (field, value) in [
            ("sales_order_id", &self.0.sales_order_id),
            ("customer_id", &self.0.customer_id),
            ("receivable_account_id", &self.0.receivable_account_id),
            ("work_item_id", &self.0.work_item_id),
        ] {
            if let Some(value) = value {
                filter.insert(field, value);
            }
        }
        if let Some(status) = self.0.status {
            filter.insert("status", status.as_str());
        }
        if let Some(q) = self.0.q.as_deref().filter(|q| !q.trim().is_empty()) {
            let mut terms = Vec::new();
            for field in ["request_no", "data.invoice_title", "data.reason"] {
                let mut term = doc! {};
                persistence_core::insert_literal_regex_filter(&mut term, field, Some(q.trim()));
                terms.push(term);
            }
            filter.insert("$or", terms);
        }
        filter
    }
}
impl Pagination for RequestFilter<'_> {
    fn page_and_size(&self) -> (u64, u64) {
        (self.0.page.unwrap_or(1).max(1), self.0.page_size.unwrap_or(20).clamp(1, 100).into())
    }
}
impl SalesInvoiceRequestRepository<'_> {
    /// 查找与财务任务一一关联的申请；缺失表示任务没有申请授权。
    /// # 错误
    /// 仓储读取或反序列化失败时返回错误。
    pub async fn find_for_task(
        &self,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesInvoiceRequest>> {
        self.find_one_by_field("work_item_id", task_id, executor).await
    }

    /// 按相同过滤条件读取列表和总数；固定时间及主键排序。
    /// # 错误
    /// 仓储或分页读取失败时返回错误。
    pub async fn page(
        &self,
        query: &InvoiceRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesInvoiceRequest>> {
        let filter = RequestFilter(query);
        let options = mongodb::options::FindOptions::builder()
            .sort(doc! {"created_at": -1, "id": -1})
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let items =
            persistence_core::mongo_ops::find_many(&self.collection(), filter.to_doc(), options, executor)
                .await?;
        let total =
            persistence_core::mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor)
                .await?;
        Ok(PageResult { items, total: total as i64 })
    }
    /// 查询占用当前应收额度的申请，供同一应收写锁内计算可用额度。
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub async fn reserved_for_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceRequest>> {
        self.find_many(
            doc! {"receivable_account_id": id, "status": {"$in": ["in_approval", "approved"]}},
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::receivable::InvoiceRequestStatus;

    #[test]
    fn text_search_is_literal_or_and_preserves_source_and_status_filters() {
        let query = InvoiceRequestQuery {
            q: Some("  KP.*  ".into()),
            sales_order_id: Some("sale-1".into()),
            customer_id: Some("customer-1".into()),
            status: Some(InvoiceRequestStatus::Approved),
            ..Default::default()
        };
        let filter = RequestFilter(&query).to_doc();
        assert_eq!(filter.get_str("sales_order_id").unwrap(), "sale-1");
        assert_eq!(filter.get_str("customer_id").unwrap(), "customer-1");
        assert_eq!(filter.get_str("status").unwrap(), "approved");
        let terms = filter.get_array("$or").unwrap();
        assert_eq!(terms.len(), 3);
        let text = terms[0].as_document().unwrap().get_document("request_no").unwrap();
        assert_eq!(text.get_str("$regex").unwrap(), r"KP\.\*");
        assert_eq!(text.get_str("$options").unwrap(), "i");
    }
    #[test]
    fn blank_search_has_no_or_and_pagination_is_bounded() {
        let query = InvoiceRequestQuery {
            q: Some(" ".into()),
            page: Some(0),
            page_size: Some(1000),
            ..Default::default()
        };
        let filter = RequestFilter(&query);
        assert!(!filter.to_doc().contains_key("$or"));
        assert_eq!(filter.page_and_size(), (1, 100));
    }
}
