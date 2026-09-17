use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::CatalogService;
use super::product_shared::resolve_product_file_references;
use super::sku_edit::{
    NewSkuContext, SkuEditItem, map_sku_edit_error, sku_edit_identity, specification_signature_for,
};
use super::support::ensure_version;
use crate::dto::{ProductSkuInput, ProductView, UpdateProductRequest};
use crate::entity::catalog::product::Product;
use crate::entity::catalog::product_revision::{ProductRevision, ProductRevisionData};
use crate::entity::catalog::product_revision_media::{MediaRole, ProductRevisionMedia};
use crate::entity::catalog::sku::{Sku, SkuEditAction};
use crate::entity::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use crate::entity::catalog::{
    EnableStatus, ProductId, ProductRevisionId, SkuId, SkuRevisionId, SpecificationSignatureSet,
    next_revision_no,
};
use crate::error::Result;
use crate::ports::{EmptyPendingAttachments, PendingAttachmentBatch};
use crate::repository::CatalogExt;

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

/// 规格编辑的基准事实：字典校验后即可确定的商品级输入。
struct SpecEditBase {
    /// 待编辑商品的稳定 ID。
    product_id: ProductId,
    /// 既有 SKU 按规范化签名的映射。
    current_by_signature: HashMap<String, Sku>,
    /// 下一个商品修订号。
    next_product_revision_no: u32,
    /// 新商品修订的稳定 ID。
    revision_id: ProductRevisionId,
    /// SPU 级媒体行。
    media: Vec<ProductRevisionMedia>,
}

/// SKU 行分类的共享上下文：逐行相同、与输入行无关的部分。
struct SkuLineContext<'a> {
    /// 待编辑商品的稳定 ID。
    product_id: &'a ProductId,
    /// 从属事实生效开始日。
    effective_from: BusinessDate,
    /// 从属事实生效结束日。
    effective_to: Option<BusinessDate>,
    /// 修订创建人。
    created_by: &'a str,
    /// 重新启用历史停用 SKU 时的变更原因。
    change_reason: Option<&'a str>,
}

/// 扫描移除签名：既有启用 SKU 的签名未出现在本次请求时转为停用。
///
/// # 参数
/// * `current_by_signature` - 既有 SKU 按规范化签名的映射
/// * `signatures` - 本次请求登记的签名集
/// * `actor_id` - 停用操作人
///
/// # 返回
/// 返回转为停用的既有 SKU。
///
/// # 错误
/// 停用迁移非法时返回状态机错误。
fn sweep_removed_signatures(
    current_by_signature: &HashMap<String, Sku>,
    signatures: &SpecificationSignatureSet,
    actor_id: &str,
) -> Result<Vec<Sku>> {
    let mut disable = Vec::new();
    for (signature, sku) in current_by_signature {
        if sku.is_active() && !signatures.contains(signature) {
            let mut sku = sku.clone();
            sku.disable(actor_id)?;
            disable.push(sku);
        }
    }
    Ok(disable)
}

/// 组装新商品修订并挂到 SPU 当前指针。
///
/// # 参数
/// * `product` - 待编辑 SPU（可变，修订挂载更新其状态）
/// * `base` - 编辑基准（含新修订 ID 与修订号）
/// * `req` - 规格编辑请求（修订字段来源）
/// * `actor_id` - 修订创建人
///
/// # 返回
/// 返回新商品修订。
///
/// # 错误
/// 修订字段校验失败时返回对应错误。
fn assemble_spec_product_revision(
    product: &mut Product,
    base: &SpecEditBase,
    req: &UpdateProductRequest,
    actor_id: &str,
) -> Result<ProductRevision> {
    let revision = ProductRevision::new(
        base.revision_id.clone(),
        ProductRevisionData {
            product_id: base.product_id.clone(),
            revision_no: base.next_product_revision_no,
            name: req.name.clone(),
            description: req.description.clone(),
            specification: req.specification.clone(),
            category_id: req.category_id.clone(),
            brand_id: req.brand_id.clone(),
            status: req.status,
            effective_from: req.effective_from,
            effective_to: req.effective_to,
        },
    )?;
    product.attach_revision(&revision, actor_id)?;
    Ok(revision)
}

