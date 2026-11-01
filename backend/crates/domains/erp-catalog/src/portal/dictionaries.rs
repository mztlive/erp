//! 最终字典映射核对属于商品域。

use persistence_core::Executor;

use super::{
    CatalogPortalService, CategoryHierarchyNode, DictionaryInput, DraftSku, NewProductInput,
    NormalizedProduct, NormalizedSku,
};
use crate::entity::catalog::UnitOfMeasure;
use crate::repository::CatalogExt;
use crate::{Error, Result};

impl CatalogPortalService {
    /// 核对草稿里已经成对选择的品牌、分类和单位。
    ///
    /// 未同时给出标识与版本的字典跳过；已选分类还会核对商品类型和分类路径。
    ///
    /// # 参数
    /// * `input` - 新品原稿
    /// * `executor` - 当前读取执行器
    ///
    /// # 返回
    /// 已选项均有效时返回空结果。
    ///
    /// # 错误
    /// 所选字典不存在、已停用、版本变化、分类与商品类型不兼容、分类路径非法、单位精度超出或仓储读取失败时返回对应错误。
    pub(super) async fn ensure_selected_dictionaries(
        &self,
        input: &NewProductInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_selected_product_dictionaries(input, executor).await?;
        for sku in &input.skus {
            if let Some((id, version)) = sku.unit.selected_id.as_deref().zip(sku.unit.expected_version) {
                let mapped = NormalizedSku {
                    row_id: sku.row_id.clone(),
                    name: sku.name.trim().into(),
                    unit_id: id.into(),
                    unit_version: version,
                    unit_synonym_confirmation: None,
                    target_sku: None,
                };
                let unit = self.mapped_unit(sku, &mapped, executor).await?;
                ensure_unit_precision(sku, &unit)?;
            }
        }
        Ok(())
    }

    async fn ensure_selected_product_dictionaries(
        &self,
        input: &NewProductInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if let Some((id, version)) = input.brand.selected_id.as_deref().zip(input.brand.expected_version) {
            let brand = self
                .db
                .product_brands()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("所选品牌不存在".into()))?;
            ensure_dictionary(&input.brand, id, version, brand.base.version, brand.is_active())?;
        }
        if let Some((id, version)) =
            input.category.selected_id.as_deref().zip(input.category.expected_version)
        {
            let category = self
                .db
                .product_categories()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("所选分类不存在".into()))?;
            ensure_dictionary(&input.category, id, version, category.base.version, category.is_active())?;
            if category.product_kind != input.product_kind {
                return Err(Error::BusinessLogicError("分类与商品类型不兼容".into()));
            }
            self.category_path(&category, executor).await?;
        }
        Ok(())
    }

    /// 按已确认映射复验品牌、分类层级和每个 SKU 单位。
    ///
    /// # 参数
    /// * `input` - 新品原稿
    /// * `mapped` - 内部确认的字典与 SKU 映射
    /// * `executor` - 当前读取执行器
    ///
    /// # 返回
    /// 映射与当前字典事实一致时返回空结果。
    ///
    /// # 错误
    /// 原稿与映射不一致、分类或品牌未匹配、字典停用或版本变化、未知品牌与无品牌混用、分类路径变化、SKU 映射或单位核对失败，以及仓储读取失败时返回对应错误。
    pub(super) async fn ensure_dictionaries(
        &self,
        input: &NewProductInput,
        mapped: &NormalizedProduct,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        input.ensure_normalized(mapped)?;
        let category = self
            .db
            .product_categories()
            .find_by_id(&mapped.category_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("分类尚未完成有效匹配".into()))?;
        let brand = self
            .db
            .product_brands()
            .find_by_id(&mapped.brand_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("品牌尚未完成有效匹配".into()))?;
        ensure_dictionary(
            &input.category,
            &mapped.category_id,
            mapped.category_version,
            category.base.version,
            category.is_active(),
        )?;
        ensure_dictionary(
            &input.brand,
            &mapped.brand_id,
            mapped.brand_version,
            brand.base.version,
            brand.is_active(),
        )?;
        if input.brand.raw_name.trim() == "未知"
            || (input.brand.raw_name.trim() == "无品牌") != (brand.name.trim() == "无品牌")
        {
            return Err(Error::BusinessLogicError(
                "未知品牌尚未匹配；无品牌必须明确且不能与已知品牌混用".into(),
            ));
        }
        if category.product_kind != input.product_kind {
            return Err(Error::BusinessLogicError("分类与商品类型不兼容".into()));
        }
        let actual_hierarchy = self.category_hierarchy(&category, executor).await?;
        ensure_frozen_hierarchy(&mapped.category_hierarchy, &actual_hierarchy)?;
        for sku in &input.skus {
            let mapped_sku = mapped
                .sku_mappings
                .iter()
                .find(|mapping| mapping.row_id == sku.row_id)
                .ok_or_else(|| Error::ValidationError("SKU映射缺失".into()))?;
            self.ensure_unit(sku, mapped_sku, executor).await?;
        }
        Ok(())
    }

    async fn ensure_unit(
        &self,
        sku: &DraftSku,
        mapped: &NormalizedSku,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let unit = self.mapped_unit(sku, mapped, executor).await?;
        ensure_unit_meaning(sku, mapped, &unit)
    }

    /// 所选单位的身份、版本、启用状态最终由字典属主复验。
    async fn mapped_unit(
        &self,
        sku: &DraftSku,
        mapped: &NormalizedSku,
        executor: &mut dyn Executor,
    ) -> Result<UnitOfMeasure> {
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(&mapped.unit_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError(format!("SKU {} 单位尚未匹配", sku.row_id)))?;
        ensure_dictionary(
            &sku.unit,
            &mapped.unit_id,
            mapped.unit_version,
            unit.base.version,
            unit.is_active(),
        )?;
        Ok(unit)
    }
}

