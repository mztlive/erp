use application_core::AuditActor;
use id_generator::next_id;
use persistence_core::Executor;

use super::CatalogService;
use crate::entity::catalog::{ProductBrandId, ProductCategoryId, UnitOfMeasureId, VoucherCatalogDefaults};
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

impl CatalogService {
    // 确保共用卡券根分类存在。
    //
    // 原事务内读取稳定代码；已存在时校验类型，不存在时原子创建。
    //
    // 返回共用卡券根分类稳定 ID。默认分类类型漂移、并发唯一键冲突或持久化失败时返回错误。
    //
    // `actor` 为审计操作人；仅在新建时写入创建人和审计日志。
    pub(super) async fn ensure_voucher_root_category(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProductCategoryId> {
        if let Some(category) = self.db.catalog().voucher_root_category(executor).await? {
            ensure_voucher_root_compatible(&category)?;
            return Ok(ProductCategoryId::new(category.base.id));
        }

        let category = VoucherCatalogDefaults::root_category(ProductCategoryId::new(next_id()), actor.id())?;
        let category_id = ProductCategoryId::new(category.base.id.clone());
        let audit = self.audit.resource_log(
            actor.clone(),
            "product_category.create",
            "product_category",
            category_id.to_string(),
        )?;
        self.db.product_categories().create(&category, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(category_id)
    }

    // 确保卡券默认品牌“福尚云”存在。
    //
    // Repository 优先按稳定代码 `FSY` 查询并兼容历史名称；不存在时按实体默认工厂在原事务中创建。
    //
    // 返回默认品牌稳定 ID。并发唯一键冲突或持久化失败时返回错误。
    //
    // `actor` 为审计操作人；仅在新建时写入创建人和审计日志。
    pub(super) async fn ensure_voucher_default_brand(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProductBrandId> {
        if let Some(brand) = self.db.catalog().voucher_default_brand(executor).await? {
            return Ok(ProductBrandId::new(brand.base.id));
        }

        let brand = VoucherCatalogDefaults::brand(ProductBrandId::new(next_id()), actor.id())?;
        let brand_id = ProductBrandId::new(brand.base.id.clone());
        let audit = self.audit.resource_log(
            actor.clone(),
            "product_brand.create",
            "product_brand",
            brand_id.to_string(),
        )?;
        self.db.product_brands().create(&brand, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(brand_id)
    }

    // 确保卡券默认基础单位“张”存在且启用。
    //
    // 原事务内校验既有单位的启用状态；不存在时按实体默认工厂创建。
    //
    // 返回默认基础单位稳定 ID。默认单位停用、并发唯一键冲突或持久化失败时返回错误。
    //
    // `actor` 为审计操作人；仅在新建时写入创建人和审计日志。
    pub(super) async fn ensure_voucher_default_unit(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<UnitOfMeasureId> {
        if let Some(unit) = self.db.catalog().voucher_default_unit(executor).await? {
            ensure_voucher_default_unit_active(&unit)?;
            return Ok(UnitOfMeasureId::new(unit.base.id));
        }

        let unit = VoucherCatalogDefaults::unit(UnitOfMeasureId::new(next_id()), actor.id())?;
        let unit_id = UnitOfMeasureId::new(unit.base.id.clone());
        let audit = self.audit.resource_log(
            actor.clone(),
            "unit_of_measure.create",
            "unit_of_measure",
            unit_id.to_string(),
        )?;
        self.db.unit_of_measures().create(&unit, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(unit_id)
    }
}

// 把默认卡券根分类兼容性规则映射为稳定的 Service 业务错误。
//
// 分类仍允许卡券类型时返回 `Ok(())`。默认稳定代码被错误用于其他商品类型时返回业务逻辑错误。
//
// `category` 为按默认稳定代码查询到的分类。
fn ensure_voucher_root_compatible(category: &crate::entity::catalog::ProductCategory) -> Result<()> {
    VoucherCatalogDefaults::ensure_root_category_compatible(category).map_err(|_| {
        Error::BusinessLogicError("系统的卡券根分类未设为卡券类型，请调整分类后再创建卡券类目".to_string())
    })
}

// 把默认基础单位启用规则映射为稳定的 Service 业务错误。
//
// 单位启用时返回 `Ok(())`。默认单位已停用时返回业务逻辑错误。
//
// `unit` 为按默认稳定代码查询到的计量单位。
fn ensure_voucher_default_unit_active(unit: &crate::entity::catalog::UnitOfMeasure) -> Result<()> {
    VoucherCatalogDefaults::ensure_unit_active(unit)
        .map_err(|_| Error::BusinessLogicError("默认单位“张”已停用，请启用后再创建卡券类目".to_string()))
}
