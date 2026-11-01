//! 门户供给和开放目录共用精确当前图片来源，下载前后比较同一组正式版本。

use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use super::PortalCatalogSku;
use super::repository::catalog::current_sku;
use crate::Result;

/// 图片来自当前 SKU 图或其精确当前商品修订的公共轮播主图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortalSkuImageSource {
    pub file_id: String,
    pub sku_id: String,
    pub sku_version: u64,
    pub sku_revision_id: String,
    pub sku_revision_version: u64,
    pub product_id: String,
    pub product_version: u64,
    pub product_revision_id: String,
    pub product_revision_version: u64,
    pub unit_id: String,
    pub unit_version: u64,
}

/// 从启用的 SKU、商品、基础单位和各自精确当前修订解析唯一展示图。
///
/// # 参数
/// `db` 为组合根数据库；`sku_id` 必须先由调用方验证供给归属或开放资格；
/// `executor` 为本次授权读取执行器，事务内调用不得更换执行器。
/// # 返回
/// 返回当前图片和全部来源版本；没有图片或来源失效返回 `None`。
/// # 错误
/// 当前事实读取或解析失败时拒绝，不从其他修订回退。
pub async fn portal_sku_image_source(
    db: &Database,
    sku_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PortalSkuImageSource>> {
    Ok(current_sku(db, sku_id, executor).await?.and_then(PortalSkuImageSource::from_catalog))
}

impl PortalSkuImageSource {
    fn from_catalog(view: PortalCatalogSku) -> Option<Self> {
        let target = view.target_version;
        Some(Self {
            file_id: view.image_asset_id?,
            sku_id: view.id,
            sku_version: target.sku_version,
            sku_revision_id: target.sku_revision_id,
            sku_revision_version: target.sku_revision_version,
            product_id: target.product_id,
            product_version: target.product_version,
            product_revision_id: target.product_revision_id,
            product_revision_version: target.product_revision_version,
            unit_id: target.unit_id,
            unit_version: target.unit_version,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn current_view(image: Option<&str>) -> PortalCatalogSku {
        serde_json::from_value(json!({
            "id":"sku1","sku_no":"S1","name":"礼盒","specification":null,
            "unit_id":"unit1","unit_name":"盒","unit_precision":0,"image_asset_id":image,
            "product_kind":"PHYSICAL","version":2,"product_id":"product1",
            "listing_status":"unlisted","own_offering_id":null,
            "target_version":{"sku_version":2,"sku_revision_id":"sr1","sku_revision_version":3,
                "product_id":"product1","product_version":4,"product_revision_id":"pr1",
                "product_revision_version":5,"unit_id":"unit1","unit_version":6}
        }))
        .unwrap()
    }

    #[test]
    fn missing_current_image_has_no_authorized_source_and_source_freezes_every_current_basis() {
        assert!(PortalSkuImageSource::from_catalog(current_view(None)).is_none());
        let source = PortalSkuImageSource::from_catalog(current_view(Some("image-1"))).unwrap();
        assert_eq!(source.file_id, "image-1");
        assert_eq!((source.sku_version, source.sku_revision_version), (2, 3));
        assert_eq!((source.product_version, source.product_revision_version, source.unit_version), (4, 5, 6));
        assert_eq!((source.sku_revision_id.as_str(), source.product_revision_id.as_str()), ("sr1", "pr1"));
        let mut current = source.clone();
        current.product_revision_id = "new-current-revision".into();
        assert_ne!(source, current);
        current = source.clone();
        current.unit_version += 1;
        assert_ne!(source, current);
    }
}
