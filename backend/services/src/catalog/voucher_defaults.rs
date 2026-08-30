use database::{CatalogExt, NoTransaction};
use entities::catalog::{ProductBrandId, ProductCategoryId, UnitOfMeasureId, VoucherCatalogDefaults};
use id_generator::next_id;

use super::CatalogService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl CatalogService {
    /// 确保共用卡券根分类存在。
    ///
    /// 已存在时校验其商品类型；不存在时按实体默认工厂创建，并在并发唯一键冲突后
    /// 重新读取稳定代码对应实体。
    ///
    /// # 参数
    /// * `actor` - 审计操作人；仅在新建时写入创建人和审计日志
    ///
    /// # 返回
    /// 返回共用卡券根分类稳定 ID。
    ///
    /// # 错误
    /// 默认分类类型漂移、并发冲突后仍不可见或持久化失败时返回错误。
    pub(super) async fn ensure_voucher_root_category(&self, actor: &AuditActor) -> Result<ProductCategoryId> {
        if let Some(category) = self
            .db
            .catalog()
            .voucher_root_category(&mut NoTransaction)
            .await?
        {
            ensure_voucher_root_compatible(&category)?;
            return Ok(ProductCategoryId::new(category.base.id));
        }

        let category = VoucherCatalogDefaults::root_category(ProductCategoryId::new(next_id()), actor.id())?;
        let category_id = ProductCategoryId::new(category.base.id.clone());
        let audit = actor.clone().resource_log(
            "product_category.create",
            "product_category",
            category_id.to_string(),
        )?;
        let category_for_tx = category.clone();
        match crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.product_categories().create(&category_for_tx, session).await?;
                Ok(())
            })
        })
        .await
        {
            Ok(()) => Ok(category_id),
            Err(error) => self.voucher_root_after_create_error(error).await,
        }
    }

    /// 在卡券根分类创建失败后处理并发唯一键冲突。
    ///
    /// # 参数
    /// * `error` - 分类单集合创建错误
    ///
    /// # 返回
    /// 唯一键冲突且并发创建实体可见时返回其稳定 ID。
    ///
    /// # 错误
    /// 非唯一键错误、冲突后实体仍不可见或默认类型漂移时返回错误。
    async fn voucher_root_after_create_error(&self, error: Error) -> Result<ProductCategoryId> {
        match error {
            Error::ConflictError(_) => {
                let category = self
                    .db
                    .catalog()
                    .voucher_root_category(&mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::ConflictError("卡券根分类并发创建冲突，请重试".to_string()))?;
                ensure_voucher_root_compatible(&category)?;
                Ok(ProductCategoryId::new(category.base.id))
            }
            other => Err(other),
        }
    }

    /// 确保卡券默认品牌“福尚云”存在。
    ///
    /// Repository 优先按稳定代码 `FSY` 查询并兼容历史名称；不存在时按实体默认工厂
    /// 创建，并在并发唯一键冲突后重新读取。
    ///
    /// # 参数
    /// * `actor` - 审计操作人；仅在新建时写入创建人和审计日志
    ///
    /// # 返回
    /// 返回默认品牌稳定 ID。
    ///
    /// # 错误
    /// 并发冲突后仍不可见或持久化失败时返回错误。
    pub(super) async fn ensure_voucher_default_brand(&self, actor: &AuditActor) -> Result<ProductBrandId> {
        if let Some(brand) = self
            .db
            .catalog()
            .voucher_default_brand(&mut NoTransaction)
            .await?
        {
            return Ok(ProductBrandId::new(brand.base.id));
        }

        let brand = VoucherCatalogDefaults::brand(ProductBrandId::new(next_id()), actor.id())?;
        let brand_id = ProductBrandId::new(brand.base.id.clone());
        let audit =
            actor
                .clone()
                .resource_log("product_brand.create", "product_brand", brand_id.to_string())?;
        let brand_for_tx = brand.clone();
        match crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.product_brands().create(&brand_for_tx, session).await?;
                Ok(())
            })
        })
        .await
        {
            Ok(()) => Ok(brand_id),
            Err(error) => self.voucher_brand_after_create_error(error).await,
        }
    }

    /// 在默认品牌创建失败后处理并发唯一键冲突。
    ///
    /// # 参数
    /// * `error` - 品牌单集合创建错误
    ///
    /// # 返回
    /// 唯一键冲突且并发创建实体可见时返回其稳定 ID。
    ///
    /// # 错误
    /// 非唯一键错误或冲突后实体仍不可见时返回错误。
    async fn voucher_brand_after_create_error(&self, error: Error) -> Result<ProductBrandId> {
        match error {
            Error::ConflictError(_) => self
                .db
                .catalog()
                .voucher_default_brand(&mut NoTransaction)
                .await?
                .map(|brand| ProductBrandId::new(brand.base.id))
                .ok_or_else(|| Error::ConflictError("默认品牌福尚云并发创建冲突，请重试".to_string())),
            other => Err(other),
        }
    }

    /// 确保卡券默认基础单位“张”存在且启用。
    ///
    /// 不存在时按实体默认工厂创建；并发唯一键冲突后重新读取并再次执行启用校验。
    ///
    /// # 参数
    /// * `actor` - 审计操作人；仅在新建时写入创建人和审计日志
    ///
    /// # 返回
    /// 返回默认基础单位稳定 ID。
    ///
    /// # 错误
    /// 默认单位停用、并发冲突后仍不可见或持久化失败时返回错误。
    pub(super) async fn ensure_voucher_default_unit(&self, actor: &AuditActor) -> Result<UnitOfMeasureId> {
        if let Some(unit) = self.db.catalog().voucher_default_unit(&mut NoTransaction).await? {
            ensure_voucher_default_unit_active(&unit)?;
            return Ok(UnitOfMeasureId::new(unit.base.id));
        }

        let unit = VoucherCatalogDefaults::unit(UnitOfMeasureId::new(next_id()), actor.id())?;
        let unit_id = UnitOfMeasureId::new(unit.base.id.clone());
        let audit =
            actor
                .clone()
                .resource_log("unit_of_measure.create", "unit_of_measure", unit_id.to_string())?;
        let unit_for_tx = unit.clone();
        match crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.unit_of_measures().create(&unit_for_tx, session).await?;
                Ok(())
            })
        })
        .await
        {
            Ok(()) => Ok(unit_id),
            Err(error) => self.voucher_unit_after_create_error(error).await,
        }
    }

    /// 在默认单位创建失败后处理并发唯一键冲突。
    ///
    /// # 参数
    /// * `error` - 计量单位单集合创建错误
    ///
    /// # 返回
    /// 唯一键冲突且并发创建实体可见、启用时返回其稳定 ID。
    ///
    /// # 错误
    /// 非唯一键错误、冲突后实体仍不可见或默认单位停用时返回错误。
    async fn voucher_unit_after_create_error(&self, error: Error) -> Result<UnitOfMeasureId> {
        match error {
            Error::ConflictError(_) => {
                let unit = self
                    .db
                    .catalog()
                    .voucher_default_unit(&mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::ConflictError("默认单位“张”并发创建冲突，请重试".to_string()))?;
                ensure_voucher_default_unit_active(&unit)?;
                Ok(UnitOfMeasureId::new(unit.base.id))
            }
            other => Err(other),
        }
    }
}

