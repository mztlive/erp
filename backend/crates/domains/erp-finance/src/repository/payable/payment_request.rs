//! 付款申请仓储；标准 Repository.update 对申请版本进行 CAS。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::{QueryModel, QueryOutput};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Error, Executor, PageResult, Repository, Result, aggregate, insert_literal_regex_filter, mongo_ops,
};

use crate::dto::payable::payment_request::{PaymentRequestListView, PurchasePaymentRequestQuery};
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
    /// 数据库先执行完整授权，再装载有界候选供组合层排序和分页。
    ///
    /// # 参数
    /// * `query` - 来源、状态和关键词筛选。
    /// * `access` - 同次查询的付款申请完整对象授权凭证。
    /// * `executor` - 当前事务执行器。
    /// # 返回
    /// 稳定倒序最多返回 10001 个候选，额外一行供调用方拒绝超限。
    /// # 错误
    /// 授权模型错配、凭证跨阶段或到期、仓储查询或反序列化失败时返回错误。
    async fn scope_candidates(
        &self,
        query: &PurchasePaymentRequestQuery,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<PurchasePaymentRequest>>;

    /// 在已授权采购上下文中读取该应付关联的有界申请候选。
    /// # 参数
    /// account_id 为上游已验证属于授权采购的应付账户，executor 为同一事务。
    /// # 返回
    /// 最多10001条关联申请；调用方只投影该采购的份额，禁止交付其他来源。
    /// # 错误
    /// 数据库读取或反序列化失败时返回错误。
    async fn linked_for_account(
        &self,
        account_id: &str,
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
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<PurchasePaymentRequest>> {
        let prefix = authorized_request_stages(access, self.collection().name(), executor)?;
        let options = FindOptions::builder().sort(doc! {"created_at": -1, "id": -1}).limit(10_001).build();
        let rows = aggregate::find(&self.collection(), query_doc(query), options, prefix, executor).await?;
        access.ensure_current(executor)?;
        Ok(rows)
    }

    async fn linked_for_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequest>> {
        let query =
            PurchasePaymentRequestQuery { payable_account_id: Some(account_id.into()), ..Default::default() };
        let options = FindOptions::builder().sort(doc! {"created_at": -1, "id": -1}).limit(10_001).build();
        mongo_ops::find_many(&self.collection(), query_doc(&query), options, executor).await
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

fn authorized_request_stages(
    access: &CompiledAccess,
    collection: &str,
    executor: &mut dyn Executor,
) -> crate::Result<Vec<Document>> {
    validate_request_model(access.model(), collection)?;
    access.ensure_current(executor)?;
    Ok(access.stages().to_vec())
}

fn validate_request_model(model: &QueryModel, collection: &str) -> crate::Result<()> {
    model.validate()?;
    if model.resource != "purchase_payment_request"
        || model.collection != collection
        || model.collection != <Database as PurchasePaymentRequestExt>::PURCHASE_PAYMENT_REQUESTS
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(crate::Error::ValidationError("付款申请授权查询模型不匹配".into()));
    }
    Ok(())
}

fn query_doc(query: &PurchasePaymentRequestQuery) -> Document {
    let mut filter = doc! {"deleted_at": NOT_DELETED_TIMESTAMP_BSON};
    for (field, value) in [
        ("supplier_id", &query.supplier_id),
        ("payer_party_id", &query.payer_party_id),
        ("created_by", &query.created_by),
        ("work_item_id", &query.work_item_id),
        ("lines.purchase_order_id", &query.purchase_order_id),
        ("lines.payable_account_id", &query.payable_account_id),
    ] {
        if let Some(value) = value {
            filter.insert(field, value);
        }
    }
    for (field, value) in [
        ("request_no", &query.request_no),
        ("lines.0.source_snapshot.purchase_preview.supplier_name", &query.supplier_name),
        ("payer_party_name", &query.payer_party_name),
        ("lines.source_snapshot.purchase_preview.purchase_no", &query.purchase_order_no),
    ] {
        insert_literal_regex_filter(&mut filter, field, value.as_deref().map(str::trim));
    }
    if let Some(view) = query.view {
        let statuses = match view {
            PaymentRequestListView::Draft => vec!["draft", "rejected", "withdrawn"],
            PaymentRequestListView::Effective => {
                vec!["in_approval", "approved", "partially_paid", "paid", "closed"]
            },
        };
        filter.insert("$and", vec![doc! {"status": {"$in": statuses}}]);
    }
    if let Some(status) = query.status {
        filter.insert("status", status.as_str());
    }
    if let Some(q) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
        let mut terms = Vec::new();
        for field in ["request_no", "data.reason", "payer_party_name"] {
            let mut term = doc! {};
            insert_literal_regex_filter(&mut term, field, Some(q.trim()));
            terms.push(term);
        }
        filter.insert("$or", terms);
    }
    filter
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_identity::entity::authorization::QueryModelKey;
    use erp_identity::testing::compiled_access;
    use persistence_core::ReadExecutor;

    use super::*;
    use crate::entity::payable::PurchasePaymentRequestStatus;

    fn request_model() -> QueryModel {
        QueryModel {
            key: QueryModelKey::parse("payment_requests").unwrap(),
            resource: "purchase_payment_request".into(),
            collection: "purchase_payment_requests".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        }
    }

    #[test]
    fn request_model_requires_the_complete_object_and_exact_target() {
        let valid = request_model();
        validate_request_model(&valid, "purchase_payment_requests").unwrap();
        let mut wrong_resource = valid.clone();
        wrong_resource.resource = "supplier_deposit".into();
        let mut wrong_collection = valid.clone();
        wrong_collection.collection = "supplier_deposit_accounts".into();
        let mut wrong_id = valid.clone();
        wrong_id.id_field = "_id".into();
        let mut share = valid.clone();
        share.output = QueryOutput::Share { fields: ["id".into(), "amount".into()].into() };
        let mut existence = valid.clone();
        existence.output = QueryOutput::Existence { fields: ["id".into()].into() };
        for model in [wrong_resource, wrong_collection, wrong_id, share, existence] {
            model.validate().unwrap();
            let mut executor = ReadExecutor::default();
            let access = compiled_access(model, &executor, None).unwrap();
            assert!(matches!(
                authorized_request_stages(&access, "purchase_payment_requests", &mut executor),
                Err(crate::Error::ValidationError(message)) if message == "付款申请授权查询模型不匹配"
            ));
        }
        assert!(matches!(
            validate_request_model(&valid, "other_collection"),
            Err(crate::Error::ValidationError(_))
        ));
    }

    #[test]
    fn request_model_rejects_malformed_descriptions() {
        let mut model = request_model();
        model.id_field = "nested.$id".into();
        assert!(matches!(
            validate_request_model(&model, "purchase_payment_requests"),
            Err(crate::Error::ValidationError(message)) if message == "授权查询模型非法"
        ));
    }

    #[test]
    fn request_stages_preserve_the_original_prefix_and_reject_stale_or_expired_access() {
        let mut executor = ReadExecutor::default();
        let access = compiled_access(request_model(), &executor, None).unwrap();
        let collection = "purchase_payment_requests";
        assert_eq!(authorized_request_stages(&access, collection, &mut executor).unwrap(), access.stages());
        assert!(matches!(
            authorized_request_stages(&access, collection, &mut ReadExecutor::default()),
            Err(crate::Error::ValidationError(_))
        ));
        executor.next_read();
        assert!(matches!(
            authorized_request_stages(&access, collection, &mut executor),
            Err(crate::Error::ValidationError(_))
        ));
        let expired = compiled_access(request_model(), &executor, Some(Instant::from_unix_secs(1))).unwrap();
        assert!(matches!(
            authorized_request_stages(&expired, collection, &mut executor),
            Err(crate::Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED：授权关系已到期"
        ));
    }

    #[test]
    fn identity_filters_are_exact_and_clearing_removes_them() {
        let query = PurchasePaymentRequestQuery {
            supplier_id: Some(" supplier-1 ".into()),
            payer_party_id: Some(" party-1 ".into()),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let filter = query_doc(&query);
        assert_eq!(filter.get_str("supplier_id").unwrap(), "supplier-1");
        assert_eq!(filter.get_str("payer_party_id").unwrap(), "party-1");
        let cleared = PurchasePaymentRequestQuery {
            supplier_id: Some(" ".into()),
            payer_party_id: Some("".into()),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let filter = query_doc(&cleared);
        assert!(!filter.contains_key("supplier_id"));
        assert!(!filter.contains_key("payer_party_id"));
    }

    #[test]
    fn independent_filters_are_combined_and_regex_characters_are_literal() {
        let query = PurchasePaymentRequestQuery {
            view: Some(PaymentRequestListView::Effective),
            request_no: Some(" FKSQ.1 ".into()),
            supplier_name: Some("供应商+".into()),
            payer_party_name: Some("我方".into()),
            purchase_order_no: Some("CG[1]".into()),
            status: Some(PurchasePaymentRequestStatus::Closed),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let filter = query_doc(&query);
        assert_eq!(filter.get_document("request_no").unwrap(), &doc! {"$regex": r"FKSQ\.1", "$options": "i"});
        assert_eq!(
            filter.get_document("lines.0.source_snapshot.purchase_preview.supplier_name").unwrap(),
            &doc! {"$regex": r"供应商\+", "$options": "i"}
        );
        assert_eq!(
            filter.get_document("payer_party_name").unwrap(),
            &doc! {"$regex": "我方", "$options": "i"}
        );
        assert_eq!(
            filter.get_document("lines.source_snapshot.purchase_preview.purchase_no").unwrap(),
            &doc! {"$regex": r"CG\[1\]", "$options": "i"}
        );
        assert_eq!(filter.get_str("status").unwrap(), "closed");
        let views = filter.get_array("$and").unwrap()[0]
            .as_document()
            .unwrap()
            .get_document("status")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert!(views.contains(&"closed".into()));
        assert!(!views.contains(&"draft".into()));
    }

    #[test]
    fn draft_view_keeps_rejected_and_withdrawn_and_empty_text_has_no_filter() {
        let query = PurchasePaymentRequestQuery {
            view: Some(PaymentRequestListView::Draft),
            supplier_name: Some("  ".into()),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        let filter = query_doc(&query);
        assert_eq!(
            filter.get_array("$and").unwrap(),
            &vec![doc! {"status": {"$in": ["draft", "rejected", "withdrawn"]}}.into()]
        );
        assert!(!filter.contains_key("lines.0.source_snapshot.purchase_preview.supplier_name"));
        assert_eq!(filter.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);
    }
}
