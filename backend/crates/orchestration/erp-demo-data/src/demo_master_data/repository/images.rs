//! 读取演示商品当前修订，构造保留人工维护值的补图输入。
use erp_catalog::entity::catalog::Product;
use erp_catalog::{CatalogExt, UpdateProductRequest};
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::super::image_request;
use crate::{Error, Result};

/// 读取商品的当前媒体与启用 SKU。
/// # 参数
/// `product` 已由调用方完成更新权限校验；所有读取使用 `executor`。
/// # 返回
/// 保留当前字段与修订号的正式更新请求。
/// # 错误
/// 修订缺失、读取失败或规格签名非法时拒绝补图。
pub(in crate::demo_master_data) async fn update_request(
    db: &Database,
    product: &Product,
    executor: &mut dyn Executor,
) -> Result<UpdateProductRequest> {
    let revision_id = product
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::NotFound("演示商品缺少当前修订".into()))?;
    let revision = db
        .product_revisions()
        .find_by_id(revision_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("演示商品当前修订不存在".into()))?;
    let media =
        db.product_revision_medias().find_many(doc! {"product_revision_id": revision_id}, executor).await?;
    let skus = db.skus().find_many(doc! {"product_id": product.base.id.as_str()}, executor).await?;
    let mut inputs = Vec::new();
    for sku in skus.into_iter().filter(|sku| sku.is_active()) {
        let id = sku
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::NotFound("演示 SKU 缺少当前修订".into()))?;
        let current = db
            .sku_revisions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("演示 SKU 当前修订不存在".into()))?;
        inputs.push(image_request::sku_input(&sku, &current)?);
    }
    Ok(image_request::product_request(product, &revision, &media, inputs))
}
