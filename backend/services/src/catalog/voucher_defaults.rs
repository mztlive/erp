use database::{AccessControlExt, CatalogExt, NoTransaction};
use entities::catalog::product_brand::{ProductBrand, ProductBrandData};
use entities::catalog::product_category::{ProductCategory, ProductCategoryData};
use entities::catalog::unit_of_measure::{UnitOfMeasure, UnitOfMeasureData};
use entities::catalog::{EnableStatus, ProductBrandId, ProductCategoryId, ProductKind, UnitOfMeasureId};
use id_generator::next_id;

use super::CatalogService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 卡券类目共用根分类稳定代码（所有未指定分类的卡券类目挂在此节点下）。
const VOUCHER_ROOT_CATEGORY_CODE: &str = "VOUCHER";
/// 卡券类目共用根分类名称。
const VOUCHER_ROOT_CATEGORY_NAME: &str = "卡券";
/// 卡券类目默认品牌稳定代码。
const VOUCHER_DEFAULT_BRAND_CODE: &str = "FSY";
/// 卡券类目默认品牌名称（业务固定：福尚云）。
const VOUCHER_DEFAULT_BRAND_NAME: &str = "福尚云";
/// 卡券类目默认基础单位代码/名称/符号（业务固定：张）。
const VOUCHER_DEFAULT_UNIT_CODE: &str = "张";

