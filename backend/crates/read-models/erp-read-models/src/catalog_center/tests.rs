use std::sync::Mutex;

use async_trait::async_trait;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};
use erp_catalog::service::catalog::prepare_product_list;
use erp_catalog::{EnableStatus, ProductKind, ProductListingStatus, ProductView};
use erp_core::AccountKind;
use erp_core::common::time::BusinessDate;
use erp_identity::testing::unavailable_authorization;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, PageResult, ReadExecutor};
use serde_json::json;

use super::*;

#[test]
fn product_maintainer_name_uses_matching_account() {
    let mut rows = vec![ProductView::from(sample_row())];
    let names = [("user-1".into(), "张三".into()), ("user-2".into(), "李四".into())].into();
    scope::apply_maintainer_names(&mut rows, &names);
    let value = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(value["maintainer_user_id"], "user-1");
    assert_eq!(value["maintainer_user_name"], "张三");
}

#[test]
fn product_maintainer_name_is_null_when_account_is_missing() {
    let mut rows = vec![ProductView::from(sample_row())];
    let names = [("user-2".into(), "李四".into())].into();
    scope::apply_maintainer_names(&mut rows, &names);
    let value = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(value["maintainer_user_id"], "user-1");
    assert!(value["maintainer_user_name"].is_null());
}

fn sample_row() -> ProductRow {
    ProductRow {
        id: "product-1".into(),
        product_no: "P-1".into(),
        product_kind: ProductKind::Physical,
        name: None,
        category_id: None,
        brand_id: Some("brand-1".into()),
        maintainer_user_id: "user-1".into(),
        business_org_unit_id: "org-1".into(),
        status: EnableStatus::Active,
        listing_status: ProductListingStatus::PartiallyListed,
        listed_sku_count: 1,
        sku_count: 3,
        supplied_sku_count: 2,
        priced_sku_count: 1,
        source_diff_count: 0,
        source_diff_fields: Vec::new(),
        current_revision_id: None,
        version: 7,
        created_at: 123,
    }
}

#[derive(Default)]
struct RecordingQuery {
    calls: Mutex<Vec<&'static str>>,
    fail: bool,
    candidates: Option<Vec<String>>,
    candidate_filter: Mutex<Option<ProductFilter>>,
}

#[async_trait]
impl CatalogSupplyQueryPort for RecordingQuery {
    /// 记录生产责任筛选传入的完整条件，返回预设的候选身份。
    ///
    /// # 参数
    /// * `filter` - 完整商品筛选；本替身克隆后记入 `candidate_filter`，不按页码收窄。
    /// * `_` - 调用方执行器；本替身不读取。
    ///
    /// # 返回
    /// `fail` 为假时返回预设 `candidates` 的克隆。
    ///
    /// # 错误
    /// `fail` 为真时，仍先记下调用与 `filter`，再返回 `persistence_core::Error::OptimisticLockingError`。
    ///
    /// # Panics
    /// `candidates` 为 `None` 时 `expect` 失败，表示普通商品查询不得读取采购候选。观测 `Mutex` 中毒时 `unwrap` 也会 panic。
    async fn product_candidate_ids(
        &self,
        filter: &ProductFilter,
        _: &mut dyn Executor,
    ) -> erp_catalog::Result<Vec<String>> {
        self.calls.lock().unwrap().push("candidates");
        *self.candidate_filter.lock().unwrap() = Some(filter.clone());
        if self.fail {
            return Err(persistence_core::Error::OptimisticLockingError.into());
        }
        Ok(self.candidates.clone().expect("ordinary product query must not read procurement candidates"))
    }
    async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<PageResult<ProductRow>> {
        self.calls.lock().unwrap().push("product");
        assert!(executor.session().is_none());
        if self.fail {
            return Err(persistence_core::Error::OptimisticLockingError.into());
        }
        if let Some(ids) = filter.ids.as_deref() {
            assert_eq!((filter.page, filter.page_size), (1, 1));
            if ids == ["missing"] {
                return Ok(PageResult { items: vec![], total: 0 });
            }
            let mut row = sample_row();
            row.name = Some("礼盒".into());
            row.category_id = Some("cat-1".into());
            row.current_revision_id = Some("rev-1".into());
            return Ok(PageResult { items: vec![row], total: 1 });
        }
        assert_eq!(filter.product_no.as_deref(), Some("P-1"));
        Ok(PageResult { items: vec![sample_row()], total: 11 })
    }