/// 把默认卡券根分类兼容性规则映射为稳定的 Service 业务错误。
///
/// # 参数
/// * `category` - 按默认稳定代码查询到的分类
///
/// # 返回
/// 分类仍允许卡券类型时返回 `Ok(())`。
///
/// # 错误
/// 默认稳定代码被错误用于其他商品类型时返回业务逻辑错误。
fn ensure_voucher_root_compatible(category: &entities::catalog::ProductCategory) -> Result<()> {
    VoucherCatalogDefaults::ensure_root_category_compatible(category).map_err(|_| {
        Error::BusinessLogicError(
            "系统卡券根分类（代码 VOUCHER）的商品类型不是卡券，请修正后再创建卡券类目".to_string(),
        )
    })
}

/// 把默认基础单位启用规则映射为稳定的 Service 业务错误。
///
/// # 参数
/// * `unit` - 按默认稳定代码查询到的计量单位
///
/// # 返回
/// 单位启用时返回 `Ok(())`。
///
/// # 错误
/// 默认单位已停用时返回业务逻辑错误。
fn ensure_voucher_default_unit_active(unit: &entities::catalog::UnitOfMeasure) -> Result<()> {
    VoucherCatalogDefaults::ensure_unit_active(unit)
        .map_err(|_| Error::BusinessLogicError("默认单位“张”已停用，请启用后再创建卡券类目".to_string()))
}
