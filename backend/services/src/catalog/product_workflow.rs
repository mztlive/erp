use std::collections::HashMap;

use database::{AccessControlExt, CatalogExt, FileAssetExt, NoTransaction, Transactional};
use entities::catalog::product::{Product, ProductData};
use entities::catalog::product_revision::{ProductRevision, ProductRevisionData};
use entities::catalog::product_revision_media::{MediaRole, ProductRevisionMedia, ProductRevisionMediaData};
use entities::catalog::sku::{Sku, SkuUpdate};
use entities::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use entities::catalog::{
    EnableStatus, ProductBrandId, ProductCategoryId, ProductId, ProductKind, ProductRevisionId,
    ProductRevisionMediaId, SkuRevisionId,
};
use entities::common::time::BusinessDate;
use id_generator::next_id;
use mongodb::bson::doc;
use validator::Validate;

use super::sku_edit::{
    ensure_existing_sku_identity, specification_signature_for, NewSkuContext, SkuEditAction, SkuEditItem,
};
use super::support::ensure_version;
use super::CatalogService;
use crate::audit::AuditActor;
use crate::catalog::dto::{
    CreateProductRequest, ProductMediaInput, ProductSkuInput, ProductView, UpdateProductRequest,
};
use crate::errors::{Error, Result};

/// 商品（SPU）创建草稿（全部 ID 在事务外预生成，事务内只做写入）。
struct ProductDraft {
    /// 写入审计日志的创建原因。
    change_reason: Option<String>,
    /// SPU 稳定身份。
    product: Product,
    /// 商品修订快照。
    revision: ProductRevision,
    /// SPU 级媒体行。
    media: Vec<ProductRevisionMedia>,
    /// SKU 行（action 均为 `Create`）。
    sku_items: Vec<SkuEditItem>,
}

/// 商品规格编辑计划（数据模型 §6.3 全量替换语义）。
struct SpecEditPlan {
    /// 写入审计日志的变更原因。
    change_reason: Option<String>,
    /// 修订后的 SPU。
    product: Product,
    /// 商品修订快照。
    revision: ProductRevision,
    /// SPU 级媒体行。
    media: Vec<ProductRevisionMedia>,
    /// 带新修订的 SKU 行（`Create`/`Keep`/`Reactivate`）。
    sku_items: Vec<SkuEditItem>,
    /// 移除签名的既有 SKU（转为停用，保留全部历史）。
    disable: Vec<Sku>,
}

impl CatalogService {
    /// 创建商品（SPU + 首个商品修订 + 媒体 + 全部 SKU 行，跨集合事务）。
    ///
    /// 数据模型 §6.3：`product_no`/`sku_no`/`(product_id, specification_signature)`
    /// 唯一由唯一索引兜底（`DuplicateKey` → 409）；新签名分配新 `sku_id`；
    /// 条码冲突阻断；分类必须允许商品类型；规格名和值在所属 SPU 内直接生效。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建商品的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 分类/品牌/基础单位/媒体文件不存在
    /// * `BusinessLogicError` - 分类不允许商品类型、规格不适用于分类、条码冲突等
    /// * `ConflictError` - 唯一约束冲突或并发事务冲突
    pub async fn product_create(&self, req: CreateProductRequest, actor: &AuditActor) -> Result<ProductView> {
        req.validate()?;
        let draft = self.build_product_draft(req, actor).await?;
        let product = self.write_product_draft(draft, actor).await?;
        self.product_view(product).await
    }

    /// 规格编辑商品（数据模型 §6.3 全量替换语义，跨集合事务）。
    ///
    /// 按规范化签名把提交前后的签名集合分类为「保留/新增/重新启用/移除」：
    /// 签名未变沿用原 `sku_id` 并追加修订；从未存在的新签名分配新 `sku_id`；
    /// 历史停用签名复用原 `sku_id` 并显式重新启用；移除签名的旧 SKU 保留
    /// 全部历史并转为停用。任一校验失败或并发冲突整体回滚。
    ///
    /// # 参数
    /// * `id` - 商品 ID
    /// * `req` - 规格编辑请求（含期望版本与修订后全部 SKU 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回编辑后商品的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 商品/分类/品牌/基础单位/媒体文件不存在
    /// * `ConflictError` - 期望版本与当前版本不一致，或并发事务冲突
    /// * `BusinessLogicError` - 分类不允许商品类型、规格不适用于分类、条码冲突等
    pub async fn product_update(
        &self,
        id: &str,
        req: UpdateProductRequest,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        req.validate()?;
        let mut product = self.load_product(id).await?;
        ensure_version(product.base.version, req.version)?;
        let plan = self.build_spec_edit_plan(&mut product, req, actor).await?;
        let product = self.write_spec_edit_plan(plan, actor).await?;
        self.product_view(product).await
    }