impl CatalogService {
    /// 确保共用卡券根分类存在（代码 `VOUCHER` / 名称「卡券」/ `product_kind=VOUCHER`）。
    ///
    /// 已存在则直接返回其 ID；不存在则创建为根分类。并发创建冲突时重新查询一次。
    ///
    /// # 参数
    /// * `actor` - 审计操作人（仅在需要新建时写入 `created_by`）
    ///
    /// # 返回
    /// 返回卡券根分类稳定 ID。
    ///
    /// # 错误
    /// * `BusinessLogicError` - 已有同代码分类但 `product_kind` 不是 VOUCHER
    /// * `ConflictError` / `RepositoryError` - 持久化失败
    pub(super) async fn ensure_voucher_root_category(&self, actor: &AuditActor) -> Result<ProductCategoryId> {
        if let Some(existing) = self
            .db
            .product_categories()
            .find_one_by_field(
                "category_code",
                VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                &mut NoTransaction,
            )
            .await?
        {
            if existing.product_kind != ProductKind::Voucher {
                return Err(Error::BusinessLogicError(format!(
                    "系统卡券根分类（代码 {VOUCHER_ROOT_CATEGORY_CODE}）的商品类型不是卡券，请修正后再创建卡券类目"
                )));
            }
            return Ok(ProductCategoryId::new(existing.base.id));
        }

        let id = ProductCategoryId::new(next_id());
        let category = ProductCategory::new(
            id.clone(),
            ProductCategoryData {
                category_code: VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                parent_category_id: None,
                name: VOUCHER_ROOT_CATEGORY_NAME.to_string(),
                product_kind: ProductKind::Voucher,
                status: EnableStatus::Active,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("product_category.create", "product_category", id.to_string())?;
        match self
            .db
            .product_categories()
            .create(&category, &mut NoTransaction)
            .await
        {
            Ok(()) => {
                self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
                Ok(id)
            }
            Err(err) => match Error::from(err) {
                Error::ConflictError(_) => {
                    let existing = self
                        .db
                        .product_categories()
                        .find_one_by_field(
                            "category_code",
                            VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                            &mut NoTransaction,
                        )
                        .await?
                        .ok_or_else(|| Error::ConflictError("卡券根分类并发创建冲突，请重试".to_string()))?;
                    Ok(ProductCategoryId::new(existing.base.id))
                }
                other => Err(other),
            },
        }
    }

    /// 确保卡券默认品牌「福尚云」存在（代码 `FSY`）。
    ///
    /// 优先按 `brand_code=FSY` 查找；若无则按名称「福尚云」查找；仍无则创建。
    ///
    /// # 参数
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回品牌稳定 ID。
    ///
    /// # 错误
    /// 持久化失败时返回对应错误；并发冲突时重新查询。
    pub(super) async fn ensure_voucher_default_brand(&self, actor: &AuditActor) -> Result<ProductBrandId> {
        if let Some(existing) = self
            .db
            .product_brands()
            .find_one_by_field(
                "brand_code",
                VOUCHER_DEFAULT_BRAND_CODE.to_string(),
                &mut NoTransaction,
            )
            .await?
        {
            return Ok(ProductBrandId::new(existing.base.id));
        }
        if let Some(existing) = self
            .db
            .product_brands()
            .find_one_by_field("name", VOUCHER_DEFAULT_BRAND_NAME.to_string(), &mut NoTransaction)
            .await?
        {
            return Ok(ProductBrandId::new(existing.base.id));
        }

        let id = ProductBrandId::new(next_id());
        let brand = ProductBrand::new(
            id.clone(),
            ProductBrandData {
                brand_code: VOUCHER_DEFAULT_BRAND_CODE.to_string(),
                name: VOUCHER_DEFAULT_BRAND_NAME.to_string(),
                status: EnableStatus::Active,
                logo_file_asset_id: None,
            },
            actor.id(),
        )?;
        let audit = actor
            .clone()
            .resource_log("product_brand.create", "product_brand", id.to_string())?;
        match self.db.product_brands().create(&brand, &mut NoTransaction).await {
            Ok(()) => {
                self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
                Ok(id)
            }
            Err(err) => match Error::from(err) {
                Error::ConflictError(_) => {
                    let existing = self
                        .db
                        .product_brands()
                        .find_one_by_field(
                            "brand_code",
                            VOUCHER_DEFAULT_BRAND_CODE.to_string(),
                            &mut NoTransaction,
                        )
                        .await?
                        .ok_or_else(|| {
                            Error::ConflictError("默认品牌福尚云并发创建冲突，请重试".to_string())
                        })?;
                    Ok(ProductBrandId::new(existing.base.id))
                }
                other => Err(other),
            },
        }
    }

    /// 确保卡券默认基础单位「张」存在（代码/名称/符号均为「张」，整数数量）。
    ///
    /// # 参数
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回计量单位稳定 ID。
    ///
    /// # 错误
    /// 持久化失败时返回对应错误；并发冲突时重新查询。
    pub(super) async fn ensure_voucher_default_unit(&self, actor: &AuditActor) -> Result<UnitOfMeasureId> {
        if let Some(existing) = self
            .db
            .unit_of_measures()
            .find_one_by_field(
                "unit_code",
                VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                &mut NoTransaction,
            )
            .await?
        {
            if !existing.is_active() {
                return Err(Error::BusinessLogicError(
                    "默认单位「张」已停用，请启用后再创建卡券类目".to_string(),
                ));
            }
            return Ok(UnitOfMeasureId::new(existing.base.id));
        }

        let id = UnitOfMeasureId::new(next_id());
        let unit = UnitOfMeasure::new(
            id.clone(),
            UnitOfMeasureData {
                unit_code: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                name: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                symbol: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                quantity_scale: 0,
                status: EnableStatus::Active,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("unit_of_measure.create", "unit_of_measure", id.to_string())?;
        match self.db.unit_of_measures().create(&unit, &mut NoTransaction).await {
            Ok(()) => {
                self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
                Ok(id)
            }
            Err(err) => match Error::from(err) {
                Error::ConflictError(_) => {
                    let existing = self
                        .db
                        .unit_of_measures()
                        .find_one_by_field(
                            "unit_code",
                            VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                            &mut NoTransaction,
                        )
                        .await?
                        .ok_or_else(|| {
                            Error::ConflictError("默认单位「张」并发创建冲突，请重试".to_string())
                        })?;
                    Ok(UnitOfMeasureId::new(existing.base.id))
                }
                other => Err(other),
            },
        }
    }
}
