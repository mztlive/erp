use database::{CatalogExt, NoTransaction};
use entities::catalog::product::Product;
use entities::catalog::product_brand::ProductBrand;
use entities::catalog::product_category::ProductCategory;
use entities::catalog::unit_of_measure::UnitOfMeasure;
use entities::catalog::{ProductBrandId, UnitOfMeasureId};

use super::CatalogService;
use crate::errors::{Error, Result};

impl CatalogService {
    // ---------- 私有加载与写入辅助 ----------

    /// 按 ID 加载未删除分类。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    ///
    /// # 返回
    /// 返回分类实体。
    ///
    /// # 错误
    /// 分类不存在时返回 `NotFound`。
    pub(super) async fn load_category(&self, id: &str) -> Result<ProductCategory> {
        self.db
            .product_categories()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品分类不存在".to_string()))
    }

    /// 按 ID 加载未删除品牌。
    ///
    /// # 参数
    /// * `id` - 品牌 ID
    ///
    /// # 返回
    /// 返回品牌实体。
    ///
    /// # 错误
    /// 品牌不存在时返回 `NotFound`。
    pub(super) async fn load_brand(&self, id: &str) -> Result<ProductBrand> {
        self.db
            .product_brands()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品品牌不存在".to_string()))
    }

    /// 按 ID 加载未删除计量单位。
    ///
    /// # 参数
    /// * `id` - 单位 ID
    ///
    /// # 返回
    /// 返回单位实体。
    ///
    /// # 错误
    /// 单位不存在时返回 `NotFound`。
    pub(super) async fn load_unit(&self, id: &str) -> Result<UnitOfMeasure> {
        self.db
            .unit_of_measures()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("计量单位不存在".to_string()))
    }

    /// 按 ID 加载未删除商品。
    ///
    /// # 参数
    /// * `id` - 商品 ID
    ///
    /// # 返回
    /// 返回商品实体。
    ///
    /// # 错误
    /// 商品不存在时返回 `NotFound`。
    pub(super) async fn load_product(&self, id: &str) -> Result<Product> {
        self.db
            .products()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品不存在".to_string()))
    }

    /// 校验品牌存在、每个基础单位存在且启用（不要求分类已落库，供卡券类目
    /// 原子创建复用——此时分类可能是本次事务内才新建的草稿）。
    ///
    /// # 参数
    /// * `brand_id` - ERP 品牌
    /// * `base_unit_ids` - 待校验的基础单位 ID 集合
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 品牌/单位不存在或单位已停用时返回错误。
    pub(super) async fn ensure_brand_and_unit_ok<'a>(
        &self,
        brand_id: &ProductBrandId,
        base_unit_ids: impl Iterator<Item = &'a UnitOfMeasureId>,
    ) -> Result<()> {
        self.load_brand(brand_id.as_ref()).await?;
        for unit_id in base_unit_ids {
            let unit = self.load_unit(unit_id.as_ref()).await?;
            if !unit.is_active() {
                return Err(Error::BusinessLogicError("基础单位已停用".to_string()));
            }
        }
        Ok(())
    }
}

/// 校验期望版本与当前版本一致（乐观锁语义）。
///
/// # 参数
/// * `current` - 当前版本
/// * `expected` - 期望版本
///
/// # 返回
/// 一致时返回 `Ok(())`。
///
/// # 错误
/// 不一致时返回 `ConflictError`（HTTP 409）。
pub(super) fn ensure_version(current: u64, expected: u64) -> Result<()> {
    if current != expected {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}