fn ensure_dictionary(
    input: &DictionaryInput,
    mapped_id: &str,
    mapped_version: u64,
    actual_version: u64,
    active: bool,
) -> Result<()> {
    if !active {
        return Err(Error::BusinessLogicError("映射字典已停用".into()));
    }
    if mapped_version != actual_version
        || input.expected_version.is_some_and(|version| version != actual_version)
    {
        return Err(Error::ConflictError("映射字典已变化，请重新核对".into()));
    }
    if input.selected_id.as_deref().is_some_and(|id| id != mapped_id) {
        return Err(Error::BusinessLogicError("不得静默改换供应商已选择的字典".into()));
    }
    Ok(())
}

fn ensure_unit_meaning(sku: &DraftSku, mapped: &NormalizedSku, unit: &UnitOfMeasure) -> Result<()> {
    let raw = sku.unit.raw_name.trim();
    if ![unit.name.as_str(), unit.unit_code.as_str(), unit.symbol.as_str()].contains(&raw) {
        mapped
            .unit_synonym_confirmation
            .as_ref()
            .ok_or_else(|| {
                Error::BusinessLogicError(format!(
                    "SKU {} 单位不同，须明确核对同义或退回供应商确认",
                    sku.row_id
                ))
            })?
            .ensure_for(&sku.unit.raw_name)?;
    }
    ensure_unit_precision(sku, unit)
}

/// 同义映射仍使用目标基础单位精度，禁止静默舍入数量。
fn ensure_unit_precision(sku: &DraftSku, unit: &UnitOfMeasure) -> Result<()> {
    if let Some(quantity) = sku.available_quantity
        && quantity.to_decimal().normalize().scale() > u32::from(unit.quantity_scale)
    {
        return Err(Error::ValidationError(format!("SKU {} 可供数量超出单位精度", sku.row_id)));
    }
    if let Some(packaging) = &sku.packaging
        && packaging.units_per_package.to_decimal().normalize().scale() > u32::from(unit.quantity_scale)
    {
        return Err(Error::ValidationError(format!("SKU {} 包装数量超出基础单位精度", sku.row_id)));
    }
    Ok(())
}

