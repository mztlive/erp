//! 生成或恢复演示商品。商品引用已经生成的单位、品牌和分类。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_catalog::{CatalogExt, CreateProductRequest, HandoverProductRequest, Product};
use erp_core::ids::{ProductBrandId, ProductCategoryId, UnitOfMeasureId};
use persistence_core::NoTransaction;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::{DemoKind, DemoStep};
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle, spec};
use crate::adapters::catalog_access;
use crate::{Error, Result, handover_product};

impl DemoMasterDataService {
    /// 解析字典引用后创建或恢复商品及 SKU。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`records` - 已登记引用；`template` - 商品创建输入。
    ///
    /// # 返回
    /// 返回商品创建、恢复或已存在结果。
    ///
    /// # 错误
    /// 引用无效、编号归属不符或写入失败时返回错误。
    pub(super) async fn ensure_product(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        records: &HashMap<String, DemoMasterRecord>,
        template: &CreateProductRequest,
    ) -> Result<EnsureOutcome> {
        let maintainer_id = self.account_id(&spec::foundation_spec().product_maintainer_account).await?;
        if let Some(existing) = self
            .db
            .products()
            .find_one_by_field_including_deleted("product_no", step.key.clone(), &mut NoTransaction)
            .await?
        {
            return self.adopt_product(actor, step, &existing, &maintainer_id).await;
        }
        let request = product_request(template, records, &maintainer_id)?;
        let view = self.catalog().product_create(request, actor).await?;
        let sku_ids = self.sku_ids(&view.id).await?;
        self.remember_product(step, view.id, sku_ids).await?;
        Ok(EnsureOutcome::Created)
    }

    /// 校验登记归属，恢复商品与 SKU，并通过正式交接对齐采购维护责任。
    async fn adopt_product(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        product: &Product,
        maintainer_id: &str,
    ) -> Result<EnsureOutcome> {
        let id = &product.base.id;
        record::ensure_owned(&self.db, &step.key, id).await?;
        let sku_ids = self.sku_ids(id).await?;
        let live = product.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if !live {
            lifecycle::restore_product_graph(&self.db, actor, id, &sku_ids).await?;
        }
        self.align_product_maintainer(actor, id, maintainer_id).await?;
        self.remember_product(step, id.to_string(), sku_ids).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }

    /// 读取恢复后的最新版本，显式移交维护人和业务组织；失败时停止当前生成请求。
    async fn align_product_maintainer(
        &self,
        actor: &AuditActor,
        id: &str,
        maintainer_id: &str,
    ) -> Result<()> {
        let access = catalog_access(self.db.clone(), self.rbac.clone());
        let (maintainer, org) = access.maintainer_org(Some(maintainer_id), actor, &mut NoTransaction).await?;
        let product = access.require_product(actor, "update", id, &mut NoTransaction).await?;
        if let Some(request) = product_handover_request(&product, &maintainer, &org) {
            handover_product(self.db.clone(), self.rbac.clone(), id, request, actor).await?;
        }
        Ok(())
    }

    /// 读取商品当前及历史 SKU 的实际主键。
    async fn sku_ids(&self, product_id: &str) -> Result<Vec<String>> {
        let skus = self
            .db
            .skus()
            .find_many_by_field_including_deleted("product_id", product_id.to_string(), &mut NoTransaction)
            .await?;
        Ok(skus.into_iter().map(|sku| sku.base.id).collect())
    }

    /// 登记商品和全部 SKU 的实际主键。
    async fn remember_product(&self, step: &DemoStep, id: String, sku_ids: Vec<String>) -> Result<()> {
        record::save(
            &self.db,
            &DemoMasterRecord {
                key: step.key.clone(),
                kind: step.kind.as_str().to_string(),
                entity_id: id,
                related_ids: sku_ids,
                label: step.request.label().to_string(),
                removed: false,
            },
        )
        .await
    }
}

/// 只接受仍有效且种类匹配的种子引用。
fn required_link(records: &HashMap<String, DemoMasterRecord>, key: &str, kind: DemoKind) -> Result<String> {
    let record = records
        .get(key)
        .filter(|record| !record.removed && record.kind() == Some(kind))
        .ok_or_else(|| Error::ValidationError(format!("请先生成关联主数据：{key}")))?;
    Ok(record.entity_id.clone())
}

/// 解析字典引用并指定采购维护人，保持价格、规格和 SKU 编号不变。
fn product_request(
    template: &CreateProductRequest,
    records: &HashMap<String, DemoMasterRecord>,
    maintainer_id: &str,
) -> Result<CreateProductRequest> {
    let mut request = template.clone();
    request.maintainer_user_id = Some(maintainer_id.to_string());
    request.brand_id =
        ProductBrandId::new(required_link(records, template.brand_id.as_ref(), DemoKind::Brand)?);
    request.category_id =
        ProductCategoryId::new(required_link(records, template.category_id.as_ref(), DemoKind::Category)?);
    for sku in &mut request.skus {
        sku.base_unit_id =
            UnitOfMeasureId::new(required_link(records, sku.base_unit_id.as_ref(), DemoKind::Unit)?);
    }
    Ok(request)
}

