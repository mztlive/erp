use super::*;
use async_trait::async_trait;
use erp_catalog::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};
use erp_catalog::{EnableStatus, ProductKind, ProductListingStatus};
use erp_core::common::time::BusinessDate;
use persistence_core::{Executor, PageResult};
use serde_json::json;
use std::sync::Mutex;

#[derive(Default)]
struct RecordingQuery {
    calls: Mutex<Vec<&'static str>>,
    fail: bool,
}

#[async_trait]
impl CatalogSupplyQueryPort for RecordingQuery {
    async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<PageResult<ProductRow>> {
        self.calls.lock().unwrap().push("product");
        assert!(executor.session().is_none());
        if self.fail {
            return Err(persistence_core::Error::OptimisticLockingError);
        }
        // 详情单查只加主键过滤，与列表共用同一聚合管道。
        if let Some(ids) = filter.ids.as_deref() {
            assert_eq!((filter.page, filter.page_size), (1, 1));
            assert!(filter.product_no.is_none() && filter.keyword.is_none());
            if ids == ["missing"] {
                return Ok(PageResult {
                    items: vec![],
                    total: 0,
                });
            }
            assert_eq!(ids, ["product-1"]);
            return Ok(PageResult {
                items: vec![ProductRow {
                    id: "product-1".into(),
                    product_no: "P-1".into(),
                    product_kind: ProductKind::Physical,
                    name: Some("礼盒".into()),
                    category_id: Some("cat-1".into()),
                    brand_id: Some("brand-1".into()),
                    status: EnableStatus::Active,
                    listing_status: ProductListingStatus::PartiallyListed,
                    listed_sku_count: 1,
                    sku_count: 3,
                    supplied_sku_count: 2,
                    priced_sku_count: 1,
                    current_revision_id: Some("rev-1".into()),
                    version: 7,
                    created_at: 123,
                }],
                total: 1,
            });
        }
        assert!(filter.ids.is_none());
        assert_eq!(filter.product_no.as_deref(), Some("P-1"));
        assert_eq!(filter.page, 2);
        assert_eq!(filter.page_size, 3);
        Ok(PageResult {
            items: vec![ProductRow {
                id: "product-1".into(),
                product_no: "P-1".into(),
                product_kind: ProductKind::Physical,
                name: None,
                category_id: None,
                brand_id: Some("brand-1".into()),
                status: EnableStatus::Active,
                listing_status: ProductListingStatus::PartiallyListed,
                listed_sku_count: 1,
                sku_count: 3,
                supplied_sku_count: 2,
                priced_sku_count: 1,
                current_revision_id: None,
                version: 7,
                created_at: 123,
            }],
            total: 11,
        })
    }

