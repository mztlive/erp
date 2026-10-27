//! 已选供应商商品的后续规格加入现有公司 SPU，沿用原规格实体规则。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{ProductId, SkuId};
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::CatalogService;
use super::sku_edit::{sku_revision_for_input, specification_signature_for};
use crate::entity::catalog::sku::SkuData;
use crate::repository::CatalogExt;
use crate::{EnableStatus, Error, ListingStatus, ProductSkuInput, Result, Sku};

impl CatalogService {
    /// 将经内部确认的新规格加入已有商品，保持未上架且不带入销售价。
    /// # 参数
    /// `product_id` 为已经绑定的公司商品；`input` 为新规格；`actor` 为当前操作人；
    /// `executor` 由选品用例提供，商品、供给和来源关系必须沿用同一事务。
    /// # 返回
    /// 新建的公司 SKU。
    /// # 错误
    /// 商品越权、停用、字典失效、规格重复、非法新身份或写入失败时拒绝。
    pub async fn append_source_sku(
        &self,
        product_id: &str,
        mut input: ProductSkuInput,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Sku> {
        input.validate()?;
        ensure_source_input(&input)?;
        let mut product = self.access().require_product(actor, "update", product_id, executor).await?;
        if !product.is_active() {
            return Err(Error::BusinessLogicError("来源对应公司商品已停用".into()));
        }
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(input.base_unit_id.as_ref(), executor)
            .await?
            .filter(|unit| unit.is_active())
            .ok_or_else(|| Error::ValidationError("请选择启用的基础单位".into()))?;
        let signature = specification_signature_for(&input.spec_entries)?;
        if self.db.catalog().sku_signature_exists(&ProductId::new(product_id), &signature, executor).await? {
            return Err(Error::ConflictError("该公司商品已存在相同规格，请明确匹配已有 SKU".into()));
        }
        let sku_id = SkuId::new(next_id());
        input.sku_no = format!("SKU-{}", sku_id.as_ref());
        let revision = sku_revision_for_input(sku_id.clone(), 1, BusinessDate::today(), None, &input)?;
        let mut sku = Sku::new(
            sku_id,
            SkuData {
                sku_no: input.sku_no,
                product_id: product.base.id.clone().into(),
                base_unit_id: unit.base.id.into(),
                specification_signature: signature,
                status: EnableStatus::Active,
                listing_status: ListingStatus::Unlisted,
            },
            actor.id(),
        )?;
        sku.attach_revision(&revision, actor.id())?;
        self.db.catalog().create_sku_with_revision(&sku, &revision, &[], executor).await?;
        product.stable.updated_by = actor.id().into();
        self.db.products().update(&mut product, executor).await?;
        let audit = self.audit.resource_log(actor.clone(), "product.update", "product", product.base.id)?;
        self.audit.persist(&audit, executor).await?;
        Ok(sku)
    }
}

fn ensure_source_input(input: &ProductSkuInput) -> Result<()> {
    if input.sku_id.is_some()
        || input.expected_sku_revision_id.is_some()
        || input.reenable
        || input.sales_visible_price_gross.is_some()
        || input.bulk_price_gross.is_some()
        || input.market_price.is_some()
        || input.bulk_min_quantity.is_some()
    {
        return Err(Error::ValidationError("API 选品只允许新增未上架且无销售价的规格".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_source_only_accepts_fresh_unpriced_sku_intent() {
        let mut input: ProductSkuInput =
            serde_json::from_value(serde_json::json!({"name":"来源规格","base_unit_id":"unit"})).unwrap();
        assert!(ensure_source_input(&input).is_ok());
        input.sales_visible_price_gross = Some("10.00".parse().unwrap());
        assert!(ensure_source_input(&input).is_err());
        input.sales_visible_price_gross = None;
        input.sku_id = Some(SkuId::new("existing"));
        assert!(ensure_source_input(&input).is_err());
    }
}
