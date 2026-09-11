//! 解析或创建导入所需的品牌、分类与计量单位。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_catalog::entity::catalog::{EnableStatus, ProductKind};
use erp_catalog::{
    CatalogExt, CreateProductBrandRequest, CreateProductCategoryRequest, PRODUCT_IMPORT_UNIT_CODE,
    PRODUCT_IMPORT_UNIT_NAME,
};
use erp_core::ids::{ProductBrandId, ProductCategoryId, UnitOfMeasureId};
use persistence_core::NoTransaction;

use super::identity::stable_code;
use super::ProductImportProcess;
use crate::adapters::catalog_service;
use crate::{Error, Result};

/// 一次导入任务内复用的字典缓存。
#[derive(Debug, Default)]
pub struct ImportDictionaryCache {
    /// 品牌名称 → ID。
    pub brands: HashMap<String, ProductBrandId>,
    /// 分类名称 → ID。
    pub categories: HashMap<String, ProductCategoryId>,
    /// 基础单位。
    pub unit_id: Option<UnitOfMeasureId>,
}

impl ProductImportProcess {
    /// 解析基础单位「件」，缺失时拒绝本批导入行。
    ///
    /// # 参数
    /// * `cache` - 字典缓存
    ///
    /// # 返回
    /// 返回启用中的「件」单位 ID。
    ///
    /// # 错误
    /// 单位未维护时返回业务错误。
    pub(super) async fn resolve_unit(&self, cache: &mut ImportDictionaryCache) -> Result<UnitOfMeasureId> {
        if let Some(unit_id) = cache.unit_id.clone() {
            return Ok(unit_id);
        }
        let mut tx = NoTransaction;
        let by_code = self
            .db
            .unit_of_measures()
            .find_enabled_by_code(PRODUCT_IMPORT_UNIT_CODE, &mut tx)
            .await?;
        let unit = match by_code {
            Some(unit) => unit,
            None => self
                .db
                .unit_of_measures()
                .find_enabled_by_exact_name(PRODUCT_IMPORT_UNIT_NAME, &mut tx)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("请先维护计量单位「件」后再导入商品".into()))?,
        };
        let unit_id = UnitOfMeasureId::new(unit.base.id);
        cache.unit_id = Some(unit_id.clone());
        Ok(unit_id)
    }

    /// 按名称匹配或创建品牌。
    ///
    /// # 参数
    /// * `name` - 规范化品牌名称
    /// * `actor` - 审计操作人
    /// * `cache` - 字典缓存
    ///
    /// # 返回
    /// 返回品牌 ID。
    ///
    /// # 错误
    /// 创建失败且回读不到既有品牌时返回错误。
    pub(super) async fn resolve_brand(
        &self,
        name: &str,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
    ) -> Result<ProductBrandId> {
        if let Some(id) = cache.brands.get(name) {
            return Ok(id.clone());
        }
        let mut tx = NoTransaction;
        if let Some(existing) = self
            .db
            .product_brands()
            .find_enabled_by_exact_name(name, &mut tx)
            .await?
        {
            let id = ProductBrandId::new(existing.base.id);
            cache.brands.insert(name.to_string(), id.clone());
            return Ok(id);
        }
        let brand_code = stable_code("BRD", &[name]);
        if let Some(existing) = self
            .db
            .product_brands()
            .find_enabled_by_code(&brand_code, &mut tx)
            .await?
        {
            let id = ProductBrandId::new(existing.base.id);
            cache.brands.insert(name.to_string(), id.clone());
            return Ok(id);
        }
        let created = catalog_service(self.db.clone())
            .product_brand_create(
                CreateProductBrandRequest {
                    brand_code,
                    name: name.to_string(),
                    status: Some(EnableStatus::Active),
                    logo_file_asset_id: None,
                },
                actor,
            )
            .await;
        let id = match created {
            Ok(view) => ProductBrandId::new(view.id),
            Err(erp_catalog::Error::ConflictError(_)) => self.read_brand_after_conflict(name).await?,
            Err(error) => return Err(error.into()),
        };
        cache.brands.insert(name.to_string(), id.clone());
        Ok(id)
    }

    /// 按名称匹配或创建实物分类。
    ///
    /// # 参数
    /// * `name` - 规范化分类名称
    /// * `actor` - 审计操作人
    /// * `cache` - 字典缓存
    ///
    /// # 返回
    /// 返回分类 ID。
    ///
    /// # 错误
    /// 已有分类不允许实物类型，或创建失败时返回错误。
    pub(super) async fn resolve_category(
        &self,
        name: &str,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
    ) -> Result<ProductCategoryId> {
        if let Some(id) = cache.categories.get(name) {
            return Ok(id.clone());
        }
        let mut tx = NoTransaction;
        if let Some(existing) = self
            .db
            .product_categories()
            .find_enabled_by_exact_name(name, &mut tx)
            .await?
        {
            if existing.product_kind != ProductKind::Physical {
                return Err(Error::BusinessLogicError(format!("分类「{name}」不允许实物商品")));
            }
            let id = ProductCategoryId::new(existing.base.id);
            cache.categories.insert(name.to_string(), id.clone());
            return Ok(id);
        }
        let category_code = stable_code("CAT", &[name]);
        if let Some(existing) = self
            .db
            .product_categories()
            .find_enabled_by_code(&category_code, &mut tx)
            .await?
        {
            let id = ProductCategoryId::new(existing.base.id);
            cache.categories.insert(name.to_string(), id.clone());
            return Ok(id);
        }
        let created = crate::create_product_category(
            self.db.clone(),
            CreateProductCategoryRequest {
                category_code,
                parent_category_id: None,
                name: name.to_string(),
                product_kind: ProductKind::Physical,
                status: Some(EnableStatus::Active),
            },
            actor.clone(),
        )
        .await;
        let id = match created {
            Ok(view) => ProductCategoryId::new(view.id),
            Err(Error::ConflictError(_)) => self.read_category_after_conflict(name).await?,
            Err(error) => return Err(error),
        };
        cache.categories.insert(name.to_string(), id.clone());
        Ok(id)
    }

    async fn read_brand_after_conflict(&self, name: &str) -> Result<ProductBrandId> {
        let mut tx = NoTransaction;
        let existing = self
            .db
            .product_brands()
            .find_enabled_by_exact_name(name, &mut tx)
            .await?
            .ok_or_else(|| Error::ConflictError("品牌已存在，请刷新后重试".into()))?;
        Ok(ProductBrandId::new(existing.base.id))
    }

    async fn read_category_after_conflict(&self, name: &str) -> Result<ProductCategoryId> {
        let mut tx = NoTransaction;
        let existing = self
            .db
            .product_categories()
            .find_enabled_by_exact_name(name, &mut tx)
            .await?
            .ok_or_else(|| Error::ConflictError("分类已存在，请刷新后重试".into()))?;
        Ok(ProductCategoryId::new(existing.base.id))
    }
}
