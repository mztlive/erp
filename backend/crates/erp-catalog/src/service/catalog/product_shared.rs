use std::collections::HashSet;

use id_generator::next_id;
use persistence_core::NoTransaction;

use super::CatalogService;
use crate::dto::{ProductMediaInput, ProductSkuInput};
use crate::entity::catalog::product_revision_media::{
    MediaRole, ProductRevisionMedia, ProductRevisionMediaData, ensure_unique_media_sort_orders,
};
use crate::entity::catalog::{
    ProductBrandId, ProductCategoryId, ProductKind, ProductRevisionId, ProductRevisionMediaId, UnitOfMeasure,
    UnitOfMeasureId,
};
use crate::error::{Error, Result};
use crate::ports::PendingAttachmentBatch;
use crate::repository::CatalogExt;

impl CatalogService {
    /// 校验商品创建/编辑引用的字典（分类/品牌/基础单位）与分类-商品类型兼容性。
    ///
    /// # 参数
    /// * `category_id` - ERP 分类
    /// * `brand_id` - ERP 品牌
    /// * `skus` - SKU 行（校验每个基础单位存在且启用）
    /// * `product_kind` - 商品业务类型
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 字典不存在/停用或分类不允许商品类型时返回错误。
    pub(super) async fn ensure_product_dictionaries(
        &self,
        category_id: &ProductCategoryId,
        brand_id: &ProductBrandId,
        skus: &[ProductSkuInput],
        product_kind: ProductKind,
        pending_assets: &dyn PendingAttachmentBatch,
    ) -> Result<()> {
        let unit_ids = skus.iter().map(|sku| sku.base_unit_id.clone()).collect::<Vec<_>>();
        let references = self
            .db
            .catalog()
            .catalog_reference_data(Some(category_id), brand_id, &unit_ids, &mut NoTransaction)
            .await?;
        let category = references.category.ok_or_else(|| Error::NotFound("商品分类不存在".to_string()))?;
        if category.product_kind != product_kind {
            return Err(Error::BusinessLogicError("所选分类不允许该商品类型".to_string()));
        }
        if references.brand.is_none() {
            return Err(Error::NotFound("商品品牌不存在".to_string()));
        }
        ensure_units_available(&unit_ids, &references.units)?;
        let asset_ids = skus
            .iter()
            .filter_map(|sku| sku.main_image_asset_id.as_ref())
            .filter(|asset_id| !pending_assets.contains_id(asset_id))
            .cloned()
            .collect::<Vec<_>>();
        if !self.file_assets.missing_ids(&asset_ids, &mut NoTransaction).await?.is_empty() {
            return Err(Error::NotFound("SKU 主图文件不存在".to_string()));
        }
        Ok(())
    }

    /// 构造媒体行（校验 `file_asset` 引用存在，媒体角色与顺序落位）。
    ///
    /// # 参数
    /// * `revision_id` - 所属商品修订
    /// * `inputs` - 媒体输入
    /// * `role` - 媒体用途
    ///
    /// # 返回
    /// 返回媒体实体集合。
    ///
    /// # 错误
    /// 媒体文件不存在或同用途顺序重复时返回错误。
    pub(super) async fn build_media_rows(
        &self,
        revision_id: &ProductRevisionId,
        inputs: &[ProductMediaInput],
        role: MediaRole,
        pending_assets: &dyn PendingAttachmentBatch,
    ) -> Result<Vec<ProductRevisionMedia>> {
        let asset_ids = inputs
            .iter()
            .map(|input| &input.file_asset_id)
            .filter(|asset_id| !pending_assets.contains_id(asset_id))
            .cloned()
            .collect::<Vec<_>>();
        if !self.file_assets.missing_ids(&asset_ids, &mut NoTransaction).await?.is_empty() {
            return Err(Error::NotFound("媒体文件不存在".to_string()));
        }
        let rows = inputs
            .iter()
            .map(|input| {
                ProductRevisionMedia::new(
                    ProductRevisionMediaId::new(next_id()),
                    ProductRevisionMediaData {
                        product_revision_id: revision_id.clone(),
                        file_asset_id: input.file_asset_id.clone(),
                        media_role: role,
                        sort_order: input.sort_order,
                        alt_text: input.alt_text.clone(),
                    },
                )
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure_unique_media_sort_orders(&rows)?;
        Ok(rows)
    }
}

/// 解析商品命令中全部临时文件引用，并返回实际被引用的临时键集合。
pub(super) fn resolve_product_file_references(
    carousel_media: &mut [ProductMediaInput],
    detail_media: &mut [ProductMediaInput],
    skus: &mut [ProductSkuInput],
    pending_assets: &dyn PendingAttachmentBatch,
) -> Result<HashSet<String>> {
    let mut used = HashSet::new();
    for media in carousel_media.iter_mut().chain(detail_media.iter_mut()) {
        pending_assets.resolve_id(&mut media.file_asset_id, &mut used)?;
    }
    for sku in skus {
        if let Some(asset_id) = sku.main_image_asset_id.as_mut() {
            pending_assets.resolve_id(asset_id, &mut used)?;
        }
    }
    Ok(used)
}

/// 校验批量读取的基础单位完整且全部启用。
///
/// # 参数
/// * `expected_ids` - SKU 输入引用的基础单位 ID
/// * `units` - Repository 批量读取到的计量单位实体
///
/// # 返回
/// 每个引用均存在且启用时返回 `Ok(())`。
///
/// # 错误
/// 任一单位缺失时返回 `NotFound`，停用时返回业务逻辑错误。
pub(super) fn ensure_units_available(
    expected_ids: &[UnitOfMeasureId],
    units: &[UnitOfMeasure],
) -> Result<()> {
    for unit_id in expected_ids {
        let unit = units
            .iter()
            .find(|unit| unit.base.id == unit_id.as_ref())
            .ok_or_else(|| Error::NotFound("计量单位不存在".to_string()))?;
        if !unit.is_active() {
            return Err(Error::BusinessLogicError("基础单位已停用".to_string()));
        }
    }
    Ok(())
}
