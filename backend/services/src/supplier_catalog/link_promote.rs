//! 关联入池：已有公司 SKU 时只写映射 Active + 双价供给，不改公司主档价格。

use std::collections::HashSet;
use std::str::FromStr;

use database::{AccessControlExt, CatalogExt, NoTransaction, SupplierCatalogExt, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    SkuId, SupplierCatalogProductId, SupplierCatalogSkuId, SupplierOfferingId, SupplierProductMappingId,
};
use entities::money::Amount;
use entities::supplier_catalog::{
    AvailabilityStatus, MappingStatus, SupplierOffering, SupplierOfferingData, SupplierProductMapping,
    SupplierProductMappingData,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{
    LinkPromoteSkuItem, LinkPromoteSkuResult, LinkPromoteToCompanyPoolRequest, LinkPromoteToCompanyPoolResult,
};
use super::{build_command, command_fingerprint, replay_command, SupplierCatalogService};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SupplierCatalogService {
    /// 关联入池：将供应商 SKU 映射到已有公司 SKU，并原子登记双价供给。
    ///
    /// 不修改公司 `sku_revision` 销售可见价/市场价；起订量取自供应商目录 SKU。
    /// 确认即生效，不接受用户填写的供给生效日期。
    ///
    /// # 参数
    /// * `req` - 关联入池请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回各行映射与供给标识。
    ///
    /// # 错误
    /// * `ValidationError` - 参数非法、缺底价/起订量
    /// * `NotFound` - 供应商商品/SKU 或公司 SKU 不存在
    /// * `ConflictError` - 来源版本变化、已有生效映射
    pub async fn link_promote_to_company_pool(
        &self,
        req: LinkPromoteToCompanyPoolRequest,
        actor: &AuditActor,
    ) -> Result<LinkPromoteToCompanyPoolResult> {
        req.validate()?;
        ensure_unique_link_items(&req.items)?;
        let request_fingerprint = command_fingerprint("link_promote", &req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "link_promote", &request_fingerprint);
        }

        let product_id = SupplierCatalogProductId::new(req.supplier_product_id.clone());
        let supplier_product = self
            .db
            .supplier_catalog_products()
            .find_by_id(&product_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商商品不存在".to_string()))?;

        let source_no = self.current_product_revision_no(&product_id).await?.unwrap_or(0);
        if source_no != req.expected_source_revision_no {
            return Err(Error::ConflictError(format!(
                "供应商商品来源版本已变化，请刷新后重试（期望 SPU 修订 {}, 当前 {}）",
                req.expected_source_revision_no, source_no
            )));
        }

        let sku_ids: Vec<SupplierCatalogSkuId> = req
            .items
            .iter()
            .map(|item| SupplierCatalogSkuId::new(item.supplier_catalog_sku_id.clone()))
            .collect();
        let supplier_skus = self.load_product_skus(&product_id, &sku_ids).await?;
        let sku_revisions = self.current_sku_revisions(&sku_ids).await?;
        let effective_from = BusinessDate::today();

        let mut prepared_rows = Vec::with_capacity(req.items.len());
        for item in &req.items {
            let supplier_sku = supplier_skus
                .get(item.supplier_catalog_sku_id.as_str())
                .ok_or_else(|| {
                    Error::NotFound(format!("供应商 SKU 不存在: {}", item.supplier_catalog_sku_id))
                })?;
            if self
                .db
                .supplier_product_mappings()
                .find_active_by_supplier_sku(&supplier_sku.base.id.clone().into(), &mut NoTransaction)
                .await?
                .is_some()
            {
                return Err(Error::ConflictError(format!(
                    "供应商 SKU 已有生效映射: {}",
                    item.supplier_catalog_sku_id
                )));
            }

            let company_sku_id = SkuId::new(item.company_sku_id.clone());
            let company_sku = self
                .db
                .skus()
                .find_by_id(company_sku_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound(format!("公司 SKU 不存在: {}", item.company_sku_id)))?;
            self.ensure_qualified_for_sku(
                &supplier_product.supplier_id,
                &company_sku.base.id.clone().into(),
                effective_from,
            )
            .await?;

            let source_sku_rev = sku_revisions
                .get(item.supplier_catalog_sku_id.as_str())
                .and_then(|v| v.as_ref())
                .ok_or_else(|| {
                    Error::NotFound(format!(
                        "供应商 SKU 来源修订不存在: {}",
                        item.supplier_catalog_sku_id
                    ))
                })?;
            let moq = source_sku_rev.bulk_minimum_order_quantity.ok_or_else(|| {
                Error::ValidationError(format!(
                    "供应商 SKU 缺少集采起订量，请先在商品中心补齐: {}",
                    item.supplier_catalog_sku_id
                ))
            })?;
            let dropship = resolve_price(
                item.dropship_supply_price_gross.as_deref(),
                source_sku_rev.dropship_floor_price_gross,
                "一件代发供给价",
                &item.supplier_catalog_sku_id,
            )?;
            let bulk = resolve_price(
                item.bulk_supply_price_gross.as_deref(),
                source_sku_rev.bulk_floor_price_gross,
                "集采供给价",
                &item.supplier_catalog_sku_id,
            )?;

            let mapping = SupplierProductMapping::new(
                SupplierProductMappingId::new(next_id()),
                SupplierProductMappingData {
                    supplier_catalog_sku_id: supplier_sku.base.id.clone().into(),
                    sku_id: company_sku.base.id.clone().into(),
                    status: MappingStatus::Active,
                    approved_by: Some(actor.id().to_string()),
                    approved_at: Some(Instant::now()),
                    reason: Some("link_promote_to_pool".to_string()),
                },
            )?;

            let mut offering = SupplierOffering::new(
                SupplierOfferingId::new(next_id()),
                SupplierOfferingData {
                    sku_id: company_sku.base.id.clone().into(),
                    supplier_id: supplier_product.supplier_id.clone(),
                    supplier_catalog_sku_id: supplier_sku.base.id.clone().into(),
                },
                actor.id(),
            )?;
            let available_qty = source_sku_rev
                .available_quantity
                .as_ref()
                .map(ToString::to_string);
            let offering_revision = self.build_offering_revision(
                &offering,
                1,
                &dropship,
                &bulk,
                req.input_tax_rate.as_str(),
                &moq.to_string(),
                &req.supply_region,
                &[],
                &effective_from.to_string(),
                None,
                None,
                None,
                None,
                available_qty.as_deref(),
                AvailabilityStatus::Available,
            )?;
            offering.stable.current_revision_id = Some(offering_revision.base.id.clone());

            prepared_rows.push(LinkRow {
                supplier_catalog_sku_id: item.supplier_catalog_sku_id.clone(),
                company_sku_id: company_sku.base.id.clone(),
                mapping,
                offering,
                offering_revision,
            });
        }

        let supplier_product_id = supplier_product.base.id.clone();
        let audit = actor.clone().resource_log(
            "supplier_catalog.link_promote",
            "supplier_catalog_product",
            supplier_product_id.clone(),
        )?;
        let items: Vec<LinkPromoteSkuResult> = prepared_rows
            .iter()
            .map(|row| LinkPromoteSkuResult {
                supplier_catalog_sku_id: row.supplier_catalog_sku_id.clone(),
                company_sku_id: row.company_sku_id.clone(),
                mapping_id: row.mapping.base.id.clone(),
                offering_id: row.offering.base.id.clone(),
                offering_revision_no: 1,
            })
            .collect();
        let result = LinkPromoteToCompanyPoolResult {
            supplier_product_id: supplier_product_id.clone(),
            items,
            reference: format!(
                "LPRO-{}",
                &supplier_product_id[..8.min(supplier_product_id.len())]
            ),
            recorded_at: Instant::now().unix_secs() as u64,
        };
        let command = build_command(
            &req.idempotency_key,
            "link_promote",
            &request_fingerprint,
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let rows_for_tx = prepared_rows.clone();
        let command_for_tx = command.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    for row in &rows_for_tx {
                        db.supplier_product_mappings()
                            .create(&row.mapping, session)
                            .await?;
                        db.supplier_catalog()
                            .create_offering_with_revision(&row.offering, &row.offering_revision, session)
                            .await?;
                    }
                    db.supplier_catalog_commands()
                        .create(&command_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;

        self.resolve_command_result(
            transaction_result,
            result,
            &req.idempotency_key,
            "link_promote",
            &request_fingerprint,
        )
        .await
    }
}

/// 关联入池事务行。
#[derive(Debug, Clone)]
struct LinkRow {
    /// 供应商目录 SKU。
    supplier_catalog_sku_id: String,
    /// 公司 SKU。
    company_sku_id: String,
    /// 映射。
    mapping: SupplierProductMapping,
    /// 供给。
    offering: SupplierOffering,
    /// 供给首修订。
    offering_revision: entities::supplier_catalog::SupplierOfferingRevision,
}

/// 校验关联行供应商 SKU 不重复。
///
/// # 参数
/// * `items` - 关联行
///
/// # 返回
/// 无重复返回 `Ok(())`。
///
/// # 错误
/// 重复时返回 `ValidationError`。
fn ensure_unique_link_items(items: &[LinkPromoteSkuItem]) -> Result<()> {
    let mut seen = HashSet::new();
    for item in items {
        let id = item.supplier_catalog_sku_id.trim();
        if !seen.insert(id.to_string()) {
            return Err(Error::ValidationError(format!("供应商 SKU 重复选择: {id}")));
        }
    }
    Ok(())
}

/// 解析正式供给价：优先确认值，否则回退目录底价。
///
/// # 参数
/// * `confirmed` - 用户确认价
/// * `floor` - 目录底价
/// * `label` - 字段名
/// * `sku_id` - 供应商 SKU（错误文案）
///
/// # 返回
/// 金额字符串。
///
/// # 错误
/// 两边都缺失时返回 `ValidationError`。
fn resolve_price(
    confirmed: Option<&str>,
    floor: Option<Amount>,
    label: &str,
    sku_id: &str,
) -> Result<String> {
    if let Some(value) = confirmed.map(str::trim).filter(|value| !value.is_empty()) {
        Amount::from_str(value).map_err(|_| Error::ValidationError(format!("非法{label}: {value}")))?;
        return Ok(value.to_string());
    }
    floor.map(|value| value.to_string()).ok_or_else(|| {
        Error::ValidationError(format!(
            "{label}不能为空（目录底价缺失，请填写正式供给价）: {sku_id}"
        ))
    })
}
