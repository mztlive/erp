//! 一行联合导入的事务步骤；所有领域读写和审计共用调用方执行器。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_catalog::CatalogExt;
use erp_catalog::repository::prelude::*;
use erp_core::ids::{ProductId, SkuId, SupplierAccountId};
use erp_supply::ports::offering_qualification::QualificationPort;
use erp_supply::repository::{SupplierOfferingExt, SupplierOfferingRepositoryExt};
use erp_supply::service::supplier_offering::CommandPreparation;
use persistence_core::Executor;

use super::ProductImportProcess;
use super::identity::NormalizedImportRow;
use super::joint::{PreparedProduct, ProductWrite};
use super::row::RowImportOutcome;
use super::supply_input::SupplyImportInput;
use crate::adapters::{catalog_access, offering_access, scoped_catalog_service, scoped_offering_service};
use crate::audit::persist_log;
use crate::supply_governance::offering::MongoOfferingQualification;
use crate::{Error, Result};

pub(super) struct StoredProduct {
    id: String,
    sku_id: String,
    changed: bool,
}

#[async_trait]
pub(super) trait RowCommitPort: Send {
    async fn supplier(&mut self, executor: &mut dyn Executor) -> Result<SupplierAccountId>;
    async fn product(&mut self, executor: &mut dyn Executor) -> Result<StoredProduct>;
    async fn offering(
        &mut self,
        supplier: &SupplierAccountId,
        product: &StoredProduct,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

/// 先重验供应商，再写目录及供给；任一步失败交给外层事务整体回滚。
pub(super) async fn commit_row<P: RowCommitPort>(
    port: &mut P,
    executor: &mut dyn Executor,
) -> Result<RowImportOutcome> {
    let supplier = port.supplier(executor).await?;
    let product = port.product(executor).await?;
    let created = port.offering(&supplier, &product, executor).await?;
    let message = if created {
        "商品及供应商供给已导入，可供数量初始为 999"
    } else {
        "供应商供给已存在，原成本价及可供数量保持不变"
    };
    Ok(RowImportOutcome::new(message.into())
        .with_product_id(Some(product.id))
        .with_skipped(!created && !product.changed))
}

pub(super) struct ImportCommit<'a> {
    pub process: &'a ProductImportProcess,
    pub input: &'a SupplyImportInput,
    pub row: &'a NormalizedImportRow,
    pub actor: &'a AuditActor,
    pub prepared: Option<PreparedProduct>,
}

#[async_trait]
impl RowCommitPort for ImportCommit<'_> {
    async fn supplier(&mut self, executor: &mut dyn Executor) -> Result<SupplierAccountId> {
        self.process.import_supplier(self.input, self.actor, executor).await
    }

    async fn product(&mut self, executor: &mut dyn Executor) -> Result<StoredProduct> {
        let prepared = self.prepared.take().expect("每个行计划只在本次事务中消费一次");
        let service = scoped_catalog_service(self.process.db.clone(), self.process.rbac.clone());
        let changed = !matches!(prepared.write, ProductWrite::Existing(_));
        let product = match prepared.write {
            ProductWrite::Create(draft) => {
                service.write_product_draft(draft, self.actor, prepared.pending, executor).await?
            },
            ProductWrite::Update(plan) => {
                service.write_spec_edit_plan(plan, self.actor, prepared.pending, executor).await?
            },
            ProductWrite::Existing(id) => {
                catalog_access(self.process.db.clone(), self.process.rbac.clone())
                    .require_product(self.actor, "detail", &id, executor)
                    .await?
            },
        };
        let skus = self
            .process
            .db
            .skus()
            .find_by_product_ids(&[ProductId::new(product.base.id.clone())], executor)
            .await?;
        let revisions = self.process.db.catalog().current_sku_revisions(&skus, executor).await?;
        let sku = self
            .row
            .matching_sku(&skus, &revisions)?
            .ok_or_else(|| Error::ConflictError("商品规格已变化，无法建立供给，请重新导入".into()))?;
        Ok(StoredProduct { id: product.base.id, sku_id: sku.base.id.clone(), changed })
    }

