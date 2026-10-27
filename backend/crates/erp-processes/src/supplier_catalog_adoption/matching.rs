//! 自动匹配的跨域读取与提交重验；身份规则由商品领域执行。

use application_core::AuditActor;
use erp_catalog::entity::catalog::auto_match::{MatchIdentity, match_text, same_specification, unique_match};
use erp_catalog::{CatalogExt, Product, ProductKind, Sku};
use erp_core::common::time::Instant;
use erp_supply::catalog_adoption::{
    AdoptionMapping, AdoptionRow, AdoptionSpecEntry, AdoptionTarget, CatalogAdoptionExt,
    ResolveAdoptionRequest, has_product_receipt,
};
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::{Executor, NoTransaction};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::SupplierCatalogAdoptionProcess;
use super::product::product_binding_id;
use crate::adapters::{catalog_access, catalog_service};
use crate::{Error, Result};

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MatchKind {
    Existing,
    CreateSku,
    CreateProduct,
}

#[derive(Serialize)]
pub struct AdoptionResolution {
    pub kind: MatchKind,
    pub expected_match: String,
    pub sku_no: Option<String>,
    pub product_id: Option<String>,
    pub reason: String,
    #[serde(skip)]
    pub(super) target: AdoptionTarget,
}

impl AdoptionResolution {
    fn existing(sku: Sku, product: Product, unit_id: String) -> Self {
        Self {
            kind: MatchKind::Existing,
            expected_match: String::new(),
            sku_no: Some(sku.sku_no),
            product_id: Some(product.base.id),
            reason: String::new(),
            target: AdoptionTarget::Existing {
                sku_id: sku.base.id,
                expected_sku_version: sku.base.version,
                expected_product_version: product.base.version,
                unit_id,
                specification_confirmed: true,
            },
        }
    }
}

impl SupplierCatalogAdoptionProcess {
    /// 在冻结预览内自动解析目标，不接受客户端指定 SKU。
    /// # 参数
    /// 连接、预览、建档映射及认证操作人。
    /// # 返回
    /// 将关联或创建的结果和提交重验摘要。
    /// # 错误
    /// 越权、过期、身份冲突、资料不足或重复候选时拒绝。
    pub async fn resolve(
        &self,
        connection: &str,
        preview_id: &str,
        request: ResolveAdoptionRequest,
        actor: &AuditActor,
    ) -> Result<AdoptionResolution> {
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        self.ensure_permission(actor, "supplier_offering_cost:detail").await?;
        let preview = self
            .db
            .catalog_adoption_previews()
            .find_by_id(preview_id, &mut NoTransaction)
            .await?
            .filter(|p| p.connection_id == connection && p.actor_id == actor.id())
            .ok_or_else(|| Error::NotFound("选品预览不存在".into()))?;
        if preview.expires_at <= Instant::now() {
            return Err(Error::ConflictError("选品预览已过期，请重新预览".into()));
        }
        let item = preview
            .items
            .iter()
            .find(|i| i.base.id == request.item_id)
            .ok_or_else(|| Error::ValidationError("商品不属于本次选品范围".into()))?;
        self.resolve_item(item, &request.mapping, actor, &mut NoTransaction).await
    }

    pub(super) async fn automatic_row(
        &self,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        actor: &AuditActor,
        command: &str,
        executor: &mut dyn Executor,
    ) -> Result<AdoptionRow> {
        let AdoptionTarget::Auto { mapping, expected_match } = &row.target else {
            return Err(Error::ValidationError("选品已改为自动匹配，请刷新页面后重新预览".into()));
        };
        let resolution = self.resolve_item(item, mapping, actor, executor).await?;
        if resolution.expected_match != *expected_match
            && !self.created_in_batch(item, row, &resolution, command, actor, executor).await?
        {
            return Err(Error::ConflictError("商品匹配结果已变化，请刷新自动匹配结果后重新确认".into()));
        }
        let mut resolved = row.clone();
        resolved.target = resolution.target;
        Ok(resolved)
    }

