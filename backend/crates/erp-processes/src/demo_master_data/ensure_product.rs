//! 生成或恢复演示商品。商品引用已经生成的单位、品牌和分类。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_catalog::{CatalogExt, CreateProductRequest};
use erp_core::ids::{ProductBrandId, ProductCategoryId, UnitOfMeasureId};
use persistence_core::NoTransaction;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::{DemoKind, DemoStep};
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle};
use crate::{Error, Result};

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
        if let Some(existing) = self
            .db
            .products()
            .find_one_by_field_including_deleted("product_no", step.key.clone(), &mut NoTransaction)
            .await?
        {
            return self.adopt_product(actor, step, &existing.base.id, existing.base.deleted_at).await;
        }
        let view = self.catalog().product_create(product_request(template, records)?, actor).await?;
        let sku_ids = self.sku_ids(&view.id).await?;
        self.remember_product(step, view.id, sku_ids).await?;
        Ok(EnsureOutcome::Created)
    }

    /// 校验登记归属并恢复商品与 SKU。
    async fn adopt_product(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        id: &str,
        deleted_at: u64,
    ) -> Result<EnsureOutcome> {
        record::ensure_owned(&self.db, &step.key, id).await?;
        let sku_ids = self.sku_ids(id).await?;
        let live = deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if !live {
            lifecycle::restore_product_graph(&self.db, actor, id, &sku_ids).await?;
        }
        self.remember_product(step, id.to_string(), sku_ids).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
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

/// 解析 JSON 中的字典引用，保持价格、规格和 SKU 编号不变。
fn product_request(
    template: &CreateProductRequest,
    records: &HashMap<String, DemoMasterRecord>,
) -> Result<CreateProductRequest> {
    let mut request = template.clone();
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

#[cfg(test)]
mod tests {
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
        let request = product_request(template, &records).unwrap();
        assert_eq!(request.brand_id.as_ref(), "brand-db-id");
        assert_eq!(request.category_id.as_ref(), "category-db-id");
        assert_eq!(request.skus[0].base_unit_id.as_ref(), "unit-db-id");
        assert_eq!(request.skus[0].sales_visible_price_gross, template.skus[0].sales_visible_price_gross);
        let brand = records.get_mut(template.brand_id.as_ref()).unwrap();
        brand.removed = true;
        assert!(product_request(template, &records).is_err());
        let brand = records.get_mut(template.brand_id.as_ref()).unwrap();
        brand.removed = false;
        brand.kind = DemoKind::Category.as_str().into();
        assert!(product_request(template, &records).is_err());
        records.remove(template.brand_id.as_ref());
        assert!(product_request(template, &records).is_err());
    }
}