impl CatalogService {
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
        self.product_update_with_assets(id, req, Arc::new(EmptyPendingAttachments), actor).await
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
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        req.validate()?;
        let used = resolve_product_file_references(
            &mut req.carousel_media,
            &mut req.detail_media,
            &mut req.skus,
            &pending_assets,
        )?;
        pending_assets.ensure_all_used(&used)?;
        let mut product = self.access().require_product(actor, "update", id, &mut NoTransaction).await?;
        product.ensure_has_responsibility()?;
        ensure_version(product.base.version, req.version)?;
        let plan = self.build_spec_edit_plan(&mut product, req, actor, &pending_assets).await?;
        let product = self.write_spec_edit_plan(plan, actor, pending_assets).await?;
        self.product_view(product).await
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
        mut req: UpdateProductRequest,
        actor: &AuditActor,
        pending_assets: &dyn PendingAttachmentBatch,
    ) -> Result<SpecEditPlan> {
        let base = self.prepare_spec_edit_base(product, &req, pending_assets).await?;
        let change_reason = req.change_reason.as_deref().map(str::trim);
        let audit_message = change_reason.filter(|reason| !reason.is_empty()).map(str::to_string);
        let line_ctx = SkuLineContext {
            product_id: &base.product_id,
            effective_from: req.effective_from,
            effective_to: req.effective_to,
            created_by: actor.id(),
            change_reason,
        };
        let mut signatures = SpecificationSignatureSet::new();
        let mut sku_items = Vec::with_capacity(req.skus.len());
        for sku_input in std::mem::take(&mut req.skus) {
            sku_items.push(
                self.build_sku_edit_item(&base.current_by_signature, &mut signatures, &line_ctx, sku_input)
                    .await?,
            );
        }
        let disable = sweep_removed_signatures(&base.current_by_signature, &signatures, actor.id())?;
        let revision = assemble_spec_product_revision(product, &base, &req, actor.id())?;
        Ok(SpecEditPlan {
            change_reason: audit_message,
            product: product.clone(),
            revision,
            media: base.media,
            sku_items,
            disable,
        })
    }

    /// 准备规格编辑的基准事实：字典校验、既有 SKU、修订号与 SPU 媒体。
    ///
    /// # 参数
    /// * `product` - 已加载的 SPU
    /// * `req` - 规格编辑请求
    /// * `pending_assets` - 待登记附件批次
    ///
    /// # 返回
    /// 返回编辑基准（商品 ID、既有签名映射、下个修订号、新修订 ID、媒体行）。
    ///
    /// # 错误
    /// 字典/媒体校验失败时返回对应错误。
    async fn prepare_spec_edit_base(
        &self,
        product: &Product,
        req: &UpdateProductRequest,
        pending_assets: &dyn PendingAttachmentBatch,
    ) -> Result<SpecEditBase> {
        self.ensure_product_dictionaries(
            &req.category_id,
            &req.brand_id,
            &req.skus,
            product.product_kind,
            pending_assets,
        )
        .await?;
        let product_id = ProductId::new(product.base.id.clone());
        let existing = self.db.catalog().skus_for_product(&product_id, &mut NoTransaction).await?;
        let current_by_signature: HashMap<String, Sku> =
            existing.into_iter().map(|sku| (sku.specification_signature.clone(), sku)).collect();
        let next_product_revision_no = self.next_product_revision_no(&product_id).await?;
        let revision_id = ProductRevisionId::new(next_id());
        let media = self
            .build_media_rows(&revision_id, &req.carousel_media, MediaRole::Carousel, pending_assets)
            .await?
            .into_iter()
            .chain(
                self.build_media_rows(&revision_id, &req.detail_media, MediaRole::Detail, pending_assets)
                    .await?,
            )
            .collect::<Vec<_>>();
        Ok(SpecEditBase { product_id, current_by_signature, next_product_revision_no, revision_id, media })
    }

    /// 分类一行 SKU 输入：命中既有签名走保留/重新启用修订，既有签名缺失走新增。
    ///
    /// 签名计算与签名去重登记在此完成；调用方传入的签名集用于后续移除扫描。
    ///
    /// # 参数
    /// * `current_by_signature` - 既有 SKU 按规范化签名的映射
    /// * `signatures` - 本次请求的签名去重集（可变，逐行登记）
    /// * `line_ctx` - 行级共享上下文（商品 ID、有效期、创建人、变更原因）
    /// * `sku_input` - 待分类的 SKU 输入行
    ///
    /// # 返回
    /// 返回带新修订的 SKU 编辑行（含 `Create`/`Keep`/`Reactivate` 动作）。
    ///
    /// # 错误
    /// 签名/规格/条码校验失败时返回对应错误。
    async fn build_sku_edit_item(
        &self,
        current_by_signature: &HashMap<String, Sku>,
        signatures: &mut SpecificationSignatureSet,
        line_ctx: &SkuLineContext<'_>,
        sku_input: ProductSkuInput,
    ) -> Result<SkuEditItem> {
        let signature = specification_signature_for(&sku_input.spec_entries)?;
        signatures.register_signature(signature.clone())?;
        if let Some(mut existing_sku) = current_by_signature.get(&signature).cloned() {
            let identity = sku_edit_identity(&sku_input, line_ctx.change_reason);
            let action = existing_sku.classify_edit(&identity).map_err(map_sku_edit_error)?;
            self.ensure_barcode_available(&sku_input.barcode, Some(existing_sku.base.id.as_str())).await?;
            let sku_id = SkuId::new(existing_sku.base.id.clone());
            let revision_no = self.next_sku_revision_no(&sku_id).await?;
            let revision = self.build_sku_revision(
                &sku_id,
                revision_no,
                line_ctx.effective_from,
                line_ctx.effective_to,
                &sku_input,
            )?;
            existing_sku.attach_revision(&revision, line_ctx.created_by)?;
            Ok(SkuEditItem { action, sku: existing_sku, revision })
        } else {
            self.build_new_sku_item(
                NewSkuContext {
                    product_id: line_ctx.product_id,
                    effective_from: line_ctx.effective_from,
                    effective_to: line_ctx.effective_to,
                    created_by: line_ctx.created_by,
                },
                sku_input,
            )
            .await
        }
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
        pending_assets: Arc<dyn PendingAttachmentBatch>,
    ) -> Result<Product> {
        let SpecEditPlan { change_reason, mut product, revision, media, sku_items, mut disable } = plan;
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "product.update",
            "product",
            product.base.id.clone(),
            change_reason,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let access = self.access();
        let actor = actor.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access
                        .ensure_writable(
                            &actor,
                            "update",
                            &product.maintainer_user_id,
                            &product.business_org_unit_id,
                            session,
                        )
                        .await?;
                    pending_assets.persist(&db, session).await?;
                    db.products().update(&mut product, session).await?;
                    db.catalog().create_product_revision_with_media(&revision, &media, session).await?;
                    for item in &sku_items {
                        match item.action {
                            SkuEditAction::Create => {
                                db.catalog()
                                    .create_sku_with_revision(&item.sku, &item.revision, &[], session)
                                    .await?;
                            },
                            SkuEditAction::Keep | SkuEditAction::Reactivate => {
                                db.sku_revisions().create(&item.revision, session).await?;
                                let mut sku = item.sku.clone();
                                db.skus().update(&mut sku, session).await?;
                            },
                        }
                    }
                    for sku in &mut disable {
                        db.skus().update(sku, session).await?;
                    }
                    audit_port.persist(&audit, session).await?;
                    Ok::<Product, crate::error::Error>(product)
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
        let latest = self.db.catalog().latest_product_revision_no(product_id, &mut NoTransaction).await?;
        Ok(next_revision_no(latest)?)
    }
}