/// 根到叶的任何字典事实变化均使旧核对失效。
fn ensure_frozen_hierarchy(
    expected: &[CategoryHierarchyNode],
    actual: &[CategoryHierarchyNode],
) -> Result<()> {
    if expected != actual {
        return Err(Error::ConflictError("分类完整路径已变化，请重新核对每级分类".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::UnitOfMeasureId;
    use erp_core::money::Quantity;
    use serde_json::{json, to_value};

    use super::*;
    use crate::ProductKind;
    use crate::entity::catalog::unit_of_measure::UnitOfMeasureData;
    use crate::portal::UnitSynonymConfirmation;

    #[test]
    fn full_category_evidence_rejects_ancestor_changes_and_keeps_leaf_unchanged() {
        let original = vec![
            CategoryHierarchyNode {
                id: "root".into(),
                version: 3,
                name: "食品".into(),
                parent_id: None,
                product_kind: ProductKind::Physical,
            },
            CategoryHierarchyNode {
                id: "leaf".into(),
                version: 7,
                name: "饮料".into(),
                parent_id: Some("root".into()),
                product_kind: ProductKind::Physical,
            },
        ];
        assert!(ensure_frozen_hierarchy(&original, &original).is_ok());
        for field in 0..4 {
            let mut current = original.clone();
            match field {
                0 => current[0].version += 1,
                1 => current[0].name = "其他食品".into(),
                2 => current[0].parent_id = Some("new-root".into()),
                _ => current[0].product_kind = ProductKind::Voucher,
            }
            assert_eq!(current[1], original[1]);
            assert!(matches!(ensure_frozen_hierarchy(&original, &current), Err(Error::ConflictError(_))));
        }
    }

    #[test]
    fn dictionary_mapping_rejects_revocation_and_stale_versions() {
        let input = DictionaryInput {
            raw_name: "瓶".into(),
            selected_id: Some("unit".into()),
            expected_version: Some(2),
        };
        assert!(ensure_dictionary(&input, "unit", 2, 2, true).is_ok());
        assert!(matches!(ensure_dictionary(&input, "unit", 2, 2, false), Err(Error::BusinessLogicError(_))));
        assert!(matches!(ensure_dictionary(&input, "unit", 3, 3, true), Err(Error::ConflictError(_))));
        assert!(matches!(ensure_dictionary(&input, "other", 2, 2, true), Err(Error::BusinessLogicError(_))));
    }

    #[test]
    fn unit_mapping_preserves_meaning_and_rejects_quantity_rounding() {
        let unit = UnitOfMeasure::new(
            UnitOfMeasureId::new("bottle"),
            UnitOfMeasureData::new("BOTTLE", "瓶", "瓶"),
            "admin",
        )
        .unwrap();
        let mut sku = DraftSku {
            row_id: "r1".into(),
            name: "水".into(),
            spec_entries: vec![],
            unit: DictionaryInput { raw_name: "瓶".into(), selected_id: None, expected_version: None },
            barcode: None,
            image_asset_id: None,
            ordering_code: "water".into(),
            supply_terms: json!({}),
            packaging: None,
            quote_basis: None,
            available_quantity: None,
            reported_at: Instant::from_unix_secs(100),
        };
        let mapped = NormalizedSku {
            row_id: "r1".into(),
            name: "水".into(),
            unit_id: "bottle".into(),
            unit_version: 1,
            unit_synonym_confirmation: None,
            target_sku: None,
        };
        assert!(ensure_unit_meaning(&sku, &mapped, &unit).is_ok());
        sku.available_quantity = Some(Quantity::from_str("0").unwrap());
        assert!(ensure_unit_meaning(&sku, &mapped, &unit).is_ok());
        sku.available_quantity = Some(Quantity::from_str("1.5").unwrap());
        assert!(matches!(ensure_unit_meaning(&sku, &mapped, &unit), Err(Error::ValidationError(_))));
        sku.available_quantity = None;
        sku.unit.raw_name = "箱".into();
        assert!(matches!(ensure_unit_meaning(&sku, &mapped, &unit), Err(Error::BusinessLogicError(_))));
        sku.unit.raw_name = "瓶".into();
        sku.packaging = Some(super::super::PackagingInput {
            original_unit: "箱".into(),
            base_unit: "瓶".into(),
            units_per_package: Quantity::from_str("1.5").unwrap(),
            original_unit_price: "120".into(),
            conversion_confirmed_by_supplier: true,
        });
        assert!(matches!(ensure_unit_meaning(&sku, &mapped, &unit), Err(Error::ValidationError(_))));
        sku.packaging.as_mut().unwrap().units_per_package = Quantity::from_str("12").unwrap();
        assert!(ensure_unit_meaning(&sku, &mapped, &unit).is_ok());
        sku.unit.raw_name = "pcs".into();
        sku.packaging.as_mut().unwrap().base_unit = "pcs".into();
        assert!(matches!(ensure_unit_meaning(&sku, &mapped, &unit), Err(Error::BusinessLogicError(_))));
        let mut mapped = mapped;
        mapped.unit_synonym_confirmation = Some(UnitSynonymConfirmation {
            original_unit: "pcs".into(),
            same_unit_meaning_confirmed: true,
            reason: "供应商明确单瓶计价，pcs是单瓶同义，不换算包装".into(),
        });
        let original = to_value(&sku).unwrap();
        assert!(ensure_unit_meaning(&sku, &mapped, &unit).is_ok());
        assert_eq!(to_value(&sku).unwrap(), original);
        mapped.unit_synonym_confirmation.as_mut().unwrap().same_unit_meaning_confirmed = false;
        assert!(matches!(ensure_unit_meaning(&sku, &mapped, &unit), Err(Error::ValidationError(_))));
        mapped.unit_synonym_confirmation.as_mut().unwrap().same_unit_meaning_confirmed = true;
        mapped.unit_synonym_confirmation.as_mut().unwrap().original_unit = "箱".into();
        assert!(matches!(ensure_unit_meaning(&sku, &mapped, &unit), Err(Error::ValidationError(_))));
        mapped.unit_synonym_confirmation.as_mut().unwrap().original_unit = "pcs".into();
        mapped.unit_synonym_confirmation.as_mut().unwrap().reason = "  ".into();
        assert!(matches!(ensure_unit_meaning(&sku, &mapped, &unit), Err(Error::ValidationError(_))));
    }
}
