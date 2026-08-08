//! 反向入池用例：供应商 SPU 同构新建公司 Product/SKU，并原子写入映射与双价供给。
//!
//! 正式粒度仍是 `supplier_catalog_sku_id → sku_id`；页面以 SPU 为上下文，
//! 勾选 SKU 行后一次事务提交。确认即生效，不接受用户填写的供给生效日期。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use database::{AccessControlExt, CatalogExt, NoTransaction, SupplierCatalogExt, Transactional};
use entities::catalog::product::ProductData;
use entities::catalog::product_revision::ProductRevisionData;
use entities::catalog::sku::SkuData;
use entities::catalog::sku_revision::SkuRevisionData;
use entities::catalog::specification::{compute_specification_signature, SpecSignatureEntry};
use entities::catalog::{EnableStatus, Product, ProductKind, ProductRevision, Sku, SkuRevision};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    ProductBrandId, ProductCategoryId, ProductId, ProductRevisionId, SkuId, SkuRevisionId, SupplierAccountId,
    SupplierCatalogProductId, SupplierCatalogSkuId, SupplierOfferingId, SupplierProductMappingId,
    UnitOfMeasureId,
};
use entities::money::Amount;
use entities::supplier_catalog::{
    AvailabilityStatus, MappingStatus, SupplierCatalogSku, SupplierCatalogSkuRevision, SupplierOffering,
    SupplierOfferingData, SupplierOfferingRevision, SupplierProductMapping, SupplierProductMappingData,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{
    ReversePromoteSkuItem, ReversePromoteSkuResult, ReversePromoteToCompanyPoolRequest,
    ReversePromoteToCompanyPoolResult,
};
use super::{build_command, command_fingerprint, replay_command, SupplierCatalogService};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SupplierCatalogService {
    /// 反向入池：以供应商 SPU 为上下文，同构新建公司商品与勾选 SKU，
    /// 并原子写入精确映射与双价供给修订。
    ///
    /// 事务边界：公司 product/revision + 各 company sku/revision + 映射 Active +
    /// offering 首修订 + 审计，任一步失败整体回滚。
    ///
    /// # 参数
    /// * `req` - 反向入池请求（含 product_kind、字典 ID、税/区域、SKU 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建公司商品与各 SKU/映射/供给标识。
    ///
    /// # 错误
    /// * `ValidationError` - 参数非法、缺底价/起订量、价格非法
    /// * `NotFound` - 供应商商品/SKU/分类/品牌/单位不存在
    /// * `ConflictError` - 来源修订已变化、SKU 已有生效映射、规格签名冲突
    /// * `BusinessLogicError` - 分类与商品类型不兼容、单位停用
    pub async fn reverse_promote_to_company_pool(
        &self,
        req: ReversePromoteToCompanyPoolRequest,
        actor: &AuditActor,
    ) -> Result<ReversePromoteToCompanyPoolResult> {
        req.validate()?;
        self.ensure_unique_item_ids(&req.items)?;
        let request_fingerprint = command_fingerprint("reverse_promote", &req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "reverse_promote", &request_fingerprint);
        }

        let product_id = SupplierCatalogProductId::new(req.supplier_product_id.clone());
        let supplier_product = self
            .db
            .supplier_catalog_products()
            .find_by_id(&product_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商商品不存在".to_string()))?;

        let source_revision = self
            .current_product_revision(&product_id)
            .await?
            .ok_or_else(|| Error::NotFound("供应商商品来源修订不存在".to_string()))?;
        if source_revision.revision.revision_no != req.expected_source_revision_no {
            return Err(Error::ConflictError(format!(
                "供应商商品来源版本已变化，请刷新后重试（期望 SPU 修订 {}, 当前 {}）",
                req.expected_source_revision_no, source_revision.revision.revision_no
            )));
        }

        self.ensure_company_dictionaries(
            &req.category_id,
            &req.brand_id,
            &req.base_unit_id,
            req.product_kind,
        )
        .await?;
        self.ensure_qualified_for_product_kind(
            &supplier_product.supplier_id,
            req.product_kind,
            BusinessDate::today(),
        )
        .await?;

        let sku_ids: Vec<SupplierCatalogSkuId> = req
            .items
            .iter()
            .map(|item| SupplierCatalogSkuId::new(item.supplier_catalog_sku_id.clone()))
            .collect();
        let supplier_skus = self.load_product_skus(&product_id, &sku_ids).await?;
        let sku_revisions = self.current_sku_revisions(&sku_ids).await?;

        for sku_id in &sku_ids {
            if self
                .db
                .supplier_product_mappings()
                .find_active_by_supplier_sku(sku_id, &mut NoTransaction)
                .await?
                .is_some()
            {
                return Err(Error::ConflictError(format!("供应商 SKU 已有生效映射: {sku_id}")));
            }
        }

        let prepared = self.prepare_reverse_promote_rows(
            &req,
            &supplier_product.supplier_id,
            &source_revision.name,
            &source_revision.description,
            &supplier_skus,
            &sku_revisions,
            actor,
        )?;

        let company_product_id = prepared.product.base.id.clone();
        let product_no = prepared.product.product_no.clone();
        let product_kind = prepared.product.product_kind;
        let supplier_product_id = supplier_product.base.id.clone();
        let audit = actor.clone().resource_log(
            "supplier_catalog.reverse_promote",
            "supplier_catalog_product",
            supplier_product_id.clone(),
        )?;

        let items: Vec<ReversePromoteSkuResult> = prepared
            .rows
            .iter()
            .map(|row| ReversePromoteSkuResult {
                supplier_catalog_sku_id: row.supplier_catalog_sku_id.clone(),
                company_sku_id: row.sku.base.id.clone(),
                company_sku_revision_id: row.sku_revision.base.id.clone(),
                mapping_id: row.mapping.base.id.clone(),
                offering_id: row.offering.base.id.clone(),
                offering_revision_no: 1,
            })
            .collect();
        let reference = format!("RPRO-{}", &company_product_id[..8.min(company_product_id.len())]);
        let result = ReversePromoteToCompanyPoolResult {
            supplier_product_id,
            company_product_id,
            product_no,
            product_kind,
            items,
            reference,
            recorded_at: Instant::now().unix_secs() as u64,
        };
        let command = build_command(
            &req.idempotency_key,
            "reverse_promote",
            &request_fingerprint,
            &result,
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let prepared_for_tx = prepared.clone();
        let command_for_tx = command.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.products().create(&prepared_for_tx.product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&prepared_for_tx.product_revision, &[], session)
                        .await?;
                    for row in &prepared_for_tx.rows {
                        db.catalog()
                            .create_sku_with_revision(&row.sku, &row.sku_revision, &[], session)
                            .await?;
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
            "reverse_promote",
            &request_fingerprint,
        )
        .await
    }

    /// 校验入选项供应商 SKU ID 不重复。
    ///
    /// # 参数
    /// * `items` - 请求中的 SKU 行
    ///
    /// # 返回
    /// 无重复时返回 `Ok(())`。
    ///
    /// # 错误
    /// 出现重复 `supplier_catalog_sku_id` 时返回 `ValidationError`。
    fn ensure_unique_item_ids(&self, items: &[ReversePromoteSkuItem]) -> Result<()> {
        let mut seen = HashSet::new();
        for item in items {
            let id = item.supplier_catalog_sku_id.trim();
            if !seen.insert(id.to_string()) {
                return Err(Error::ValidationError(format!("供应商 SKU 重复选择: {id}")));
            }
        }
        Ok(())
    }

    /// 校验公司分类/品牌/单位存在，且分类与商品类型兼容、单位启用。
    ///
    /// # 参数
    /// * `category_id` - 分类 ID
    /// * `brand_id` - 品牌 ID
    /// * `base_unit_id` - 基础单位 ID
    /// * `product_kind` - 公司商品类型
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 字典缺失/停用或分类不允许该类型时返回错误。
    async fn ensure_company_dictionaries(
        &self,
        category_id: &str,
        brand_id: &str,
        base_unit_id: &str,
        product_kind: ProductKind,
    ) -> Result<()> {
        let category = self
            .db
            .product_categories()
            .find_by_id(category_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("分类不存在".to_string()))?;
        if category.product_kind != product_kind {
            return Err(Error::BusinessLogicError("所选分类不允许该商品类型".to_string()));
        }
        self.db
            .product_brands()
            .find_by_id(brand_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("品牌不存在".to_string()))?;
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(base_unit_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基础单位不存在".to_string()))?;
        if !unit.is_active() {
            return Err(Error::BusinessLogicError("基础单位已停用".to_string()));
        }
        Ok(())
    }

    /// 加载并校验入选 SKU 均属于指定供应商 SPU。
    ///
    /// # 参数
    /// * `product_id` - 供应商 SPU
    /// * `sku_ids` - 入选供应商 SKU
    ///
    /// # 返回
    /// 返回 `sku_id → 实体` 映射。
    ///
    /// # 错误
    /// SKU 不存在或不属于该 SPU 时返回错误。
    pub(super) async fn load_product_skus(
        &self,
        product_id: &SupplierCatalogProductId,
        sku_ids: &[SupplierCatalogSkuId],
    ) -> Result<HashMap<String, SupplierCatalogSku>> {
        let mut map = HashMap::new();
        for sku_id in sku_ids {
            let sku = self
                .db
                .supplier_catalog_skus()
                .find_by_id(sku_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound(format!("供应商 SKU 不存在: {sku_id}")))?;
            if &sku.supplier_catalog_product_id != product_id {
                return Err(Error::ValidationError(format!(
                    "供应商 SKU 不属于当前商品: {sku_id}"
                )));
            }
            map.insert(sku_id.to_string(), sku);
        }
        Ok(map)
    }

    /// 取回供应商 SPU 的当前来源修订。
    ///
    /// # 参数
    /// * `product_id` - 供应商 SPU
    ///
    /// # 返回
    /// 返回修订号最大的来源修订；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn current_product_revision(
        &self,
        product_id: &SupplierCatalogProductId,
    ) -> Result<Option<entities::supplier_catalog::SupplierCatalogProductRevision>> {
        let revisions = self
            .db
            .supplier_catalog_product_revisions()
            .find_many(
                mongodb::bson::doc! { "supplier_catalog_product_id": product_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        Ok(revisions
            .into_iter()
            .max_by_key(|revision| revision.revision.revision_no))
    }

    /// 预生成反向入池全部实体（事务外完成校验与 ID 分配）。
    ///
    /// # 参数
    /// * `req` - 请求
    /// * `supplier_id` - 供应商账户
    /// * `product_name` - 公司/供应商商品名称快照
    /// * `product_description` - 描述快照
    /// * `supplier_skus` - 供应商 SKU 实体
    /// * `sku_revisions` - 供应商 SKU 当前修订
    /// * `actor` - 操作人
    ///
    /// # 返回
    /// 返回待写入草稿。
    ///
    /// # 错误
    /// 缺起订量/底价、价格非法或规格签名冲突时返回错误。
    fn prepare_reverse_promote_rows(
        &self,
        req: &ReversePromoteToCompanyPoolRequest,
        supplier_id: &SupplierAccountId,
        product_name: &str,
        product_description: &Option<String>,
        supplier_skus: &HashMap<String, SupplierCatalogSku>,
        sku_revisions: &HashMap<String, Option<SupplierCatalogSkuRevision>>,
        actor: &AuditActor,
    ) -> Result<ReversePromoteDraft> {
        let company_product_id = ProductId::new(next_id());
        let product_no = req
            .product_no
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                let id = company_product_id.to_string();
                format!("P-{}", &id[..8.min(id.len())])
            });
        let mut product = Product::new(
            company_product_id.clone(),
            ProductData {
                product_no: product_no.clone(),
                product_kind: req.product_kind,
                status: EnableStatus::Active,
            },
            actor.id(),
        )?;
        let product_revision_id = ProductRevisionId::new(next_id());
        let effective_from = BusinessDate::today();
        let product_revision = ProductRevision::new(
            product_revision_id,
            ProductRevisionData {
                product_id: company_product_id.clone(),
                revision_no: 1,
                name: product_name.to_string(),
                description: product_description.clone(),
                specification: None,
                category_id: ProductCategoryId::new(req.category_id.clone()),
                brand_id: ProductBrandId::new(req.brand_id.clone()),
                status: EnableStatus::Active,
                effective_from,
                effective_to: None,
            },
        )?;
        product.stable.current_revision_id = Some(product_revision.base.id.clone());

        let mut rows = Vec::with_capacity(req.items.len());
        let mut signatures = HashSet::new();
        for item in &req.items {
            let supplier_sku = supplier_skus
                .get(item.supplier_catalog_sku_id.as_str())
                .ok_or_else(|| {
                    Error::NotFound(format!("供应商 SKU 不存在: {}", item.supplier_catalog_sku_id))
                })?;
            let source_sku_rev = sku_revisions
                .get(item.supplier_catalog_sku_id.as_str())
                .and_then(|value| value.as_ref())
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
            let sales_price = parse_amount_required(&item.sales_visible_price_gross, "销售可见价")?;
            let market_price = parse_amount_required(&item.market_price, "市场价")?;

            let signature =
                signature_for_supplier_sku(source_sku_rev, &supplier_sku.supplier_sku_code, req.items.len())?;
            if !signatures.insert(signature.clone()) {
                return Err(Error::ConflictError(format!(
                    "入选 SKU 规格签名冲突: {}",
                    item.supplier_catalog_sku_id
                )));
            }

            let company_sku_id = SkuId::new(next_id());
            let sku_id_str = company_sku_id.to_string();
            let sku_no = item
                .sku_no
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "SKU-{}",
                        sanitize_code(&supplier_sku.supplier_sku_code, &sku_id_str)
                    )
                });
            let mut sku = Sku::new(
                company_sku_id.clone(),
                SkuData {
                    sku_no,
                    product_id: company_product_id.clone(),
                    base_unit_id: UnitOfMeasureId::new(req.base_unit_id.clone()),
                    specification_signature: signature,
                    status: EnableStatus::Active,
                },
                actor.id(),
            )?;
            let sku_revision = SkuRevision::new(
                SkuRevisionId::new(next_id()),
                SkuRevisionData {
                    sku_id: company_sku_id.clone(),
                    revision_no: 1,
                    name: source_sku_rev.name.clone(),
                    description: None,
                    specification: Some(source_sku_rev.specification.clone())
                        .filter(|value| !value.trim().is_empty()),
                    barcode: source_sku_rev.barcode.clone(),
                    source_main_image_asset_id: source_sku_rev.source_main_image_asset_id.clone(),
                    weight_kg: None,
                    volume_m3: None,
                    sales_visible_price_gross: Some(sales_price),
                    market_price: Some(market_price),
                    status: EnableStatus::Active,
                    effective_from,
                    effective_to: None,
                },
            )?;
            sku.stable.current_revision_id = Some(sku_revision.base.id.clone());

            let mapping = SupplierProductMapping::new(
                SupplierProductMappingId::new(next_id()),
                SupplierProductMappingData {
                    supplier_catalog_sku_id: supplier_sku.base.id.clone().into(),
                    sku_id: company_sku_id.clone(),
                    status: MappingStatus::Active,
                    approved_by: Some(actor.id().to_string()),
                    approved_at: Some(Instant::now()),
                    reason: Some("reverse_promote_to_pool".to_string()),
                },
            )?;

            let mut offering = SupplierOffering::new(
                SupplierOfferingId::new(next_id()),
                SupplierOfferingData {
                    sku_id: company_sku_id,
                    supplier_id: supplier_id.clone(),
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

            rows.push(ReversePromoteRow {
                supplier_catalog_sku_id: item.supplier_catalog_sku_id.clone(),
                sku,
                sku_revision,
                mapping,
                offering,
                offering_revision,
            });
        }

        Ok(ReversePromoteDraft {
            product,
            product_revision,
            rows,
        })
    }
}

/// 反向入池事务草稿。
#[derive(Debug, Clone)]
struct ReversePromoteDraft {
    /// 新建公司商品。
    product: Product,
    /// 公司商品首个修订。
    product_revision: ProductRevision,
    /// 各 SKU 行实体。
    rows: Vec<ReversePromoteRow>,
}

/// 反向入池单行实体（公司 SKU + 映射 + 供给）。
#[derive(Debug, Clone)]
struct ReversePromoteRow {
    /// 供应商目录 SKU。
    supplier_catalog_sku_id: String,
    /// 新建公司 SKU。
    sku: Sku,
    /// 公司 SKU 首个修订。
    sku_revision: SkuRevision,
    /// 已生效映射。
    mapping: SupplierProductMapping,
    /// 供给稳定身份。
    offering: SupplierOffering,
    /// 首个供给修订。
    offering_revision: SupplierOfferingRevision,
}

/// 解析正式供给价：优先用户确认值，否则回退目录底价。
///
/// # 参数
/// * `confirmed` - 用户确认价（可空）
/// * `floor` - 目录底价
/// * `label` - 字段中文名（用于错误文案）
/// * `sku_id` - 供应商 SKU（用于错误文案）
///
/// # 返回
/// 返回可用于供给修订的金额字符串。
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

/// 解析必填金额。
///
/// # 参数
/// * `value` - 金额字符串
/// * `label` - 字段中文名
///
/// # 返回
/// 返回定点金额。
///
/// # 错误
/// 空白或非法金额时返回 `ValidationError`。
fn parse_amount_required(value: &str, label: &str) -> Result<Amount> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(format!("{label}不能为空")));
    }
    Amount::from_str(trimmed).map_err(|_| Error::ValidationError(format!("非法{label}: {trimmed}")))
}