    async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<PageResult<SellableSkuRow>> {
        self.calls.lock().unwrap().push("sellable");
        assert!(executor.session().is_none());
        assert_eq!(filter.keyword.as_deref(), Some("茶礼"));
        assert_eq!(filter.supplier_id, None);
        assert_eq!(filter.supply_region.as_deref(), Some("上海"));
        assert_eq!(filter.eligibility_as_of.to_string(), "2026-09-07");
        Ok(PageResult {
            items: vec![],
            total: 9,
        })
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
    let service = CatalogCenterReadService::new(query.clone());
    let params = serde_json::from_value(json!({"page": 0})).unwrap();
    assert!(matches!(
        service.product_list(&params).await,
        Err(crate::Error::ValidationError(_))
    ));
    assert!(query.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn product_projection_preserves_optional_fields_counts_and_page() {
    let query = Arc::new(RecordingQuery::default());
    let service = CatalogCenterReadService::new(query.clone());
    let params = serde_json::from_value(json!({"product_no":" P-1 ","page":2,"page_size":3})).unwrap();
    let page = service.product_list(&params).await.unwrap();
    assert_eq!((page.page, page.page_size, page.total), (2, 3, 11));
    let row = &page.items[0];
    assert_eq!(row.id, "product-1");
    assert_eq!(row.name, None);
    assert_eq!(row.current_revision_id, None);
    assert_eq!(
        (
            row.listed_sku_count,
            row.sku_count,
            row.supplied_sku_count,
            row.priced_sku_count
        ),
        (1, 3, 2, 1)
    );
    assert_eq!((row.version, row.created_at), (7, 123));
    assert_eq!(*query.calls.lock().unwrap(), vec!["product"]);
}

#[tokio::test]
async fn product_repository_failure_keeps_original_catalog_error_mapping() {
    let query = Arc::new(RecordingQuery {
        fail: true,
        ..Default::default()
    });
    let service = CatalogCenterReadService::new(query.clone());
    let params = serde_json::from_value(json!({})).unwrap();
    assert!(matches!(service.product_list(&params).await,
        Err(crate::Error::ConflictError(message)) if message == "数据已被其他请求修改，请刷新后重试"));
    assert_eq!(*query.calls.lock().unwrap(), vec!["product"]);
}

#[tokio::test]
async fn sellable_validation_precedes_price_validation_and_query() {
    let query = Arc::new(RecordingQuery::default());
    let service = CatalogCenterReadService::new(query.clone());
    let params =
        serde_json::from_value(json!({"page":0,"sales_price_min":"5.00","sales_price_max":"1.00"})).unwrap();
    assert!(matches!(service.sellable_sku_list(&params).await,
        Err(crate::Error::ValidationError(message)) if message.contains("页码必须大于0")));
    let params = serde_json::from_value(json!({"sales_price_min":"5.00","sales_price_max":"1.00"})).unwrap();
    assert!(matches!(service.sellable_sku_list(&params).await,
        Err(crate::Error::ValidationError(message)) if message == "最低销售价不能高于最高销售价"));
    assert!(query.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sellable_query_keeps_normalized_filters_explicit_date_and_default_page() {
    let query = Arc::new(RecordingQuery::default());
    let service = CatalogCenterReadService::new(query.clone());
    let params = serde_json::from_value(
        json!({"q":" 茶礼 ","supplier_id":" ","supply_region":" 上海 ","eligibility_as_of":"2026-09-07"}),
    )
    .unwrap();
    let page = service.sellable_sku_list(&params).await.unwrap();
    assert_eq!((page.page, page.page_size, page.total), (1, 20, 9));
    assert!(page.items.is_empty());
    assert_eq!(*query.calls.lock().unwrap(), vec!["sellable"]);
}

#[tokio::test]
async fn product_detail_filter_targets_single_id_without_business_filters() {
    let filter = super::product_detail_filter("product-1");
    assert_eq!(filter.ids.as_deref(), Some(["product-1".to_string()].as_slice()));
    assert_eq!((filter.page, filter.page_size), (1, 1));
    assert!(filter.product_no.is_none() && filter.keyword.is_none());
    assert!(filter.supplier_id.is_none() && filter.supply_coverage.is_none());
}

#[tokio::test]
async fn product_detail_preserves_list_supply_and_price_counts() {
    let query = Arc::new(RecordingQuery::default());
    let service = CatalogCenterReadService::new(query.clone());
    let view = service.product_detail("product-1").await.unwrap();
    assert_eq!(view.id, "product-1");
    assert_eq!((view.supplied_sku_count, view.priced_sku_count), (2, 1));
    assert_eq!((view.sku_count, view.listed_sku_count), (3, 1));
    assert_eq!(*query.calls.lock().unwrap(), vec!["product"]);
}

#[tokio::test]
async fn product_detail_missing_returns_not_found() {
    let query = Arc::new(RecordingQuery::default());
    let service = CatalogCenterReadService::new(query.clone());
    assert!(matches!(
        service.product_detail("missing").await,
        Err(crate::Error::NotFound(message)) if message == "商品不存在"
    ));
}