/// 已对齐的商品不再写入；其他商品按当前版本构造可重试的正式交接请求。
fn product_handover_request(
    product: &Product,
    maintainer: &str,
    org: &str,
) -> Option<HandoverProductRequest> {
    if product.maintainer_user_id == maintainer && product.business_org_unit_id == org {
        return None;
    }
    Some(HandoverProductRequest {
        target_user_id: maintainer.to_string(),
        target_org_unit_id: Some(org.to_string()),
        reason: "演示商品交给采购账号及其主属部门".to_string(),
        expected_version: product.base.version,
        idempotency_key: format!("demo-product-handover-{}", product.base.version),
    })
}

#[cfg(test)]
mod tests {
    use erp_catalog::EnableStatus;
    use erp_catalog::entity::catalog::product::ProductData;
    use erp_core::ids::ProductId;
    use validator::Validate;

    use super::*;
    use crate::demo_master_data::plan;
    use crate::demo_master_data::seed::SeedRequest;

    #[test]
    fn references_resolve_to_registered_ids_and_reject_missing_removed_or_wrong_kind() {
        let steps = plan::demo_steps().unwrap();
        let SeedRequest::Product(template) = &steps.last().unwrap().request else {
            panic!("missing product")
        };
        let mut records = HashMap::new();
        for (key, kind, id) in [
            (template.brand_id.as_ref(), DemoKind::Brand, "brand-db-id"),
            (template.category_id.as_ref(), DemoKind::Category, "category-db-id"),
            (template.skus[0].base_unit_id.as_ref(), DemoKind::Unit, "unit-db-id"),
        ] {
            records.insert(
                key.to_string(),
                DemoMasterRecord {
                    key: key.into(),
                    kind: kind.as_str().into(),
                    entity_id: id.into(),
                    related_ids: vec![],
                    label: "label".into(),
                    removed: false,
                },
            );
        }
        let request = product_request(template, &records, "buyer-db-id").unwrap();
        assert_eq!(request.maintainer_user_id.as_deref(), Some("buyer-db-id"));
        assert_eq!(request.brand_id.as_ref(), "brand-db-id");
        assert_eq!(request.category_id.as_ref(), "category-db-id");
        assert_eq!(request.skus[0].base_unit_id.as_ref(), "unit-db-id");
        assert_eq!(request.skus[0].sales_visible_price_gross, template.skus[0].sales_visible_price_gross);
        let brand = records.get_mut(template.brand_id.as_ref()).unwrap();
        brand.removed = true;
        assert!(product_request(template, &records, "buyer-db-id").is_err());
        let brand = records.get_mut(template.brand_id.as_ref()).unwrap();
        brand.removed = false;
        brand.kind = DemoKind::Category.as_str().into();
        assert!(product_request(template, &records, "buyer-db-id").is_err());
        records.remove(template.brand_id.as_ref());
        assert!(product_request(template, &records, "buyer-db-id").is_err());
    }

    #[test]
    fn existing_demo_product_handover_aligns_owner_and_org_and_then_skips() {
        let mut product = existing_product("admin", "system-org");
        product.base.version = 7;
        let request = product_handover_request(&product, "buyer-db-id", "procurement-org").unwrap();
        request.validate().unwrap();
        assert_eq!(request.expected_version, 7);
        assert_eq!(request.idempotency_key, "demo-product-handover-7");
        let retry = product_handover_request(&product, "buyer-db-id", "procurement-org").unwrap();
        assert_eq!(serde_json::to_value(&request).unwrap(), serde_json::to_value(retry).unwrap());
        product.handover(request.target_user_id, request.target_org_unit_id, "admin").unwrap();
        assert_eq!(product.maintainer_user_id, "buyer-db-id");
        assert_eq!(product.business_org_unit_id, "procurement-org");
        assert_eq!(product.stable.created_by, "admin");
        assert!(product_handover_request(&product, "buyer-db-id", "procurement-org").is_none());
    }

    #[test]
    fn matching_owner_with_old_org_still_requires_handover() {
        let mut product = existing_product("buyer-db-id", "system-org");
        let first = product_handover_request(&product, "buyer-db-id", "procurement-org").unwrap();
        assert_eq!(first.target_org_unit_id.as_deref(), Some("procurement-org"));
        product.base.version += 1;
        let next = product_handover_request(&product, "buyer-db-id", "procurement-org").unwrap();
        assert_ne!(first.idempotency_key, next.idempotency_key);
    }

    /// 构造具有有效责任的旧演示商品，不依赖真实数据库。
    fn existing_product(owner: &str, org: &str) -> Product {
        let steps = plan::demo_steps().unwrap();
        let SeedRequest::Product(template) = &steps.last().unwrap().request else {
            panic!("missing product")
        };
        let mut product = Product::new(
            ProductId::new("demo-product"),
            ProductData {
                product_no: template.product_no.clone(),
                product_kind: template.product_kind,
                status: EnableStatus::Active,
                maintainer_user_id: owner.into(),
                business_org_unit_id: org.into(),
            },
            "admin",
        )
        .unwrap();
        product.base = entity_core::BaseModel::fake();
        product
    }
}