/// 由供应商 SKU 修订计算公司规格签名。
///
/// 优先用来源结构化属性；单 SKU 无属性时用空规格签名；
/// 多 SKU 无属性时用供应商 SKU 编码保证行间唯一。
///
/// # 参数
/// * `revision` - 供应商 SKU 来源修订
/// * `supplier_sku_code` - 供应商 SKU 编码
/// * `item_count` - 本批入选行数
///
/// # 返回
/// 返回规范化规格签名。
///
/// # 错误
/// 属性代码/值非法时返回错误。
fn signature_for_supplier_sku(
    revision: &SupplierCatalogSkuRevision,
    supplier_sku_code: &str,
    item_count: usize,
) -> Result<String> {
    if !revision.structured_attributes.is_empty() {
        let entries: Vec<SpecSignatureEntry> = revision
            .structured_attributes
            .iter()
            .map(|attr| SpecSignatureEntry {
                attribute_code: truncate_code(&attr.attribute_name),
                value_code: truncate_code(&attr.attribute_value),
            })
            .collect();
        return compute_specification_signature(&entries).map_err(Into::into);
    }
    if item_count <= 1 {
        return compute_specification_signature(&[]).map_err(Into::into);
    }
    let entries = vec![SpecSignatureEntry {
        attribute_code: "supplier_sku".to_string(),
        value_code: truncate_code(supplier_sku_code),
    }];
    compute_specification_signature(&entries).map_err(Into::into)
}

/// 截断属性值代码到签名允许长度。
///
/// # 参数
/// * `value` - 原值
///
/// # 返回
/// 返回最多 64 字符的非空代码；空白时返回 `na`。
fn truncate_code(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "na".to_string();
    }
    trimmed.chars().take(64).collect()
}

/// 生成安全的短业务编码片段。
///
/// # 参数
/// * `preferred` - 优先使用的业务码
/// * `fallback_id` - 回退 ID
///
/// # 返回
/// 返回净化后的短码。
fn sanitize_code(preferred: &str, fallback_id: &str) -> String {
    let cleaned: String = preferred
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .take(24)
        .collect();
    if cleaned.is_empty() {
        fallback_id.chars().take(8).collect()
    } else {
        cleaned
    }
}
