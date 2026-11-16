use std::sync::atomic::{AtomicUsize, Ordering};

use erp_catalog::Result as CatalogResult;
use erp_catalog::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};
use erp_sales::Error as SalesError;
use persistence_core::{
    Error as PersistenceError, ExecutionBoundary, PageResult, Result as PersistenceResult,
};
use serde_json::{from_value, json};

use super::*;

struct RecordingQuery {
    boundary: ExecutionBoundary,
    refs: Vec<(String, String)>,
    date: BusinessDate,
    returned_refs: Vec<(String, String)>,
    calls: AtomicUsize,
    fail: bool,
}

#[async_trait]
impl CatalogSupplyQueryPort for RecordingQuery {
    async fn product_candidate_ids(
        &self,
        _: &ProductFilter,
        _: &mut dyn Executor,
    ) -> CatalogResult<Vec<String>> {
        panic!("选品发布复验必须查询精确修订")
    }

    async fn product_page(
        &self,
        _: &ProductFilter,
        _: &mut dyn Executor,
    ) -> CatalogResult<PageResult<ProductRow>> {
        panic!("选品发布复验必须查询精确修订")
    }

    async fn search_sellable_skus(
        &self,
        _: &SellableSkuFilter,
        _: &mut dyn Executor,
    ) -> PersistenceResult<PageResult<SellableSkuRow>> {
        panic!("选品发布复验不得查询分页商品池")
    }

    async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> PersistenceResult<Vec<SellableSkuRow>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(self.boundary.is_current(executor));
        assert_eq!(refs, self.refs);
        assert_eq!(date, self.date);
        if self.fail {
            return Err(PersistenceError::OptimisticLockingError);
        }
        Ok(self.returned_refs.iter().map(|(sku, revision)| row(sku, revision)).collect())
    }

    async fn find_sellable_skus_by_ids(
        &self,
        _: &[String],
        _: BusinessDate,
        _: &mut dyn Executor,
    ) -> PersistenceResult<Vec<SellableSkuRow>> {
        panic!("选品发布复验不得替换为当前 SKU 修订")
    }
}

fn row(sku_id: &str, revision_id: &str) -> SellableSkuRow {
    from_value(json!({
        "sku_id": sku_id,
        "sku_version": 3,
        "sku_revision_id": revision_id,
        "sku_revision_no": 2,
        "sku_no": "SKU-1",
        "product_id": "product-1",
        "product_no": "P-1",
        "product_kind": "PHYSICAL",
        "name": "茶礼",
        "specification_signature": "",
        "base_unit_id": "unit-1",
        "sales_visible_price_gross": "10.00",
        "effective_from": "2026-09-01",
        "supplier_count": 1
    }))
    .unwrap()
}

#[tokio::test]
async fn qualified_refs_preserves_the_original_executor_refs_date_and_exact_result() {
    let mut executor = ReadExecutor::default();
    let refs = vec![("sku-z".into(), "rev-2".into()), ("sku-a".into(), "rev-1".into())];
    let returned_refs = vec![("sku-a".into(), "rev-1".into())];
    let date = "2026-11-17".parse().unwrap();
    let query = Arc::new(RecordingQuery {
        boundary: executor.boundary().unwrap(),
        refs: refs.clone(),
        date,
        returned_refs: returned_refs.clone(),
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let adapter = CatalogAdapter { query: query.clone() };

    let actual = adapter.qualified_refs(&refs, date, &mut executor).await.unwrap();

    assert_eq!(actual, returned_refs);
    assert_eq!(query.calls.load(Ordering::SeqCst), 1);
    assert!(query.boundary.is_current(&executor));
}

#[tokio::test]
async fn qualified_refs_preserves_provider_failure_without_retry() {
    let mut executor = ReadExecutor::default();
    let refs = vec![("sku-z".into(), "rev-2".into())];
    let date = "2026-11-17".parse().unwrap();
    let query = Arc::new(RecordingQuery {
        boundary: executor.boundary().unwrap(),
        refs: refs.clone(),
        date,
        returned_refs: Vec::new(),
        calls: AtomicUsize::new(0),
        fail: true,
    });
    let adapter = CatalogAdapter { query: query.clone() };

    let error = adapter.qualified_refs(&refs, date, &mut executor).await.unwrap_err();

    assert!(
        matches!(error, SalesError::ConflictError(message) if message == "数据已被其他请求修改，请刷新后重试")
    );
    assert_eq!(query.calls.load(Ordering::SeqCst), 1);
    assert!(query.boundary.is_current(&executor));
}
