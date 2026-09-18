use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::SupplierAccountId;
use erp_core::money::Amount;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::sort_doc;
use crate::entity::payable::{SupplierPayment, SupplierPaymentStatus};

/// 供应商付款单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierPaymentRow {
    /// 实体主键。
    pub id: String,
    /// 付款单状态。
    pub status: SupplierPaymentStatus,
    /// 付款单号。
    pub payment_no: String,
    /// 收款供应商。
    pub supplier_id: String,
    /// 实际付款时间（秒级时间戳）。
    pub paid_at: u64,
    /// 含税付款金额。
    pub amount: Amount,
    /// 付款凭证引用。
    pub bank_reference: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商付款单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierPaymentFilter {
    /// 关键词的完整命中集合；空集合匹配零行，None 不筛选。
    pub keyword_ids: Option<Vec<String>>,
    /// 付款单号或供应商名称关键词。
    pub keyword: Option<String>,
    /// 查询层按当前名称解析的供应商主键。
    pub keyword_supplier_ids: Vec<SupplierAccountId>,
    /// 付款单号模糊匹配；`None` 表示不筛选。
    pub payment_no: Option<String>,
    /// 收款供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 付款单状态；`None` 表示不筛选。
    pub status: Option<SupplierPaymentStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for SupplierPaymentFilter {
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
            keyword_supplier_ids: Vec::new(),
            payment_no: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for SupplierPaymentFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "payment_no", self.payment_no.as_deref());
        if let Some(keyword) = self.keyword.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            let mut number_match = Document::new();
            insert_literal_regex_filter(&mut number_match, "payment_no", Some(keyword));
            let mut alternatives = vec![number_match];
            if !self.keyword_supplier_ids.is_empty() {
                alternatives.push(doc! { "supplier_id": { "$in": self.keyword_supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } });
            }
            filter.insert("$or", alternatives);
        }
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
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

impl Pagination for SupplierPaymentFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

#[allow(async_fn_in_trait)]
pub trait SupplierPaymentRepositoryExt {
    /// 按付款单号查询未删除付款单。
    ///
    /// # 参数
    /// * `payment_no` - 付款单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配付款单；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_payment_no(
        &self,
        payment_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierPayment>>;

    /// 按主键集合批量取回供应商付款单（FIN-R02，`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `payment_ids` - 付款单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配付款单；空集合直接返回空列表，不发送空 `$in`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_supplier_payments_by_ids(
        &self,
        payment_ids: &[erp_core::ids::SupplierPaymentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierPayment>>;

    /// 分页检索供应商付款单列表（投影查询）。
    ///
    /// 只返回 [`SupplierPaymentRow`] 所需的列表字段；付款单号支持字面量
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
    async fn search_supplier_payments(
        &self,
        filter: &SupplierPaymentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierPaymentRow>>;
}

impl SupplierPaymentRepositoryExt for persistence_core::Repository<'_, SupplierPayment> {
    async fn find_by_payment_no(
        &self,
        payment_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierPayment>> {
        self.find_one_by_field("payment_no", payment_no, executor).await
    }

    async fn find_supplier_payments_by_ids(
        &self,
        payment_ids: &[erp_core::ids::SupplierPaymentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierPayment>> {
        if payment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = payment_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    async fn search_supplier_payments(
        &self,
        filter: &SupplierPaymentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierPaymentRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["paid_at", "amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_payment_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierPaymentRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}

/// 供应商付款单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_payment_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "payment_no": 1,
        "supplier_id": 1,
        "paid_at": 1,
        "amount": 1,
        "bank_reference": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::SupplierPaymentFilter;

    #[test]
    fn payment_filter_escapes_regex_literals() {
        let filter = SupplierPaymentFilter {
            keyword: Some("狮峰.茶".into()),
            keyword_supplier_ids: vec![erp_core::ids::SupplierAccountId::new("supplier-1")],
            payment_no: Some("PAY-9.9".to_string()),
            ..Default::default()
        };

        let document = filter.to_doc();
        let alternatives = document.get_array("$or").unwrap();
        assert_eq!(alternatives.len(), 2);
        assert_eq!(
            alternatives[0]
                .as_document()
                .unwrap()
                .get_document("payment_no")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"狮峰\.茶"
        );
        assert_eq!(
            alternatives[1]
                .as_document()
                .unwrap()
                .get_document("supplier_id")
                .unwrap()
                .get_array("$in")
                .unwrap()[0]
                .as_str(),
            Some("supplier-1")
        );
        assert!(document.contains_key("deleted_at"));
        let regex = document.get_document("payment_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), r"PAY\-9\.9");
    }
}

#[cfg(test)]
mod keyword_regression_tests {
    use super::*;
    #[test]
    fn keyword_preserves_structural_scope() {
        let mut filter = SupplierPaymentFilter {
            keyword_ids: None,
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: None,
            supplier_id: None,
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