    async fn created_in_batch(
        &self,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        current: &AdoptionResolution,
        command: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let AdoptionTarget::Auto { mapping, expected_match: expected } = &row.target else {
            return Ok(false);
        };
        let Some(product_id) = current.product_id.as_deref() else { return Ok(false) };
        if !has_product_receipt(&self.db, command, product_id, executor).await? {
            return Ok(false);
        }
        let kind = serde_json::from_value(serde_json::Value::String(mapping.product_kind.clone()))
            .map_err(|_| Error::ValidationError("商品类型无效".into()))?;
        let mapping = self.resolve_unit_mapping(item, mapping, actor, executor).await?;
        let original = self.create_resolution(item, &mapping, None, kind, actor, executor).await?;
        Ok(resolution_digest(item, &mapping, &original)? == *expected)
    }

    async fn resolve_item(
        &self,
        item: &SupplierCatalogItem,
        mapping: &AdoptionMapping,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AdoptionResolution> {
        let connection =
            SupplierApiService::new(self.db.clone()).load_connection(&item.connection_id, executor).await?;
        self.ensure_source(&connection)?;
        connection.ensure_offering_source(&item.supplier_id)?;
        let kind: ProductKind =
            serde_json::from_value(serde_json::Value::String(mapping.product_kind.clone()))
                .map_err(|_| Error::ValidationError("商品类型无效".into()))?;
        if kind == ProductKind::Voucher
            || item.specification.trim().is_empty()
            || item.requires_identity_review
        {
            return Err(Error::ValidationError(
                "来源规格缺失、发生变化或商品类型不支持，请先核对来源商品".into(),
            ));
        }
        let mapping = self.resolve_unit_mapping(item, mapping, actor, executor).await?;
        let mapping = &mapping;
        let binding =
            self.db.catalog_adoption_products().find_by_id(&product_binding_id(item), executor).await?;
        let bound_id = binding.as_ref().map(|b| b.product_id.as_str());
        let (sku, reason) = self.find_match(item, mapping, bound_id, kind, executor).await?;
        let mut resolution = if let Some(sku) = sku {
            if bound_id.is_some_and(|id| id != sku.product_id.as_ref()) {
                return Err(Error::ConflictError(
                    "匹配 SKU 与既有来源商品归属不一致，请先核对商品档案".into(),
                ));
            }
            self.existing_resolution(item, mapping, sku, kind, actor, executor).await?
        } else {
            self.create_resolution(item, mapping, bound_id, kind, actor, executor).await?
        };
        resolution.reason = reason;
        resolution.expected_match = resolution_digest(item, mapping, &resolution)?;
        Ok(resolution)
    }

    async fn resolve_unit_mapping(
        &self,
        item: &SupplierCatalogItem,
        mapping: &AdoptionMapping,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AdoptionMapping> {
        let unit = if mapping.unit_id.is_empty() {
            let name = item
                .suggested_unit()
                .ok_or_else(|| Error::ValidationError("无法确定商品单位，请选择公司计量单位".into()))?;
            catalog_service(self.db.clone()).source_unit(&name, actor, executor).await?
        } else {
            self.db
                .unit_of_measures()
                .find_by_id(&mapping.unit_id, executor)
                .await?
                .filter(|u| u.is_active() && !u.base.is_deleted())
                .ok_or_else(|| Error::ValidationError("请选择启用的公司计量单位".into()))?
        };
        if let Some(source) = item.unit.as_deref().filter(|u| !u.trim().is_empty())
            && ![unit.name.as_str(), unit.symbol.as_str(), unit.unit_code.as_str()]
                .iter()
                .any(|v| match_text(v) == match_text(source))
        {
            return Err(Error::ValidationError(
                "来源单位与公司单位不一致，不支持自动换算，请核对单位字典".into(),
            ));
        }
        let mut mapping = mapping.clone();
        mapping.unit_id = unit.base.id;
        Ok(mapping)
    }

    async fn find_match(
        &self,
        item: &SupplierCatalogItem,
        mapping: &AdoptionMapping,
        bound_id: Option<&str>,
        kind: ProductKind,
        executor: &mut dyn Executor,
    ) -> Result<(Option<Sku>, String)> {
        if let Some(id) = &item.sku_id {
            return Ok((Some(self.match_sku(id, executor).await?), "复用已有来源绑定".into()));
        }
        if let Some(barcode) = &item.barcode {
            let ids = self.db.catalog().match_barcode(barcode, executor).await?;
            if let Some(id) = unique_match(&ids)? {
                return Ok((Some(self.match_sku(&id, executor).await?), "条码、规格和单位一致".into()));
            }
        }
        let products = if let Some(id) = bound_id {
            vec![
                self.db
                    .products()
                    .find_by_id(id, executor)
                    .await?
                    .ok_or_else(|| Error::ConflictError("来源绑定的公司商品不存在".into()))?,
            ]
        } else if item.brand_name.as_deref().is_some_and(|b| !b.trim().is_empty() && b.trim() != "无品牌")
        {
            self.db.catalog().match_products(&item.name, executor).await?
        } else {
            vec![]
        };
        let mut candidates = Vec::new();
        for product in products {
            if product.product_kind != kind || !self.same_brand(item, &product, executor).await? {
                continue;
            }
            for sku in self.db.catalog().match_skus(&product.base.id, executor).await? {
                if same_specification(&item.specification, &sku.specification_signature)?
                    && sku.base_unit_id.as_ref() == mapping.unit_id
                {
                    candidates.push(sku.base.id);
                }
            }
        }
        let sku = match unique_match(&candidates)? {
            Some(id) => Some(self.match_sku(&id, executor).await?),
            None => None,
        };
        Ok((
            sku,
            if bound_id.is_some() {
                "同一来源商品内按规格和单位匹配"
            } else {
                "按品牌、完整名称、规格和单位精确匹配；无匹配时新建"
            }
            .into(),
        ))
    }

    async fn match_sku(&self, id: &str, executor: &mut dyn Executor) -> Result<Sku> {
        self.db.skus().find_by_id(id, executor).await?.ok_or_else(|| {
            Error::ConflictError("已绑定或已占用条码的公司 SKU 不存在，请先核对商品档案".into())
        })
    }

    async fn same_brand(
        &self,
        item: &SupplierCatalogItem,
        product: &Product,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let revision = self
            .db
            .catalog()
            .current_product_revision(product, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司商品缺少当前资料".into()))?;
        let brand = self
            .db
            .product_brands()
            .find_by_id(revision.brand_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司品牌不存在".into()))?;
        Ok(item
            .brand_name
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .is_none_or(|source| match_text(source) == match_text(&brand.name)))
    }

    async fn existing_resolution(
        &self,
        item: &SupplierCatalogItem,
        mapping: &AdoptionMapping,
        sku: Sku,
        kind: ProductKind,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AdoptionResolution> {
        let product = catalog_access(self.db.clone(), self.rbac.clone())
            .require_product(actor, "update", sku.product_id.as_ref(), executor)
            .await?;
        let revision = self
            .db
            .catalog()
            .current_sku_revision(&sku, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司 SKU 缺少当前资料".into()))?;
        let product_revision = self
            .db
            .catalog()
            .current_product_revision(&product, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司商品缺少当前资料".into()))?;
        let brand = self
            .db
            .product_brands()
            .find_by_id(product_revision.brand_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司品牌不存在".into()))?;
        let source_identity = MatchIdentity {
            kind,
            brand: item.brand_name.as_deref(),
            specification: &item.specification,
            unit: &mapping.unit_id,
            barcode: item.barcode.as_deref(),
        };
        let company_identity = MatchIdentity {
            kind: product.product_kind,
            brand: Some(&brand.name),
            specification: &sku.specification_signature,
            unit: sku.base_unit_id.as_ref(),
            barcode: revision.barcode.as_deref(),
        };
        source_identity
            .ensure_matches(&company_identity, sku.is_active() && product.is_active() && brand.is_active())?;
        Ok(AdoptionResolution::existing(sku, product, mapping.unit_id.clone()))
    }

    async fn create_resolution(
        &self,
        item: &SupplierCatalogItem,
        mapping: &AdoptionMapping,
        bound_id: Option<&str>,
        kind: ProductKind,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AdoptionResolution> {
        let mut category_id = mapping.category_id.clone();
        let mut brand_id = String::new();
        if let Some(id) = bound_id {
            let product = catalog_access(self.db.clone(), self.rbac.clone())
                .require_product(actor, "update", id, executor)
                .await?;
            if !product.is_active()
                || product.product_kind != kind
                || !self.same_brand(item, &product, executor).await?
            {
                return Err(Error::ConflictError("来源绑定的公司商品已停用或类型、品牌不一致".into()));
            }
            let revision = self
                .db
                .catalog()
                .current_product_revision(&product, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("公司商品缺少当前资料".into()))?;
            category_id = revision.category_id.to_string();
            brand_id = revision.brand_id.to_string();
        }
        self.db
            .product_categories()
            .find_by_id(&category_id, executor)
            .await?
            .filter(|category| category.is_active() && category.product_kind == kind)
            .ok_or_else(|| {
                Error::ValidationError("没有匹配的 SKU，请选择适用于当前商品类型的启用公司分类".into())
            })?;
        Ok(AdoptionResolution {
            kind: if bound_id.is_some() { MatchKind::CreateSku } else { MatchKind::CreateProduct },
            expected_match: String::new(),
            sku_no: None,
            product_id: bound_id.map(str::to_owned),
            reason: String::new(),
            target: AdoptionTarget::Create {
                category_id,
                brand_id,
                unit_id: mapping.unit_id.clone(),
                product_kind: mapping.product_kind.clone(),
                company_name: mapping.company_name.clone(),
                company_description: mapping.company_description.clone(),
                spec_entries: vec![AdoptionSpecEntry {
                    attribute_code: "规格".into(),
                    attribute_value_code: item.specification.clone(),
                }],
                unit_confirmed: true,
                specification_confirmed: true,
            },
        })
    }
}

fn resolution_digest(
    item: &SupplierCatalogItem,
    mapping: &AdoptionMapping,
    resolution: &AdoptionResolution,
) -> Result<String> {
    let encoded = serde_json::to_vec(&(
        &item.base.id,
        (&item.name, &item.specification, &item.unit, &item.brand_name, &item.barcode),
        mapping,
        &resolution.kind,
        &resolution.product_id,
        &resolution.target,
    ))
    .map_err(|_| Error::Internal("匹配结果无法编码".into()))?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

#[cfg(test)]
mod tests {
    use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use erp_supply::ports::connector::offer::{Offer, OfferDetails};

    use super::*;

    #[test]
    fn reviewed_result_tracks_identity_mapping_and_versions_but_not_background_media() {
        let mut item = SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "job",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(1),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("product".into()), spec_id: "spec".into() },
                    name: "牛奶".into(),
                    specification: "250ml×12盒".into(),
                    unit: Some("箱".into()),
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap();
        let mut mapping = AdoptionMapping {
            category_id: "cat".into(),
            unit_id: "unit".into(),
            product_kind: "PHYSICAL".into(),
            company_name: None,
            company_description: None,
        };
        let mut resolution = AdoptionResolution {
            kind: MatchKind::Existing,
            expected_match: String::new(),
            sku_no: Some("SKU-1".into()),
            product_id: Some("company".into()),
            reason: String::new(),
            target: AdoptionTarget::Existing {
                sku_id: "sku".into(),
                expected_sku_version: 1,
                expected_product_version: 1,
                unit_id: "unit".into(),
                specification_confirmed: true,
            },
        };
        let original = resolution_digest(&item, &mapping, &resolution).unwrap();
        item.source_fingerprint = "background-image-archive".into();
        assert_eq!(original, resolution_digest(&item, &mapping, &resolution).unwrap());
        item.barcode = Some("00123".into());
        assert_ne!(original, resolution_digest(&item, &mapping, &resolution).unwrap());
        item.barcode = None;
        mapping.unit_id = "other".into();
        assert_ne!(original, resolution_digest(&item, &mapping, &resolution).unwrap());
        mapping.unit_id = "unit".into();
        if let AdoptionTarget::Existing { expected_sku_version, .. } = &mut resolution.target {
            *expected_sku_version = 2;
        }
        assert_ne!(original, resolution_digest(&item, &mapping, &resolution).unwrap());
    }
}
