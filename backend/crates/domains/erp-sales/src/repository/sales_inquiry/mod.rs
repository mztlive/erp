//! 询价列表及来源反查，所有查询使用调用方提供的 Executor。
mod scope;
use mongodb::Database;
use mongodb::bson::{Document, doc, serialize_to_vec};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Result, insert_literal_regex_filter, mongo_ops};
pub use scope::{InquiryReadScope, InquiryScopeClause};

use crate::entity::sales_inquiry::{InquiryStatus, SalesInquiry};
use crate::repository::SalesInquiryExt;
use crate::{Error, Result as SalesResult};

/// 授权范围须由调用方解析，默认不返回任何询价。
#[derive(Debug, Clone)]
pub struct InquiryListFilter {
    pub access_scope: InquiryReadScope,
    pub owner_user_ids: Option<Vec<String>>,
    pub business_org_unit_ids: Option<Vec<String>>,
    pub inquiry_ids: Option<Vec<String>>,
    pub customer_id: Option<String>,
    pub status: Option<InquiryStatus>,
    pub q: Option<String>,
    pub page: u64,
    pub page_size: u32,
}

impl Default for InquiryListFilter {
    fn default() -> Self {
        Self {
            access_scope: InquiryReadScope::default(),
            owner_user_ids: None,
            business_org_unit_ids: None,
            inquiry_ids: None,
            customer_id: None,
            status: None,
            q: None,
            page: 1,
            page_size: 20,
        }
    }
}

impl InquiryListFilter {
    /// 组装授权与业务筛选的交集，禁止关键字覆盖责任范围。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回未删除、授权及查询条件交集。
    /// # 错误
    /// 无。
    pub fn document(&self) -> Document {
        let mut parts = vec![self.access_scope.document(), doc! { "deleted_at": 0_i64 }];
        if let Some(ids) = &self.owner_user_ids {
            parts.push(doc! { "sales_owner_user_id": { "$in": ids } });
        }
        if let Some(ids) = &self.business_org_unit_ids {
            parts.push(doc! { "business_org_unit_id": { "$in": ids } });
        }
        if let Some(ids) = &self.inquiry_ids {
            parts.push(doc! { "id": { "$in": ids } });
        }
        if let Some(id) = &self.customer_id {
            parts.push(doc! { "customer_id": id });
        }
        if let Some(status) = self.status {
            if matches!(status, InquiryStatus::AwaitingProcurement | InquiryStatus::ProcurementProcessing) {
                parts.push(doc! { "status": { "$in": ["awaiting_procurement", "procurement_processing"] } });
            } else {
                parts.push(doc! { "status": status_string(status) });
            }
        }
        if let Some(q) = self.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            let mut no = Document::new();
            let mut customer = Document::new();
            insert_literal_regex_filter(&mut no, "inquiry_no", Some(q));
            insert_literal_regex_filter(&mut customer, "customer_name", Some(q));
            parts.push(doc! { "$or": [no, customer] });
        }
        doc! { "$and": parts }
    }
}

/// 保持与领域 serde 的 snake_case 状态一致。
fn status_string(status: InquiryStatus) -> &'static str {
    match status {
        InquiryStatus::Draft => "draft",
        InquiryStatus::AwaitingProcurement => "awaiting_procurement",
        InquiryStatus::ProcurementProcessing => "procurement_processing",
        InquiryStatus::AwaitingSalesSupplement => "awaiting_sales_supplement",
        InquiryStatus::AwaitingSales => "awaiting_sales",
        InquiryStatus::Confirmed => "confirmed",
        InquiryStatus::Closed => "closed",
    }
}

/// 本域分页仓储不推断任何访问资格。
pub struct SalesInquiryRepository<'a> {
    db: &'a Database,
}

impl<'a> SalesInquiryRepository<'a> {
    /// 绑定本域数据库。
    /// # 参数
    /// `db` 为目标数据库。
    /// # 返回
    /// 返回仓储。
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 创建有界询价聚合，不截断冻结历史。
    /// # 参数
    /// `inquiry` 为已验证聚合；`executor` 为同一事务。
    /// # 返回
    /// 创建成功时返回空结果。
    /// # 错误
    /// 序列化后超过 14 MiB 或数据库写入失败时拒绝。
    pub async fn create(&self, inquiry: &SalesInquiry, executor: &mut dyn Executor) -> SalesResult<()> {
        ensure_document_size(inquiry)?;
        self.db.sales_inquiries().create(inquiry, executor).await?;
        Ok(())
    }

    /// 按版本条件更新有界询价聚合。
    /// # 参数
    /// `inquiry` 为同执行器内读取后修改的聚合。
    /// # 返回
    /// 更新成功时同步内存中的版本。
    /// # 错误
    /// 版本过期、聚合超过 14 MiB 或数据库失败时拒绝。
    pub async fn update(&self, inquiry: &mut SalesInquiry, executor: &mut dyn Executor) -> SalesResult<()> {
        ensure_document_size(inquiry)?;
        self.db.sales_inquiries().update(inquiry, executor).await?;
        Ok(())
    }

