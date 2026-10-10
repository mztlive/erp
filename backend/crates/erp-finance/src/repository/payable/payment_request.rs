//! 付款申请仓储；标准 Repository.update 对申请版本进行 CAS。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Error, Executor, PageResult, Repository, Result, mongo_ops};

use crate::dto::payable::payment_request::PurchasePaymentRequestQuery;
use crate::entity::payable::PurchasePaymentRequest;

/// 付款申请集合访问器，索引与仓储共用此集合常量。
pub trait PurchasePaymentRequestExt {
    const PURCHASE_PAYMENT_REQUESTS: &'static str = "purchase_payment_requests";

    /// 获取申请的标准 CAS 仓储。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回本域申请集合仓储。
    /// # 错误
    /// 不返回错误。
    fn purchase_payment_requests(&self) -> Repository<'_, PurchasePaymentRequest>;
}

impl PurchasePaymentRequestExt for Database {
    fn purchase_payment_requests(&self) -> Repository<'_, PurchasePaymentRequest> {
        Repository::new(self, Self::PURCHASE_PAYMENT_REQUESTS)
    }
}

#[allow(async_fn_in_trait)]
pub trait PurchasePaymentRequestRepositoryExt {
    /// 装载有界完整候选，由组合层按每个真实采购来源授权后分页。
    ///
    /// # 参数
    /// * `query` - 来源、状态和关键词筛选。
    /// * `executor` - 当前事务执行器。
    /// # 返回
    /// 稳定倒序最多返回 10001 个候选，额外一行供调用方拒绝超限。
    /// # 错误
    /// 仓储查询或反序列化失败时返回错误。
    async fn scope_candidates(
        &self,
        query: &PurchasePaymentRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequest>>;

    /// 查找唯一执行任务对应的申请。
    ///
    /// # 参数
    /// * `task_id` - 当前或历史执行任务身份。
    /// * `executor` - 当前执行器。
    /// # 返回
    /// 返回关联申请；没有关联时为空。
    /// # 错误
    /// 仓储读取失败时返回错误。
    async fn find_for_task(
        &self,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchasePaymentRequest>>;

    /// 按当前筛选返回列表和总数，仅供已完成权限范围收敛的单域读取。
    ///
    /// # 参数
    /// * `query` - 筛选与分页条件。
    /// * `executor` - 当前执行器。
    /// # 返回
    /// 返回稳定倒序、分页上限为 100 的列表。
    /// # 错误
    /// 查询、计数或总数转换失败时返回错误。
    async fn page(
        &self,
        query: &PurchasePaymentRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchasePaymentRequest>>;

    /// 读取占用指定应付的全部进行中申请。
    ///
    /// # 参数
    /// * `account_id` - 应付子账身份。
    /// * `executor` - 当前来源额度事务执行器。
    /// # 返回
    /// 返回审批中、待付款或部分付款的申请。
    /// # 错误
    /// 读取失败时返回错误。
    async fn reserved_for_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequest>>;
}

impl PurchasePaymentRequestRepositoryExt for Repository<'_, PurchasePaymentRequest> {
    async fn scope_candidates(
        &self,
        query: &PurchasePaymentRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequest>> {
        let options = FindOptions::builder().sort(doc! {"created_at": -1, "id": -1}).limit(10_001).build();
        mongo_ops::find_many(&self.collection(), query_doc(query), options, executor).await
    }

    async fn find_for_task(
        &self,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchasePaymentRequest>> {
        self.find_one_by_field("work_item_id", task_id, executor).await
    }

    async fn page(
        &self,
        query: &PurchasePaymentRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchasePaymentRequest>> {
        let size = u64::from(query.page_size.unwrap_or(20).clamp(1, 100));
        let skip = query.page.unwrap_or(1).max(1).saturating_sub(1).saturating_mul(size);
        let options = FindOptions::builder()
            .sort(doc! {"created_at": -1, "id": -1})
            .skip(skip)
            .limit(i64::try_from(size).unwrap_or(100))
            .build();
        let filter = query_doc(query);
        let items = mongo_ops::find_many(&self.collection(), filter.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter, executor).await?;
        let total =
            i64::try_from(total).map_err(|_| Error::EntityMetadataOutOfRange("付款申请总数超过范围"))?;
        Ok(PageResult { items, total })
    }

    async fn reserved_for_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequest>> {
        self.find_many(
            doc! {"lines.payable_account_id": account_id,
            "status": {"$in": ["in_approval", "approved", "partially_paid"]}},
            executor,
        )
        .await
    }
}

fn query_doc(query: &PurchasePaymentRequestQuery) -> Document {
    let mut filter = doc! {"deleted_at": NOT_DELETED_TIMESTAMP_BSON};
    for (field, value) in [
        ("supplier_id", &query.supplier_id),
        ("created_by", &query.created_by),
        ("work_item_id", &query.work_item_id),
        ("lines.purchase_order_id", &query.purchase_order_id),
        ("lines.payable_account_id", &query.payable_account_id),
    ] {
        if let Some(value) = value {
            filter.insert(field, value);
        }
    }
    if let Some(status) = query.status {
        filter.insert("status", status.as_str());
    }
    if let Some(q) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
        let mut terms = Vec::new();
        for field in ["request_no", "data.reason", "payer_party_name"] {
            let mut term = doc! {};
            persistence_core::insert_literal_regex_filter(&mut term, field, Some(q.trim()));
            terms.push(term);
        }
        filter.insert("$or", terms);
    }
    filter
}
