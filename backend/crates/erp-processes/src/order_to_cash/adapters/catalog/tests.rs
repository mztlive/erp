use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use erp_catalog::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};
use erp_core::money::Quantity;
use persistence_core::PageResult;

use super::*;

struct TestExecutor(u64);
impl Executor for TestExecutor {
    fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
        assert_eq!(self.0, 71);
        None
    }
}

struct RecordingQuery {
    pointer: usize,
    refs: Vec<(String, String)>,
    calls: AtomicUsize,
    fail: bool,
}

#[async_trait]
impl CatalogSupplyQueryPort for RecordingQuery {
    /// 销售资格复验必须继续使用精确引用查询。
    async fn product_candidate_ids(
        &self,
        _: &ProductFilter,
        _: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<String>> {
        panic!("sales qualification must use the exact-reference query")
    }
    async fn product_page(
        &self,
        _: &ProductFilter,
        _: &mut dyn Executor,
    ) -> persistence_core::Result<PageResult<ProductRow>> {
        panic!("sales qualification must use the exact-reference query")
    }
    async fn search_sellable_skus(
        &self,
        _: &SellableSkuFilter,
        _: &mut dyn Executor,
    ) -> persistence_core::Result<PageResult<SellableSkuRow>> {
        panic!("sales qualification must not load a paginated list")
    }
    async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<SellableSkuRow>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
        assert!(executor.session().is_none());
        assert_eq!(refs, self.refs);
        assert_eq!(date.to_string(), "2026-09-07");
        if self.fail {
            return Err(persistence_core::Error::OptimisticLockingError);
        }
        Ok(refs
            .iter()
            .map(|(sku, revision)| {
                serde_json::from_value(serde_json::json!({
                    "sku_id":sku,"sku_version":3,"sku_revision_id":revision,"sku_revision_no":2,
                    "sku_no":"SKU-1","product_id":"product-1","product_no":"P-1",
                    "product_kind":"PHYSICAL","name":"茶礼","specification_signature":"",
                    "base_unit_id":"unit-1","sales_visible_price_gross":"10.00",
                    "bulk_price_gross":"8.00","bulk_min_quantity":"10",
                    "effective_from":"2026-09-01","supplier_count":1
                }))
                .unwrap()
            })
            .collect())
    }
    async fn find_sellable_skus_by_ids(
        &self,
        _: &[String],
        _: BusinessDate,
        _: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<SellableSkuRow>> {
        panic!("sales qualification must use the exact-reference query")
    }
}

#[tokio::test]
async fn exact_qualification_keeps_refs_date_and_nonzero_executor() {
    let mut executor = TestExecutor(71);
    let refs = vec![("sku-z".into(), "rev-2".into()), ("sku-a".into(), "rev-1".into())];
    let query = Arc::new(RecordingQuery {
        pointer: &mut executor as *mut TestExecutor as usize,
        refs: refs.clone(),
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let adapter = CatalogQualificationAdapter { query: query.clone() };
    let result = adapter.qualified_refs(&refs, "2026-09-07".parse().unwrap(), &mut executor).await.unwrap();
    assert_eq!(result, refs);
    assert_eq!(query.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn exact_qualification_keeps_persistence_failure_without_retry() {
    let mut executor = TestExecutor(71);
    let refs = vec![("sku-z".into(), "rev-2".into())];
    let query = Arc::new(RecordingQuery {
        pointer: &mut executor as *mut TestExecutor as usize,
        refs: refs.clone(),
        calls: AtomicUsize::new(0),
        fail: true,
    });
    let adapter = CatalogQualificationAdapter { query: query.clone() };
    assert!(matches!(adapter.qualified_refs(&refs, "2026-09-07".parse().unwrap(), &mut executor).await,
        Err(erp_sales::Error::ConflictError(message)) if message == "数据已被其他请求修改，请刷新后重试"));
    assert_eq!(query.calls.load(Ordering::SeqCst), 1);
}

/// 按同一精确修订的数量边界解析自动报价，同时保留传入执行器。
#[tokio::test]
async fn reference_prices_use_catalog_quantity_rule_and_exact_revision() {
    let mut executor = TestExecutor(71);
    let requests = ["9.999999", "10", "10.000001"].map(|quantity| SalesReferencePriceRequest {
        sku_id: "sku-1".into(),
        sku_revision_id: "rev-1".into(),
        quantity: Quantity::from_str(quantity).unwrap(),
    });
    let query = Arc::new(RecordingQuery {
        pointer: &mut executor as *mut TestExecutor as usize,
        refs: vec![("sku-1".into(), "rev-1".into()); 3],
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let adapter = CatalogQualificationAdapter { query: query.clone() };
    let facts =
        adapter.reference_prices(&requests, "2026-09-07".parse().unwrap(), &mut executor).await.unwrap();
    assert_eq!(
        facts.iter().map(|fact| fact.unit_price_gross).collect::<Vec<_>>(),
        ["10", "8", "8"].map(|price| UnitPrice::from_str(price).unwrap())
    );
    assert_eq!(
        facts.iter().map(|fact| &fact.request).collect::<Vec<_>>(),
        requests.iter().collect::<Vec<_>>()
    );
    assert_eq!(query.calls.load(Ordering::SeqCst), 1);
}

/// 自动报价读取失败保持原错误类别，且不重试供应方读取。
#[tokio::test]
async fn reference_prices_keep_provider_failure_without_retry() {
    let mut executor = TestExecutor(71);
    let requests = [SalesReferencePriceRequest {
        sku_id: "sku-1".into(),
        sku_revision_id: "rev-1".into(),
        quantity: Quantity::from_str("10").unwrap(),
    }];
    let query = Arc::new(RecordingQuery {
        pointer: &mut executor as *mut TestExecutor as usize,
        refs: vec![("sku-1".into(), "rev-1".into())],
        calls: AtomicUsize::new(0),
        fail: true,
    });
    let adapter = CatalogQualificationAdapter { query: query.clone() };
    assert!(matches!(
        adapter.reference_prices(&requests, "2026-09-07".parse().unwrap(), &mut executor).await,
        Err(erp_sales::Error::ConflictError(_))
    ));
    assert_eq!(query.calls.load(Ordering::SeqCst), 1);
}