    /// 按已授权范围稳定分页。
    /// # 参数
    /// `filter` 包含授权文档及分页；`executor` 为事务执行器。
    /// # 返回
    /// 返回当前页和使用同一条件的总数。
    /// # 错误
    /// 查询失败时返回存储错误。
    pub async fn inquiry_list(
        &self,
        filter: &InquiryListFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesInquiry>> {
        let document = filter.document();
        let size = u64::from(filter.page_size.clamp(1, 100));
        let skip = filter.page.max(1).saturating_sub(1).saturating_mul(size);
        let collection = self.db.sales_inquiries().collection();
        let items = mongo_ops::find_many(
            &collection,
            document.clone(),
            FindOptions::builder()
                .sort(doc! { "created_at": -1, "id": -1 })
                .skip(skip)
                .limit(i64::try_from(size).unwrap_or(100))
                .build(),
            executor,
        )
        .await?;
        let total = mongo_ops::count_documents(&collection, document, executor).await?;
        Ok(PageResult { items, total: i64::try_from(total).unwrap_or(i64::MAX) })
    }

    /// 按有界稳定身份集合读取询价，不从历史任务推断访问权。
    /// # 参数
    /// `ids` 为调用方当前任务或来源证明的身份；`executor` 为同一执行器。
    /// # 返回
    /// 返回全部匹配的未删除聚合，空输入不查库。
    /// # 错误
    /// 超过 10000 个身份或数据库读取失败时拒绝。
    pub async fn by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> SalesResult<Vec<SalesInquiry>> {
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("询价批量读取最多 10000 个身份".into()));
        }
        Ok(self.db.sales_inquiries().list_active_by_ids(ids, executor).await?)
    }

    /// 查询同一费用单据关联的所有询价；单据费用最多 50 行。
    /// # 参数
    /// `source_document_id` 是采购或财务事实身份；`executor` 为同一事务。
    /// # 返回
    /// 返回关联聚合，含已释放来源供幂等核对。
    /// # 错误
    /// 查询失败时返回存储错误。
    pub async fn fee_sources(
        &self,
        source_document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInquiry>> {
        mongo_ops::find_many(
            &self.db.sales_inquiries().collection(),
            doc! { "fee_ledger.source_document_id": source_document_id, "deleted_at": 0_i64 },
            FindOptions::builder().sort(doc! { "id": 1 }).limit(50).build(),
            executor,
        )
        .await
    }
}

/// 留出 MongoDB 文档元数据余量，不截断任何已发布历史。
fn ensure_document_size(inquiry: &SalesInquiry) -> SalesResult<()> {
    let bytes = serialize_to_vec(inquiry).map_err(|_| Error::Internal("询价资料无法持久化".into()))?;
    if bytes.len() > 14 * 1024 * 1024 {
        return Err(Error::BusinessLogicError("询价历史已达到容量限制，请复制为新询价继续处理".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn processing_filter_includes_legacy_assignments_without_expanding_scope() {
        for status in [InquiryStatus::AwaitingProcurement, InquiryStatus::ProcurementProcessing] {
            let filter = InquiryListFilter { status: Some(status), ..Default::default() };
            let query = filter.document();
            let parts = query.get_array("$and").unwrap();
            assert_eq!(parts[0].as_document().unwrap(), &filter.access_scope.document());
            assert_eq!(
                parts.last().unwrap().as_document().unwrap(),
                &doc! { "status": { "$in": ["awaiting_procurement", "procurement_processing"] } }
            );
        }
        let filter = InquiryListFilter { status: Some(InquiryStatus::AwaitingSales), ..Default::default() };
        let query = filter.document();
        assert_eq!(
            query.get_array("$and").unwrap().last().unwrap().as_document().unwrap(),
            &doc! { "status": "awaiting_sales" }
        );
    }

    #[test]
    fn default_scope_denies_and_search_stays_in_intersection() {
        let denied = InquiryListFilter::default().document();
        assert_eq!(
            denied.get_array("$and").unwrap()[0].as_document().unwrap(),
            &doc! { "id": { "$in": [] } }
        );
        let filter = InquiryListFilter {
            access_scope: InquiryReadScope {
                clauses: vec![InquiryScopeClause {
                    owner_user_id: Some("u".into()),
                    business_org_unit_ids: vec!["o".into()],
                    company: false,
                }],
                limit: None,
            },
            q: Some("SO.[1]".into()),
            customer_id: Some("c".into()),
            ..Default::default()
        };
        let query = filter.document();
        let parts = query.get_array("$and").unwrap();
        assert_eq!(parts[0].as_document().unwrap(), &filter.access_scope.document());
        assert_eq!(parts[2].as_document().unwrap().get_str("customer_id").unwrap(), "c");
        let search = parts[3].as_document().unwrap().get_array("$or").unwrap();
        assert_eq!(
            search[0].as_document().unwrap().get_document("inquiry_no").unwrap().get_str("$regex").unwrap(),
            r"SO\.\[1\]"
        );
    }
}