    /// 构造商品创建草稿（全部 ID 预生成，事务外完成全部业务校验）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回待写入的草稿。
    ///
    /// # 错误
    /// 字典/媒体/规格校验失败时返回对应错误。
    async fn build_product_draft(
        &self,
        req: CreateProductRequest,
        actor: &AuditActor,
    ) -> Result<ProductDraft> {
        self.ensure_product_dictionaries(&req.category_id, &req.brand_id, &req.skus, req.product_kind)
            .await?;
        let product_id = ProductId::new(next_id());
        let revision_id = ProductRevisionId::new(next_id());
        let status = req.status.unwrap_or(EnableStatus::Active);
        let product = Product::new(
            product_id.clone(),
            ProductData {
                product_no: req.product_no,
                product_kind: req.product_kind,
                status,
            },
            actor.id(),
        )?;
        let media = self
            .build_media_rows(&revision_id, &req.carousel_media, MediaRole::Carousel)
            .await?
            .into_iter()
            .chain(
                self.build_media_rows(&revision_id, &req.detail_media, MediaRole::Detail)
                    .await?,
            )
            .collect::<Vec<_>>();
        let mut sku_items = Vec::with_capacity(req.skus.len());
        let mut seen_signatures = std::collections::HashSet::new();
        for sku_input in req.skus {
            if sku_input.sku_id.is_some()
                || sku_input.expected_sku_revision_id.is_some()
                || sku_input.reenable
            {
                return Err(Error::ValidationError(
                    "新建商品的 SKU 不得指定既有身份或重新启用意图".to_string(),
                ));
            }
            let signature = specification_signature_for(&sku_input.spec_entries)?;
            if !seen_signatures.insert(signature) {
                return Err(Error::BusinessLogicError("规格集合中存在重复签名".to_string()));
            }
            let item = self
                .build_new_sku_item(
                    NewSkuContext {
                        product_id: &product_id,
                        product_name: &req.name,
                        effective_from: req.effective_from,
                        effective_to: req.effective_to,
                        created_by: actor.id(),
                    },
                    sku_input,
                )
                .await?;
            sku_items.push(item);
        }
        let mut product = product;
        let revision = ProductRevision::new(
            revision_id.clone(),
            ProductRevisionData {
                product_id,
                revision_no: 1,
                name: req.name,
                description: req.description,
                specification: req.specification,
                category_id: req.category_id,
                brand_id: req.brand_id,
                status,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(revision.base.id.clone());
        Ok(ProductDraft {
            change_reason: req.change_reason,
            product,
            revision,
            media,
            sku_items,
        })
    }

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
    async fn ensure_product_dictionaries(
        &self,
        category_id: &ProductCategoryId,
        brand_id: &ProductBrandId,
        skus: &[ProductSkuInput],
        product_kind: ProductKind,
    ) -> Result<()> {
        let category = self.load_category(category_id.as_ref()).await?;
        if category.product_kind != product_kind {
            return Err(Error::BusinessLogicError("所选分类不允许该商品类型".to_string()));
        }
        self.ensure_brand_and_unit_ok(brand_id, skus.iter().map(|sku| &sku.base_unit_id))
            .await
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
    async fn build_media_rows(
        &self,
        revision_id: &ProductRevisionId,
        inputs: &[ProductMediaInput],
        role: MediaRole,
    ) -> Result<Vec<ProductRevisionMedia>> {
        let mut rows = Vec::with_capacity(inputs.len());
        for input in inputs {
            self.db
                .file_assets()
                .find_by_id(input.file_asset_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("媒体文件不存在".to_string()))?;
            let row = ProductRevisionMedia::new(
                ProductRevisionMediaId::new(next_id()),
                ProductRevisionMediaData {
                    product_revision_id: revision_id.clone(),
                    file_asset_id: input.file_asset_id.clone(),
                    media_role: role,
                    sort_order: input.sort_order,
                    alt_text: input.alt_text.clone(),
                },
            )?;
            rows.push(row);
        }
        ensure_unique_sort_orders(rows.iter().map(|row| row.sort_order), "媒体展示顺序")?;
        Ok(rows)
    }

    /// 在单个事务内写入商品创建草稿并返回 SPU。
    ///
    /// 依次写入 `products`、`product_revisions` + 媒体、每个 SKU 的
    /// `skus` + `sku_revisions` + 规格属性值，以及审计日志。
    ///
    /// # 参数
    /// * `draft` - 创建草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回写入后的 SPU 实体。
    ///
    /// # 错误
    /// 唯一索引冲突（409）或事务失败时返回错误并整体回滚。
    async fn write_product_draft(&self, draft: ProductDraft, actor: &AuditActor) -> Result<Product> {
        let ProductDraft {
            change_reason,
            product,
            revision,
            media,
            sku_items,
        } = draft;
        let audit = actor.clone().resource_log_with_message(
            "product.create",
            "product",
            product.base.id.clone(),
            change_reason,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.products().create(&product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&revision, &media, session)
                        .await?;
                    for item in &sku_items {
                        db.catalog()
                            .create_sku_with_revision(&item.sku, &item.revision, &[], session)
                            .await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Product, crate::errors::Error>(product)
                })
            })
            .await
    }
    /// 构造规格编辑计划（分类保留/新增/重新启用/移除 + 新商品修订与媒体）。
    ///
    /// # 参数
    /// * `product` - 已加载并完成版本校验的 SPU（可变，计划构建中更新状态）
    /// * `req` - 规格编辑请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回编辑计划。
    ///
    /// # 错误
    /// 字典/媒体/规格/条码校验失败时返回对应错误。
    async fn build_spec_edit_plan(
        &self,
        product: &mut Product,
        req: UpdateProductRequest,
        actor: &AuditActor,
    ) -> Result<SpecEditPlan> {
        self.ensure_product_dictionaries(&req.category_id, &req.brand_id, &req.skus, product.product_kind)
            .await?;
        let product_id = ProductId::new(product.base.id.clone());
        let existing = self
            .db
            .skus()
            .find_many(doc! { "product_id": product_id.to_string() }, &mut NoTransaction)
            .await?;
        let current_by_signature: HashMap<String, Sku> = existing
            .into_iter()
            .map(|sku| (sku.specification_signature.clone(), sku))
            .collect();
        let next_product_revision_no = self.next_product_revision_no(&product_id).await?;
        let revision_id = ProductRevisionId::new(next_id());
        let media = self
            .build_media_rows(&revision_id, &req.carousel_media, MediaRole::Carousel)
            .await?
            .into_iter()
            .chain(
                self.build_media_rows(&revision_id, &req.detail_media, MediaRole::Detail)
                    .await?,
            )
            .collect::<Vec<_>>();

        let mut sku_items = Vec::with_capacity(req.skus.len());
        let mut seen_signatures = std::collections::HashSet::new();
        let mut disable = Vec::new();
        let change_reason = req.change_reason.as_deref().map(str::trim);
        let audit_message = change_reason
            .filter(|reason| !reason.is_empty())
            .map(str::to_string);
        for sku_input in req.skus {
            let signature = specification_signature_for(&sku_input.spec_entries)?;
            if !seen_signatures.insert(signature.clone()) {
                return Err(Error::BusinessLogicError("规格集合中存在重复签名".to_string()));
            }
            if let Some(mut existing_sku) = current_by_signature.get(&signature).cloned() {
                // 保留/重新启用：沿用原 sku_id，追加修订；重新启用显式置 Active。
                let reactivating = !existing_sku.is_active();
                ensure_existing_sku_identity(&existing_sku, &sku_input, reactivating, change_reason)?;
                self.ensure_barcode_available(&sku_input.barcode, Some(existing_sku.base.id.as_str()))
                    .await?;
                let revision_no = self.next_sku_revision_no(&existing_sku.base.id).await?;
                let revision = self.build_sku_revision(
                    &existing_sku.base.id,
                    revision_no,
                    &req.name,
                    req.effective_from,
                    req.effective_to,
                    &sku_input,
                )?;
                if reactivating {
                    existing_sku.update(
                        SkuUpdate {
                            status: Some(EnableStatus::Active),
                        },
                        actor.id(),
                    )?;
                }
                sku_items.push(SkuEditItem {
                    action: if reactivating {
                        SkuEditAction::Reactivate
                    } else {
                        SkuEditAction::Keep
                    },
                    sku: existing_sku,
                    revision,
                });
            } else {
                // 全新签名：分配新 SKU 身份。
                if sku_input.sku_id.is_some()
                    || sku_input.expected_sku_revision_id.is_some()
                    || sku_input.reenable
                {
                    return Err(Error::ValidationError(
                        "新增规格签名不得指定或猜测既有 SKU 身份".to_string(),
                    ));
                }
                let item = self
                    .build_new_sku_item(
                        NewSkuContext {
                            product_id: &product_id,
                            product_name: &req.name,
                            effective_from: req.effective_from,
                            effective_to: req.effective_to,
                            created_by: actor.id(),
                        },
                        sku_input,
                    )
                    .await?;
                sku_items.push(item);
            }
        }
        for (signature, sku) in &current_by_signature {
            if sku.is_active() && !seen_signatures.contains(signature) {
                let mut sku = sku.clone();
                sku.update(
                    SkuUpdate {
                        status: Some(EnableStatus::Disabled),
                    },
                    actor.id(),
                )?;
                disable.push(sku);
            }
        }

        let revision = ProductRevision::new(
            revision_id.clone(),
            ProductRevisionData {
                product_id: product_id.clone(),
                revision_no: next_product_revision_no,
                name: req.name,
                description: req.description,
                specification: req.specification,
                category_id: req.category_id,
                brand_id: req.brand_id,
                status: req.status,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(revision.base.id.clone());
        product.stable.status = req.status;
        product.stable.touch(actor.id());
        Ok(SpecEditPlan {
            change_reason: audit_message,
            product: product.clone(),
            revision,
            media,
            sku_items,
            disable,
        })
    }

    /// 在单个事务内写入规格编辑计划。
    ///
    /// 写新商品修订与媒体、按动作写 SKU 修订/状态，更新 SPU，
    /// 移除签名的既有 SKU 转为停用，最后写审计日志；任一步失败整体回滚。
    ///
    /// # 参数
    /// * `plan` - 规格编辑计划
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回编辑后的 SPU 实体。
    ///
    /// # 错误
    /// 并发冲突（409）或事务失败时返回错误并整体回滚。
    async fn write_spec_edit_plan(&self, plan: SpecEditPlan, actor: &AuditActor) -> Result<Product> {
        let SpecEditPlan {
            change_reason,
            mut product,
            revision,
            media,
            sku_items,
            mut disable,
        } = plan;
        let audit = actor.clone().resource_log_with_message(
            "product.update",
            "product",
            product.base.id.clone(),
            change_reason,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.products().update(&mut product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&revision, &media, session)
                        .await?;
                    for item in &sku_items {
                        match item.action {
                            SkuEditAction::Create => {
                                db.catalog()
                                    .create_sku_with_revision(&item.sku, &item.revision, &[], session)
                                    .await?;
                            }
                            SkuEditAction::Keep | SkuEditAction::Reactivate => {
                                db.sku_revisions().create(&item.revision, session).await?;
                                if item.action == SkuEditAction::Reactivate {
                                    let mut sku = item.sku.clone();
                                    db.skus().update(&mut sku, session).await?;
                                }
                            }
                        }
                    }
                    for sku in &mut disable {
                        db.skus().update(sku, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Product, crate::errors::Error>(product)
                })
            })
            .await
    }

    /// 构造既有 SKU 的追加修订（Keep/Reactivate 动作）。
    ///
    /// # 参数
    /// * `sku_id` - 既有 SKU
    /// * `revision_no` - 下一个修订序号
    /// * `product_name` - 商品名称（修订名称快照）
    /// * `effective_from` / `effective_to` - 生效区间
    /// * `input` - SKU 输入行
    ///
    /// # 返回
    /// 返回 SKU 修订实体。
    ///
    /// # 错误
    /// 实体校验失败时返回错误。
    fn build_sku_revision(
        &self,
        sku_id: &str,
        revision_no: u32,
        product_name: &str,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
        input: &ProductSkuInput,
    ) -> Result<SkuRevision> {
        Ok(SkuRevision::new(
            SkuRevisionId::new(next_id()),
            SkuRevisionData {
                sku_id: sku_id.to_string().into(),
                revision_no,
                name: product_name.to_string(),
                description: None,
                specification: None,
                barcode: input.barcode.clone(),
                source_main_image_asset_id: input.main_image_asset_id.clone(),
                weight_kg: input.weight_kg,
                volume_m3: input.volume_m3,
                sales_visible_price_gross: input.sales_visible_price_gross,
                market_price: input.market_price,
                status: EnableStatus::Active,
                effective_from,
                effective_to,
            },
        )?)
    }

    /// 计算某商品已有修订的最大序号 + 1。
    ///
    /// # 参数
    /// * `product_id` - 商品 ID
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    pub(super) async fn next_product_revision_no(&self, product_id: &str) -> Result<u32> {
        let revisions = self
            .db
            .product_revisions()
            .find_many(doc! { "product_id": product_id }, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }
}

/// 校验一组整数内无重复（用于媒体展示顺序等组合唯一前置检查）。
///
/// # 参数
/// * `values` - 待检查值
/// * `label` - 字段说明
///
/// # 返回
/// 无重复时返回 `Ok(())`。
///
/// # 错误
/// 存在重复时返回 `BusinessLogicError`。
fn ensure_unique_sort_orders(values: impl Iterator<Item = i32>, label: &str) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(Error::BusinessLogicError(format!("{label}不能重复")));
        }
    }
    Ok(())
}
