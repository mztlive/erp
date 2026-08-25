use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, CatalogExt, NoTransaction, Transactional};
use entities::catalog::product::{Product, ProductData};
use entities::catalog::product_revision::{ProductRevision, ProductRevisionData};
use entities::catalog::product_revision_media::{
    ensure_unique_media_sort_orders, MediaRole, ProductRevisionMedia, ProductRevisionMediaData,
};
use entities::catalog::sku::{Sku, SkuEditAction};
use entities::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use entities::catalog::{
    next_revision_no, EnableStatus, ProductBrandId, ProductCategoryId, ProductId, ProductKind,
    ProductRevisionId, ProductRevisionMediaId, SkuId, SkuRevisionId, SpecificationSignatureSet,
    UnitOfMeasure, UnitOfMeasureId,
};
use entities::common::time::BusinessDate;
use id_generator::next_id;
use validator::Validate;

use super::sku_edit::{
    existing_sku_edit_identity, map_sku_edit_error, specification_signature_for, NewSkuContext, SkuEditItem,
};
use super::CatalogService;
use crate::audit::AuditActor;
use crate::catalog::dto::{
    CreateProductRequest, DisableProductRequest, ProductMediaInput, ProductSkuInput, ProductView,
    UpdateProductRequest,
};
use crate::errors::{Error, Result};
use crate::file_asset::PendingFileAssetRequest;
use crate::pending_file_assets::PendingFileAssets;

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
        self.product_create_with_assets(req, Vec::new(), actor).await
    }

    /// 创建商品，并把同一次 multipart 命令携带的文件资产与商品聚合原子登记。
    ///
    /// # 参数
    /// * `req` - 创建请求，文件字段可使用本次请求内临时引用
    /// * `asset_requests` - 已写入对象存储、尚未登记的文件资产
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建商品的响应视图。
    ///
    /// # 错误
    /// 临时引用无效、业务校验失败或事务写入失败时返回错误。
    pub async fn product_create_with_assets(
        &self,
        mut req: CreateProductRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        req.validate()?;
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let used = resolve_product_file_references(
            &mut req.carousel_media,
            &mut req.detail_media,
            &mut req.skus,
            &pending_assets,
        )?;
        pending_assets.ensure_all_used(&used)?;
        let draft = self.build_product_draft(req, actor, &pending_assets).await?;
        let product = self.write_product_draft(draft, actor, pending_assets).await?;
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
        self.product_update_with_assets(id, req, Vec::new(), actor).await
    }

    /// 编辑商品，并把同一次 multipart 命令携带的文件资产与新修订原子登记。
    ///
    /// # 参数
    /// * `id` - 商品稳定 ID
    /// * `req` - 完整规格编辑请求，文件字段可使用本次请求内临时引用
    /// * `asset_requests` - 已写入对象存储、尚未登记的文件资产
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的商品视图。
    ///
    /// # 错误
    /// 临时引用无效、业务校验失败、版本冲突或事务写入失败时返回错误。
    pub async fn product_update_with_assets(
        &self,
        id: &str,
        mut req: UpdateProductRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        req.validate()?;
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let used = resolve_product_file_references(
            &mut req.carousel_media,
            &mut req.detail_media,
            &mut req.skus,
            &pending_assets,
        )?;
        pending_assets.ensure_all_used(&used)?;
        let mut product = self.load_product(id).await?;
        ensure_product_version(&product, req.version)?;
        let plan = self
            .build_spec_edit_plan(&mut product, req, actor, &pending_assets)
            .await?;
        let product = self.write_spec_edit_plan(plan, actor, pending_assets).await?;
        self.product_view(product).await
    }

    /// 停用商品并生成一份服务端派生的不可变商品修订。
    ///
    /// 客户端只提交商品身份、已见版本、原因和生效日。服务端在同一事务内
    /// 读取当前商品及修订、复制当前媒体、写入停用修订、更新稳定主表并记录
    /// 审计，避免客户端为拼装完整更新请求而发起额外读取。
    ///
    /// # 参数
    /// * `id` - 商品稳定 ID
    /// * `req` - 停用命令
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回停用后的商品视图。
    ///
    /// # 错误
    /// * `NotFound` - 商品或当前修订不存在
    /// * `ConflictError` - 页面版本已过期或并发写入冲突
    /// * `BusinessLogicError` - 商品已经停用
    pub async fn product_disable(
        &self,
        id: &str,
        req: DisableProductRequest,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        req.validate()?;
        let audit = actor.clone().resource_log_with_message(
            "product.disable",
            "product",
            id.to_string(),
            req.change_reason.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let id = id.to_string();
        let actor_id = actor.id().to_string();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let snapshot = db
                        .catalog()
                        .product_disable_snapshot(&id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("商品不存在".to_string()))?;
                    let mut product = snapshot.product;
                    ensure_product_version(&product, req.version)?;
                    product
                        .disable(&actor_id)
                        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                    let revision_no = next_revision_no(snapshot.latest_revision_no)?;
                    let current_revision = snapshot
                        .current_revision
                        .ok_or_else(|| Error::NotFound("商品当前修订不存在".to_string()))?;
                    let revision = current_revision.disabled_successor(
                        ProductRevisionId::new(next_id()),
                        revision_no,
                        req.effective_from,
                    )?;
                    let media = snapshot
                        .media
                        .iter()
                        .map(|row| {
                            row.copy_to_revision(
                                ProductRevisionMediaId::new(next_id()),
                                ProductRevisionId::new(revision.base.id.clone()),
                            )
                        })
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    product.attach_revision(&revision, &actor_id)?;
                    db.products().update(&mut product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&revision, &media, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Product, crate::errors::Error>(product)
                })
            })
            .await?;

        self.product_view(updated).await
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
        pending_assets: &PendingFileAssets,
    ) -> Result<ProductDraft> {
        self.ensure_product_dictionaries(
            &req.category_id,
            &req.brand_id,
            &req.skus,
            req.product_kind,
            pending_assets,
        )
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
            .build_media_rows(
                &revision_id,
                &req.carousel_media,
                MediaRole::Carousel,
                pending_assets,
            )
            .await?
            .into_iter()
            .chain(
                self.build_media_rows(&revision_id, &req.detail_media, MediaRole::Detail, pending_assets)
                    .await?,
            )
            .collect::<Vec<_>>();
        let mut sku_items = Vec::with_capacity(req.skus.len());
        let mut signatures = SpecificationSignatureSet::new();
        for sku_input in req.skus {
            let signature = specification_signature_for(&sku_input.spec_entries)?;
            signatures.register_signature(signature)?;
            let item = self
                .build_new_sku_item(
                    NewSkuContext {
                        product_id: &product_id,
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
        product.attach_revision(&revision, actor.id())?;
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
        pending_assets: &PendingFileAssets,
    ) -> Result<()> {
        let unit_ids = skus
            .iter()
            .map(|sku| sku.base_unit_id.clone())
            .collect::<Vec<_>>();
        let references = self
            .db
            .catalog()
            .catalog_reference_data(Some(category_id), brand_id, &unit_ids, &mut NoTransaction)
            .await?;
        let category = references
            .category
            .ok_or_else(|| Error::NotFound("商品分类不存在".to_string()))?;
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
        if !self
            .db
            .catalog()
            .missing_file_asset_ids(&asset_ids, &mut NoTransaction)
            .await?
            .is_empty()
        {
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
    async fn build_media_rows(
        &self,
        revision_id: &ProductRevisionId,
        inputs: &[ProductMediaInput],
        role: MediaRole,
        pending_assets: &PendingFileAssets,
    ) -> Result<Vec<ProductRevisionMedia>> {
        let asset_ids = inputs
            .iter()
            .map(|input| &input.file_asset_id)
            .filter(|asset_id| !pending_assets.contains_id(asset_id))
            .cloned()
            .collect::<Vec<_>>();
        if !self
            .db
            .catalog()
            .missing_file_asset_ids(&asset_ids, &mut NoTransaction)
            .await?
            .is_empty()
        {
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
    async fn write_product_draft(
        &self,
        draft: ProductDraft,
        actor: &AuditActor,
        pending_assets: PendingFileAssets,
    ) -> Result<Product> {
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
                    pending_assets.persist(&db, session).await?;
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
        pending_assets: &PendingFileAssets,
    ) -> Result<SpecEditPlan> {
        self.ensure_product_dictionaries(
            &req.category_id,
            &req.brand_id,
            &req.skus,
            product.product_kind,
            pending_assets,
        )
        .await?;
        let product_id = ProductId::new(product.base.id.clone());
        let existing = self
            .db
            .catalog()
            .skus_for_product(&product_id, &mut NoTransaction)
            .await?;
        let current_by_signature: HashMap<String, Sku> = existing
            .into_iter()
            .map(|sku| (sku.specification_signature.clone(), sku))
            .collect();
        let next_product_revision_no = self.next_product_revision_no(&product_id).await?;
        let revision_id = ProductRevisionId::new(next_id());
        let media = self
            .build_media_rows(
                &revision_id,
                &req.carousel_media,
                MediaRole::Carousel,
                pending_assets,
            )
            .await?
            .into_iter()
            .chain(
                self.build_media_rows(&revision_id, &req.detail_media, MediaRole::Detail, pending_assets)
                    .await?,
            )
            .collect::<Vec<_>>();

        let mut sku_items = Vec::with_capacity(req.skus.len());
        let mut signatures = SpecificationSignatureSet::new();
        let mut disable = Vec::new();
        let change_reason = req.change_reason.as_deref().map(str::trim);
        let audit_message = change_reason
            .filter(|reason| !reason.is_empty())
            .map(str::to_string);
        for sku_input in req.skus {
            let signature = specification_signature_for(&sku_input.spec_entries)?;
            signatures.register_signature(signature.clone())?;
            if let Some(mut existing_sku) = current_by_signature.get(&signature).cloned() {
                let identity = existing_sku_edit_identity(&sku_input, change_reason);
                let action = existing_sku
                    .classify_edit(&identity)
                    .map_err(map_sku_edit_error)?;
                self.ensure_barcode_available(&sku_input.barcode, Some(existing_sku.base.id.as_str()))
                    .await?;
                let sku_id = SkuId::new(existing_sku.base.id.clone());
                let revision_no = self.next_sku_revision_no(&sku_id).await?;
                let revision = self.build_sku_revision(
                    &sku_id,
                    revision_no,
                    req.effective_from,
                    req.effective_to,
                    &sku_input,
                )?;
                existing_sku.attach_revision(&revision, actor.id())?;
                sku_items.push(SkuEditItem {
                    action,
                    sku: existing_sku,
                    revision,
                });
            } else {
                let item = self
                    .build_new_sku_item(
                        NewSkuContext {
                            product_id: &product_id,
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
            if sku.is_active() && !signatures.contains(signature) {
                let mut sku = sku.clone();
                sku.disable(actor.id())?;
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
        product.attach_revision(&revision, actor.id())?;
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
    async fn write_spec_edit_plan(
        &self,
        plan: SpecEditPlan,
        actor: &AuditActor,
        pending_assets: PendingFileAssets,
    ) -> Result<Product> {
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
                    pending_assets.persist(&db, session).await?;
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
                                let mut sku = item.sku.clone();
                                db.skus().update(&mut sku, session).await?;
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
    /// * `effective_from` / `effective_to` - 生效区间
    /// * `input` - SKU 输入行（含独立 SKU 名称）
    ///
    /// # 返回
    /// 返回 SKU 修订实体。
    ///
    /// # 错误
    /// 实体校验失败时返回错误。
    fn build_sku_revision(
        &self,
        sku_id: &SkuId,
        revision_no: u32,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
        input: &ProductSkuInput,
    ) -> Result<SkuRevision> {
        Ok(SkuRevision::new(
            SkuRevisionId::new(next_id()),
            SkuRevisionData {
                sku_id: sku_id.clone(),
                revision_no,
                name: input.name.clone(),
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
    pub(super) async fn next_product_revision_no(&self, product_id: &ProductId) -> Result<u32> {
        let latest = self
            .db
            .catalog()
            .latest_product_revision_no(product_id, &mut NoTransaction)
            .await?;
        Ok(next_revision_no(latest)?)
    }
}

/// 解析商品命令中全部临时文件引用，并返回实际被引用的临时键集合。
fn resolve_product_file_references(
    carousel_media: &mut [ProductMediaInput],
    detail_media: &mut [ProductMediaInput],
    skus: &mut [ProductSkuInput],
    pending_assets: &PendingFileAssets,
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

/// 校验商品乐观锁版本。
///
/// # 参数
/// * `product` - 当前商品稳定实体
/// * `expected` - 客户端读取时看到的期望版本
///
/// # 返回
/// 当前版本与期望版本一致时返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回稳定的 409 冲突错误。
fn ensure_product_version(product: &Product, expected: u64) -> Result<()> {
    if !product.has_version(expected) {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
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
fn ensure_units_available(expected_ids: &[UnitOfMeasureId], units: &[UnitOfMeasure]) -> Result<()> {
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
