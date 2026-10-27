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
    /// 将经内部确认的新规格加入已有商品，保持未上架并保存采购确认的销售参考价。
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
        let existing = self.db.catalog().skus_for_product(&ProductId::new(product_id), executor).await?;
        ProductSkuInput::fill_blank_sku_nos(
            &product.product_no,
            existing.into_iter().map(|sku| sku.sku_no),
            std::slice::from_mut(&mut input),
        );
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
        || !input.sku_no.trim().is_empty()
    {
        return Err(Error::ValidationError("API 选品只允许新增未上架规格，编号由系统分配".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_revision_keeps_confirmed_prices_and_rejects_invalid_values() {
        let mut input: ProductSkuInput = serde_json::from_value(serde_json::json!({
            "name":"来源规格", "base_unit_id":"unit", "sales_visible_price_gross":"120.00",
            "bulk_price_gross":"110.00", "bulk_min_quantity":"10", "market_price":"199.00"
        }))
        .unwrap();
        let revision =
            sku_revision_for_input(SkuId::new("sku"), 1, BusinessDate::today(), None, &input).unwrap();
        assert_eq!(revision.sales_visible_price_gross, input.sales_visible_price_gross);
        assert_eq!(revision.bulk_price_gross, input.bulk_price_gross);
        assert_eq!(revision.bulk_min_quantity, input.bulk_min_quantity);
        assert_eq!(revision.market_price, input.market_price);
        input.market_price = Some("-1.00".parse().unwrap());
        assert!(sku_revision_for_input(SkuId::new("sku"), 1, BusinessDate::today(), None, &input).is_err());
        input.market_price = None;
        input.bulk_min_quantity = Some("0".parse().unwrap());
        assert!(sku_revision_for_input(SkuId::new("sku"), 1, BusinessDate::today(), None, &input).is_err());
    }

    #[test]
    fn append_source_accepts_confirmed_prices_but_rejects_existing_or_explicit_identity() {
        let mut input: ProductSkuInput =
            serde_json::from_value(serde_json::json!({"name":"来源规格","base_unit_id":"unit"})).unwrap();
        assert!(ensure_source_input(&input).is_ok());
        input.sales_visible_price_gross = Some("10.00".parse().unwrap());
        assert!(ensure_source_input(&input).is_ok());
        input.sku_no = "manual".into();
        assert!(ensure_source_input(&input).is_err());
        input.sku_no.clear();
        input.sales_visible_price_gross = None;
        input.sku_id = Some(SkuId::new("existing"));
        assert!(ensure_source_input(&input).is_err());
    }
}
