//! 本供应商定向开放目录的允许列表读取入口。

use application_core::PageView;
use erp_catalog::{ListingStatus, ProductKind, SpecificationSignatureRead, read_specification_signature};
use erp_identity::PortalActor;
use erp_supply::portal::QuoteTargetVersion;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::repository::catalog::catalog_page;
use super::repository::query::PortalQuery;
use super::{PortalListParams, SupplierPortalReadService};
use crate::Result;

/// 仅公开该供应商有报价资格的 SKU 展示资料。
#[derive(Debug, Serialize, Deserialize)]
pub struct PortalCatalogSku {
    pub id: String,
    pub sku_no: String,
    pub name: String,
    pub specification: Option<String>,
    #[serde(skip_serializing, default)]
    pub(crate) specification_signature: String,
    pub unit_id: String,
    pub unit_name: String,
    pub unit_precision: u8,
    pub image_asset_id: Option<String>,
    #[serde(skip_serializing, default)]
    pub(crate) product_image_asset_id: Option<String>,
    pub product_kind: ProductKind,
    pub version: u64,
    pub product_id: String,
    pub target_version: QuoteTargetVersion,
    pub listing_status: ListingStatus,
    pub own_offering_id: Option<String>,
}

impl PortalCatalogSku {
    /// 将当前规范规格签名和已核对当前商品图映射成外部展示字段。
    /// # 参数
    /// `self` 为仓储沿精确当前修订读取的允许列表事实。
    /// # 返回
    /// 返回可区分规格、优先使用SKU图片的外部视图。
    /// # 错误
    /// 无；非法历史规格保持空显示，不使用旧修订规格兜底。
    pub(super) fn freeze_current(mut self) -> Self {
        self.specification = Self::specification_text(&self.specification_signature);
        self.image_asset_id = self.image_asset_id.or(self.product_image_asset_id.take());
        self
    }

    /// 复用正式目录解析器；空规格或非法历史签名不冒充其他规格。
    /// # 参数
    /// `signature` 为稳定SKU保存的规范规格签名。
    /// # 返回
    /// 返回规范顺序的规格名和值；空规格或非法签名返回空。
    /// # 错误
    /// 无；解析政策由正式目录拥有领域提供。
    pub(crate) fn specification_text(signature: &str) -> Option<String> {
        let SpecificationSignatureRead::Canonical(entries) = read_specification_signature(signature) else {
            return None;
        };
        (!entries.is_empty()).then(|| {
            entries
                .into_iter()
                .map(|entry| format!("{}：{}", entry.attribute_code, entry.value_code))
                .collect::<Vec<_>>()
                .join(" / ")
        })
    }
}

impl SupplierPortalReadService {
    /// 读取当前供应商定向开放且仍有效的公司 SKU。
    /// # 参数
    /// `actor` 为可信供应商绑定；`params` 仅收窄开放目录。
    /// # 返回
    /// 同一资格过滤下的分页及总数，不包含销售价格或其他供应商。
    /// # 错误
    /// 分页非法、供应商失效或事实查询失败时拒绝。
    pub async fn catalog(
        &self,
        actor: &PortalActor,
        params: &PortalListParams,
    ) -> Result<PageView<PortalCatalogSku>> {
        self.portal_supplier(actor, &mut NoTransaction).await?;
        let query = PortalQuery::new(params)?;
        let (items, total) = catalog_page(&self.db, &actor.supplier_id, &query, &mut NoTransaction).await?;
        Ok(PageView { items, total, page: query.page, page_size: query.page_size })
    }
}

#[cfg(test)]
mod tests {
    use erp_catalog::EMPTY_SPEC_SIGNATURE;

    use super::{PortalCatalogSku, ProductKind};

    #[test]
    fn external_sku_projection_excludes_sales_prices_and_foreign_supplier_data() {
        let row: PortalCatalogSku = serde_json::from_value(serde_json::json!({
            "id":"sku1","sku_no":"S1","name":"茶杯","specification":"350ml",
            "unit_id":"unit1","unit_name":"个","unit_precision":0,"image_asset_id":null,
            "product_kind":"PHYSICAL","version":2,"product_id":"product1","listing_status":"unlisted","own_offering_id":"own1",
            "target_version":{"sku_version":2,"sku_revision_id":"sr1","sku_revision_version":1,"product_id":"product1","product_version":2,"product_revision_id":"pr1","product_revision_version":1,"unit_id":"unit1","unit_version":1},
            "sales_visible_price_gross":"100","other_supplier_price":"1","business_org_unit_id":"internal"
        })).unwrap();
        let public = serde_json::to_value(row).unwrap();
        assert_eq!(public["name"], "茶杯");
        assert_eq!(public["product_kind"], serde_json::to_value(ProductKind::Physical).unwrap());
        assert!(public.get("sales_visible_price_gross").is_none());
        assert!(public.get("other_supplier_price").is_none());
        assert!(public.get("business_org_unit_id").is_none());
    }
    #[test]
    fn canonical_specifications_distinguish_same_name_skus_and_empty_signature_has_no_display() {
        assert_eq!(PortalCatalogSku::specification_text("容量=350ml"), Some("容量：350ml".into()));
        assert_eq!(PortalCatalogSku::specification_text("容量=500ml"), Some("容量：500ml".into()));
        assert_eq!(PortalCatalogSku::specification_text(EMPTY_SPEC_SIGNATURE), None);
        assert_eq!(PortalCatalogSku::specification_text("不是规范签名"), None);
    }

    #[test]
    fn current_product_image_fills_missing_sku_image_but_never_replaces_an_explicit_sku_image() {
        fn row(sku_image: Option<&str>) -> PortalCatalogSku {
            serde_json::from_value(serde_json::json!({
                "id":"sku1","sku_no":"S1","name":"茶杯","specification":null,"specification_signature":"容量=350ml",
                "unit_id":"unit1","unit_name":"个","unit_precision":0,"image_asset_id":sku_image,
                "product_image_asset_id":"product-main","product_kind":"PHYSICAL","version":2,
                "product_id":"product1","listing_status":"unlisted","own_offering_id":null,
                "target_version":{"sku_version":2,"sku_revision_id":"sr1","sku_revision_version":1,"product_id":"product1","product_version":2,"product_revision_id":"pr1","product_revision_version":1,"unit_id":"unit1","unit_version":1}
            })).unwrap()
        }
        let fallback = row(None).freeze_current();
        assert_eq!(fallback.image_asset_id.as_deref(), Some("product-main"));
        assert_eq!(fallback.specification.as_deref(), Some("容量：350ml"));
        let preferred = row(Some("sku-main")).freeze_current();
        assert_eq!(preferred.image_asset_id.as_deref(), Some("sku-main"));
        let serialized = serde_json::to_value(preferred).unwrap();
        assert!(serialized.get("product_image_asset_id").is_none());
        assert!(serialized.get("specification_signature").is_none());
        assert_eq!(serialized["target_version"]["product_revision_id"], "pr1");
    }
}