    async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<PageResult<SellableSkuRow>> {
        self.calls.lock().unwrap().push("sellable");
        assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
        if self.fail {
            return Err(persistence_core::Error::OptimisticLockingError);
        }
        assert_eq!(filter.keyword.as_deref(), Some("茶礼"));
        assert!(filter.supplier_id.is_none());
        assert_eq!(filter.eligibility_as_of.to_string(), "2026-09-07");
        Ok(PageResult { items: vec![], total: 9 })
    }

    async fn find_sellable_sku_refs(
        &self,
        _: &[(String, String)],
        _: BusinessDate,
        _: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<SellableSkuRow>> {
        panic!("list entry points must not execute the exact-reference query")
    }
    async fn find_sellable_skus_by_ids(
        &self,
        _: &[String],
        _: BusinessDate,
        _: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<SellableSkuRow>> {
        panic!("list entry points must not execute the id-only query")
    }
}

#[tokio::test]
async fn invalid_product_page_stops_before_any_query() {
    let query = Arc::new(RecordingQuery::default());
    let params = serde_json::from_value(json!({"page": 0})).unwrap();
    assert!(prepare_product_list(&params).is_err());
    assert!(query.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn product_projection_preserves_optional_fields_counts_and_page() {
    let query = Arc::new(RecordingQuery::default());
    let params = serde_json::from_value(json!({"product_no":" P-1 ","page":2,"page_size":3})).unwrap();
    let filter = prepare_product_list(&params).unwrap();
    let page = query.product_page(&filter, &mut NoTransaction).await.unwrap();

    assert_eq!((filter.page, filter.page_size, page.total), (2, 3, 11));
    let row = &page.items[0];
    assert_eq!(row.maintainer_user_id, "user-1");
    assert_eq!(row.business_org_unit_id, "org-1");
    assert_eq!(*query.calls.lock().unwrap(), vec!["product"]);
}

#[tokio::test]
async fn sellable_query_keeps_normalized_filters_explicit_date_and_default_page() {
    let query = Arc::new(RecordingQuery::default());
    let params =
        serde_json::from_value(json!({"q":" 茶礼 ","supplier_id":" ","eligibility_as_of":"2026-09-07"}))
            .unwrap();
    let mut executor = SnapshotExecutor::default();
    let boundary = executor.boundary().unwrap();
    let (page, supplier_ids) = sellable_sku_page(query.as_ref(), &params, &mut executor).await.unwrap();

    assert!(supplier_ids.is_empty());
    assert!(boundary.is_current(&executor));
    assert_eq!((page.page, page.page_size, page.total), (1, 20, 9));
    assert_eq!(*query.calls.lock().unwrap(), vec!["sellable"]);
}

/// 不可用数据库只用于证明拒绝路径不会开始商品或供应商读取。
async fn sellable_service(query: Arc<RecordingQuery>) -> CatalogCenterReadService {
    let client =
        mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
    let db = client.database("sellable_snapshot_contract");
    let authorization = unavailable_authorization();
    CatalogCenterReadService::new(
        db.clone(),
        query,
        CatalogAccess::new(db, authorization.clone()),
        Arc::new(MapProductProcurementOwners { owners: Default::default() }),
        authorization,
    )
}

#[tokio::test]
async fn sellable_snapshot_rejects_non_snapshot_before_authorization_or_business_reading() {
    let query = Arc::new(RecordingQuery::default());
    let service = sellable_service(query.clone()).await;
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

    let result = service
        .sellable_sku_list_with(&SellableSkuListParams::default(), &actor, &mut ReadExecutor::default())
        .await;
    assert!(
        matches!(result, Err(Error::ValidationError(reason)) if reason == "暂时无法查询可售商品，请从商品列表重新查询")
    );
    assert!(query.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sellable_snapshot_stops_before_business_reading_when_authorization_fails() {
    let query = Arc::new(RecordingQuery::default());
    let service = sellable_service(query.clone()).await;
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

    let result = service
        .sellable_sku_list_with(&SellableSkuListParams::default(), &actor, &mut SnapshotExecutor::default())
        .await;
    assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    assert!(query.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sellable_page_propagates_business_query_failure() {
    let query = RecordingQuery { fail: true, ..Default::default() };
    let result =
        sellable_sku_page(&query, &SellableSkuListParams::default(), &mut SnapshotExecutor::default()).await;
    assert!(
        matches!(result, Err(Error::ConflictError(reason)) if reason == "数据已被其他请求修改，请刷新后重试")
    );
    assert_eq!(*query.calls.lock().unwrap(), vec!["sellable"]);
}

#[tokio::test]
async fn procurement_owner_filter_does_not_expand_maintainer_authorization() {
    let port = MapProductProcurementOwners {
        owners: [("prod-a".into(), "buyer-1".into()), ("prod-b".into(), "buyer-2".into())].into(),
    };
    let matched = port
        .matching_product_ids(
            Some(&["prod-a".into(), "prod-b".into()]),
            &["buyer-1".into()],
            &mut NoTransaction,
        )
        .await
        .unwrap();
    assert_eq!(matched, vec!["prod-a".to_string()]);
    let none = port
        .matching_product_ids(Some(&["prod-b".into()]), &["buyer-1".into()], &mut NoTransaction)
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn product_list_rejects_unknown_owner_alias() {
    assert!(serde_json::from_value::<erp_catalog::ProductListParams>(json!({"owner": "张三"})).is_err());
}

#[tokio::test]
/// 实际编排先传入完整筛选，再在全部候选内按采购负责人收窄，不依赖目标页。
async fn procurement_candidates_receive_complete_filters_and_preserve_paging() {
    let query =
        RecordingQuery { candidates: Some(vec!["prod-a".into(), "prod-b".into()]), ..Default::default() };
    let port = MapProductProcurementOwners {
        owners: [
            ("prod-a".into(), "buyer-2".into()),
            ("prod-b".into(), "buyer-1".into()),
            ("outside".into(), "buyer-1".into()),
        ]
        .into(),
    };
    let params: erp_catalog::ProductListParams = serde_json::from_value(json!({
        "product_no": " P-1 ", "keyword": " 礼盒.* ", "owner_user_ids": "maintainer",
        "org_unit_ids": "org", "procurement_owner_user_ids": "buyer-1", "page": 7,
        "page_size": 1
    }))
    .unwrap();
    let mut filter = prepare_product_list(&params).unwrap();

    scope::apply_procurement_filter(&query, &mut filter, &port, &params, &mut NoTransaction).await.unwrap();
    assert_eq!(filter.ids, Some(vec!["prod-b".to_string()]));
    assert_eq!((filter.page, filter.page_size), (7, 1));

    let recorded = query.candidate_filter.lock().unwrap();
    let recorded = recorded.as_ref().unwrap();
    assert_eq!(recorded.keyword.as_deref(), Some("礼盒.*"));
    assert_eq!(recorded.product_no.as_deref(), Some("P-1"));
    assert_eq!(recorded.maintainer_user_ids.as_deref(), Some(["maintainer".to_string()].as_slice()));
    assert_eq!(recorded.business_org_unit_ids.as_deref(), Some(["org".to_string()].as_slice()));
    assert_eq!(*query.calls.lock().unwrap(), vec!["candidates"]);
}

#[tokio::test]
/// 空候选保持显式零结果；候选超限与读取失败在责任解析前整体拒绝。
async fn procurement_candidates_keep_empty_results_and_reject_invalid_snapshots() {
    let params = serde_json::from_value(json!({"procurement_owner_user_ids": "buyer"})).unwrap();
    let port = MapProductProcurementOwners { owners: [("outside".into(), "buyer".into())].into() };
    let mut filter = prepare_product_list(&params).unwrap();

    let empty = RecordingQuery { candidates: Some(Vec::new()), ..Default::default() };
    scope::apply_procurement_filter(&empty, &mut filter, &port, &params, &mut NoTransaction).await.unwrap();
    assert_eq!(filter.ids, Some(Vec::new()));

    let large = RecordingQuery { candidates: Some(vec!["id".into(); 10001]), ..Default::default() };
    assert!(matches!(
        scope::apply_procurement_filter(&large, &mut filter, &port, &params, &mut NoTransaction).await,
        Err(crate::Error::ValidationError(_))
    ));

    let failing = RecordingQuery { fail: true, ..Default::default() };
    assert!(
        scope::apply_procurement_filter(&failing, &mut filter, &port, &params, &mut NoTransaction)
            .await
            .is_err()
    );
}