    async fn offering(
        &mut self,
        supplier: &SupplierAccountId,
        product: &StoredProduct,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.process.write_import_offering(self.input, supplier, &product.sku_id, self.actor, executor).await
    }
}

impl ProductImportProcess {
    /// 创建尚不存在的供给；重复供给只复用身份，禁止重置成本或数量。
    async fn write_import_offering(
        &self,
        input: &SupplyImportInput,
        supplier: &SupplierAccountId,
        sku_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let qualification = MongoOfferingQualification::new(self.db.clone());
        qualification
            .ensure_qualified(supplier, &SkuId::new(sku_id), input.terms.valid_from.parse()?, executor)
            .await?;
        if let Some(existing) = self
            .db
            .supplier_offerings()
            .find_by_supplier_identity(supplier, &input.supplier_sku_code, executor)
            .await?
        {
            offering_access(self.db.clone(), self.rbac.clone())
                .require_offering(actor, "create", &existing.base.id, executor)
                .await?;
            if existing.sku_id.as_ref() != sku_id {
                return Err(Error::ConflictError("供应商订货编码已关联其他 SKU，请核对后重新导入".into()));
            }
            return Ok(false);
        }
        let request = input.request(supplier.as_ref(), sku_id);
        let service = scoped_offering_service(self.db.clone(), self.rbac.clone());
        let prepared = service.prepare_create(&request, actor, &qualification, executor).await?;
        let CommandPreparation::Apply(prepared) = prepared else {
            return Err(Error::ConflictError(
                "该导入已有供给登记记录，但原供给已不可用，请核对后重试".into(),
            ));
        };
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.create",
            "supplier_offering",
            prepared.offering.base.id.clone(),
            Some(request.change_reason),
        )?;
        service.persist_created(&prepared, executor).await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Marker;
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        calls: Vec<&'static str>,
        pointer: usize,
        fail: Option<&'static str>,
        new_supply: bool,
    }
    impl Recorder {
        fn record(&mut self, call: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            self.calls.push(call);
            if self.fail == Some(call) {
                return Err(Error::ValidationError(call.into()));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl RowCommitPort for Recorder {
        async fn supplier(&mut self, executor: &mut dyn Executor) -> Result<SupplierAccountId> {
            self.record("supplier", executor)?;
            Ok(SupplierAccountId::new("supplier-1"))
        }
        async fn product(&mut self, executor: &mut dyn Executor) -> Result<StoredProduct> {
            self.record("product", executor)?;
            Ok(StoredProduct { id: "product-1".into(), sku_id: "sku-1".into(), changed: false })
        }
        async fn offering(
            &mut self,
            supplier: &SupplierAccountId,
            product: &StoredProduct,
            executor: &mut dyn Executor,
        ) -> Result<bool> {
            assert_eq!(supplier.as_ref(), "supplier-1");
            assert_eq!(product.sku_id, "sku-1");
            self.record("offering", executor)?;
            Ok(self.new_supply)
        }
    }

    #[tokio::test]
    async fn supplier_failure_stops_writes_and_existing_sku_still_gets_offering() {
        for fail in [Some("supplier"), Some("product"), Some("offering"), None] {
            let mut executor = Marker;
            let mut port = Recorder {
                calls: vec![],
                pointer: &mut executor as *mut Marker as usize,
                fail,
                new_supply: true,
            };
            let result = commit_row(&mut port, &mut executor).await;
            let order = ["supplier", "product", "offering"];
            if let Some(fail) = fail {
                assert!(matches!(result, Err(Error::ValidationError(message)) if message == fail));
                assert_eq!(port.calls, order[..=order.iter().position(|call| *call == fail).unwrap()]);
            } else {
                assert!(!result.unwrap().skipped);
                assert_eq!(port.calls, order);
                port.new_supply = false;
                assert!(commit_row(&mut port, &mut executor).await.unwrap().skipped);
            }
        }
    }
}
