//! 供给资格的实际catalog/supplier组合，按原位置读取六项事实。
use async_trait::async_trait;
use erp_catalog::{CatalogExt, ProductKind};
use erp_core::{
    common::time::BusinessDate,
    ids::{ProductId, SkuId, SupplierAccountId},
};
use erp_supplier::entity::supplier::eligibility::OfferingProductKind;
use erp_supply::ports::offering_qualification::QualificationPort;
use mongodb::Database;
use persistence_core::Executor;
use services::{Error, Result};
/// 生产资格适配器；构造不读取任何事实。
pub struct MongoOfferingQualification {
    db: Database,
}
impl MongoOfferingQualification {
    /// 绑定数据库；资格读取由ensure_qualified在原命令位置触发。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
struct SkuQualificationFact {
    product_id: ProductId,
    is_active: bool,
}
#[async_trait]
trait CatalogQualificationPort: Sync {
    async fn sku(&self, id: &SkuId, executor: &mut dyn Executor) -> Result<Option<SkuQualificationFact>>;
    async fn product_kind(&self, id: &ProductId, executor: &mut dyn Executor) -> Result<Option<ProductKind>>;
    async fn supplier(
        &self,
        id: &SupplierAccountId,
        kind: OfferingProductKind,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
#[async_trait]
impl CatalogQualificationPort for MongoOfferingQualification {
    async fn sku(&self, id: &SkuId, executor: &mut dyn Executor) -> Result<Option<SkuQualificationFact>> {
        Ok(self
            .db
            .skus()
            .find_by_id(id, executor)
            .await?
            .map(|sku| SkuQualificationFact {
                is_active: sku.is_active(),
                product_id: sku.product_id,
            }))
    }
    async fn product_kind(&self, id: &ProductId, executor: &mut dyn Executor) -> Result<Option<ProductKind>> {
        Ok(self
            .db
            .products()
            .find_by_id(id, executor)
            .await?
            .map(|product| product.product_kind))
    }
    async fn supplier(
        &self,
        id: &SupplierAccountId,
        kind: OfferingProductKind,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        erp_supplier::service::supplier::eligibility::ensure_offering_capability_qualified(
            &self.db, id, kind, on_date, executor,
        )
        .await
        .map_err(Into::into)
    }
}
#[async_trait]
impl QualificationPort for MongoOfferingQualification {
    type Error = Error;
    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        sku_id: &SkuId,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        qualify(self, supplier_id, sku_id, on_date, executor).await
    }
}
/// SKU存在/启用先于商品与供应商读取；供应商能力政策只委派一次。
async fn qualify<P: CatalogQualificationPort>(
    port: &P,
    supplier_id: &SupplierAccountId,
    sku_id: &SkuId,
    on_date: BusinessDate,
    executor: &mut dyn Executor,
) -> Result<()> {
    let sku = port
        .sku(sku_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("公司 SKU 不存在".to_string()))?;
    if !sku.is_active {
        return Err(Error::BusinessLogicError("公司 SKU 未启用".to_string()));
    }
    let kind = port
        .product_kind(&sku.product_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("公司商品不存在".to_string()))?;
    let fact = match kind {
        ProductKind::Physical => OfferingProductKind::Physical,
        ProductKind::Virtual => OfferingProductKind::Virtual,
        ProductKind::OfflineService => OfferingProductKind::OfflineService,
        ProductKind::Voucher => OfferingProductKind::Voucher,
    };
    port.supplier(supplier_id, fact, on_date, executor).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Marker(u64);
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        pointer: usize,
        calls: Mutex<Vec<&'static str>>,
        fail: Option<usize>,
        kind: ProductKind,
        active: bool,
        missing_sku: bool,
        missing_product: bool,
    }
    impl Recorder {
        fn record(&self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            let mut calls = self.calls.lock().unwrap();
            let i = calls.len();
            calls.push(name);
            if self.fail == Some(i) {
                return Err(Error::ConflictError(format!("catalog {i}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl CatalogQualificationPort for Recorder {
        async fn sku(&self, id: &SkuId, executor: &mut dyn Executor) -> Result<Option<SkuQualificationFact>> {
            assert_eq!(id.as_ref(), "sku");
            self.record("sku", executor)?;
            Ok((!self.missing_sku).then(|| SkuQualificationFact {
                product_id: ProductId::new("product"),
                is_active: self.active,
            }))
        }
        async fn product_kind(
            &self,
            id: &ProductId,
            executor: &mut dyn Executor,
        ) -> Result<Option<ProductKind>> {
            assert_eq!(id.as_ref(), "product");
            self.record("product", executor)?;
            Ok((!self.missing_product).then_some(self.kind))
        }
        async fn supplier(
            &self,
            id: &SupplierAccountId,
            kind: OfferingProductKind,
            on_date: BusinessDate,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(id.as_ref(), "supplier");
            assert_eq!(on_date, BusinessDate::from_ymd(2026, 1, 2).unwrap());
            let expected = match self.kind {
                ProductKind::Physical => OfferingProductKind::Physical,
                ProductKind::Virtual => OfferingProductKind::Virtual,
                ProductKind::OfflineService => OfferingProductKind::OfflineService,
                ProductKind::Voucher => OfferingProductKind::Voucher,
            };
            assert_eq!(kind, expected);
            self.record("supplier", executor)
        }
    }
    async fn invoke(
        kind: ProductKind,
        active: bool,
        missing_sku: bool,
        missing_product: bool,
        fail: Option<usize>,
    ) -> (Result<()>, Vec<&'static str>) {
        let mut executor = Marker(162);
        let port = Recorder {
            pointer: &mut executor as *mut Marker as usize,
            calls: Mutex::new(vec![]),
            fail,
            kind,
            active,
            missing_sku,
            missing_product,
        };
        let result = qualify(
            &port,
            &SupplierAccountId::new("supplier"),
            &SkuId::new("sku"),
            BusinessDate::from_ymd(2026, 1, 2).unwrap(),
            &mut executor,
        )
        .await;
        assert_eq!(executor.0, 162);
        (result, port.calls.into_inner().unwrap())
    }
    #[tokio::test]
    async fn catalog_qualification_keeps_executor_and_all_kind_variants() {
        for kind in [
            ProductKind::Physical,
            ProductKind::Virtual,
            ProductKind::OfflineService,
            ProductKind::Voucher,
        ] {
            let (result, calls) = invoke(kind, true, false, false, None).await;
            result.unwrap();
            assert_eq!(calls, ["sku", "product", "supplier"]);
        }
    }
    #[tokio::test]
    async fn catalog_qualification_stops_on_every_provider_error() {
        for i in 0..3 {
            let (result, calls) = invoke(ProductKind::Physical, true, false, false, Some(i)).await;
            assert!(matches!(result,Err(Error::ConflictError(ref e)) if e==&format!("catalog {i}")));
            assert_eq!(calls, ["sku", "product", "supplier"][..=i]);
        }
    }
    #[tokio::test]
    async fn sku_and_product_fail_before_supplier_qualification() {
        for (active, missing_sku, missing_product, message, expected) in [
            (true, true, false, "公司 SKU 不存在", 1),
            (false, false, false, "公司 SKU 未启用", 1),
            (true, false, true, "公司商品不存在", 2),
        ] {
            let (result, calls) =
                invoke(ProductKind::Physical, active, missing_sku, missing_product, None).await;
            let error = result.unwrap_err();
            assert!(matches!(error,Error::NotFound(ref e)|Error::BusinessLogicError(ref e) if e==message));
            assert_eq!(calls.len(), expected);
        }
    }
}
