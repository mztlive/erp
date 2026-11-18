use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_permissions::catalog as catalog_permissions;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};
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
use crate::entity::catalog::product_revision_media::ProductRevisionMedia;
use crate::entity::catalog::sku::{Sku, SkuEditAction};
use crate::entity::catalog::{
    ProductId, ProductRevisionId, SkuId, SpecificationSignatureSet, next_revision_no,
};
use crate::error::Result;
use crate::ports::{EmptyPendingAttachments, PendingAttachmentBatch};
use crate::repository::CatalogExt;

/// 已规范化的商品规格编辑输入；当前商品和修订在原写事务读取。
pub struct SpecEditPlan {
    id: String,
    request: UpdateProductRequest,
}

struct PreparedSpecEditPlan {
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

// 完整规格编辑结束时，将未在目标签名集中出现的原启用 SKU 标为停用；既不删除 SKU，也不改历史修订。
fn sweep_removed_signatures(
    current_by_signature: &mut HashMap<String, Sku>,
    signatures: &SpecificationSignatureSet,
    actor_id: &str,
) -> Result<Vec<Sku>> {
    let mut disable = Vec::new();
    for (signature, mut sku) in current_by_signature.drain() {
        if sku.is_active() && !signatures.contains(&signature) {
            sku.disable(actor_id)?;
            disable.push(sku);
        }
    }
    Ok(disable)
}

// 组装新商品修订并挂到 SPU 当前指针。修订字段校验失败时返回对应错误。
//
// - `product`：待编辑 SPU（可变，修订挂载更新其状态）。
// - `base`：编辑基准（含新修订 ID 与修订号）。
// - `req`：规格编辑请求（修订字段来源）。
fn assemble_spec_product_revision(
    product: &mut Product,
    base: &SpecEditBase,
    req: UpdateProductRequest,
    actor_id: &str,
) -> Result<ProductRevision> {
    let revision = ProductRevision::new(
        base.revision_id.clone(),
        ProductRevisionData {
            product_id: base.product_id.clone(),
            revision_no: base.next_product_revision_no,
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
    product.attach_revision(&revision, actor_id)?;
    Ok(revision)
}

impl CatalogService {
    /// 按期望版本和完整 SKU 目标列表编辑商品规格；不携带待登记附件时使用此入口。
    /// 规范签名未变则保留 SKU ID 并追加修订；首次出现的签名创建新 SKU，缺省编号按序列生成并避开新旧占用。
    /// 显式恢复历史签名时复用原 SKU ID；移除的签名只停用 SKU，不删除历史。授权、版本、字典或条码校验失败不提交写入。
    pub async fn product_update(
        &self,
        id: &str,
        req: UpdateProductRequest,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        self.product_update_with_assets(id, req, Arc::new(EmptyPendingAttachments), actor).await
    }

    /// 同一 multipart 请求同时编辑商品规格和添加媒体时，先校验临时附件引用，再在一个事务中登记文件资产和新修订。
    /// 文件字节由调用方预先上传；引用无效、业务校验失败、版本冲突或写入失败时不提交目录修改。
    pub async fn product_update_with_assets(
        &self,
        id: &str,
        req: UpdateProductRequest,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        let plan = self.prepare_product_update(id, req, &pending_assets, actor).await?;

        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let product =
                        service.write_spec_edit_plan(plan, &actor, pending_assets, executor).await?;
                    service.product_view(product, executor).await
                })
            })
            .await
    }

    /// 校验更新输入及附件引用；版本、字典和规格事实由原写事务核验。
    ///
    /// 返回尚未持久化的编辑计划。输入、临时附件引用或未使用附件校验失败时返回错误。
    pub async fn prepare_product_update(
        &self,
        id: &str,
        mut req: UpdateProductRequest,
        pending_assets: &Arc<dyn PendingAttachmentBatch>,
        _actor: &AuditActor,
    ) -> Result<SpecEditPlan> {
        req.validate()?;
        let used = resolve_product_file_references(
            &mut req.carousel_media,
            &mut req.detail_media,
            &mut req.skus,
            pending_assets,
        )?;
        pending_assets.ensure_all_used(&used)?;
        Ok(SpecEditPlan { id: id.to_owned(), request: req })
    }

    // 构造规格编辑计划（分类保留/新增/重新启用/移除 + 新商品修订与媒体）。字典/媒体/规格/条码校验失败时返回对应错误。
    //
    // `product` 为已加载并完成版本校验的 SPU，所有权转入编辑计划。
    async fn build_spec_edit_plan(
        &self,
        mut product: Product,
        mut req: UpdateProductRequest,
        actor: &AuditActor,
        pending_assets: &dyn PendingAttachmentBatch,
        executor: &mut dyn Executor,
    ) -> Result<PreparedSpecEditPlan> {
        let mut base = self.prepare_spec_edit_base(&product, &req, pending_assets, executor).await?;
        let existing_sku_nos = base.current_by_signature.values().map(|sku| sku.sku_no.clone());
        ProductSkuInput::fill_blank_sku_nos(&product.product_no, existing_sku_nos, &mut req.skus);

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
                self.build_sku_edit_item(
                    &mut base.current_by_signature,
                    &mut signatures,
                    &line_ctx,
                    sku_input,
                    executor,
                )
                .await?,
            );
        }
        let disable = sweep_removed_signatures(&mut base.current_by_signature, &signatures, actor.id())?;

        let revision = assemble_spec_product_revision(&mut product, &base, req, actor.id())?;
        Ok(PreparedSpecEditPlan {
            change_reason: audit_message,
            product,
            revision,
            media: base.media,
            sku_items,
            disable,
        })
    }

    // 准备规格编辑的基准事实：字典校验、既有 SKU、修订号与 SPU 媒体。
    //
    // 返回编辑基准（商品 ID、既有签名映射、下个修订号、新修订 ID、媒体行）。字典/媒体校验失败时返回对应错误。
    async fn prepare_spec_edit_base(
        &self,
        product: &Product,
        req: &UpdateProductRequest,
        pending_assets: &dyn PendingAttachmentBatch,
        executor: &mut dyn Executor,
    ) -> Result<SpecEditBase> {
        self.ensure_product_dictionaries(
            &req.category_id,
            &req.brand_id,
            &req.skus,
            product.product_kind,
            pending_assets,
            executor,
        )
        .await?;

        let product_id = ProductId::new(product.base.id.clone());
        let existing = self.db.catalog().skus_for_product(&product_id, executor).await?;
        let current_by_signature: HashMap<String, Sku> =
            existing.into_iter().map(|sku| (sku.specification_signature.clone(), sku)).collect();

        let next_product_revision_no = self.next_product_revision_no(&product_id, executor).await?;
        let revision_id = ProductRevisionId::new(next_id());
        let media = self
            .build_spu_media(&revision_id, &req.carousel_media, &req.detail_media, pending_assets, executor)
            .await?;
        Ok(SpecEditBase { product_id, current_by_signature, next_product_revision_no, revision_id, media })
    }

    // 分类一行 SKU 输入：命中既有签名走保留/重新启用修订，既有签名缺失走新增。
    //
    // 签名计算与签名去重登记在此完成；调用方传入的签名集用于后续移除扫描。
    //
    // 返回带新修订的 SKU 编辑行（含 `Create`/`Keep`/`Reactivate` 动作）。签名/规格/条码校验失败时返回对应错误。
    //
    // - `signatures`：本次请求的签名去重集（可变，逐行登记）。
    // - `line_ctx`：行级共享上下文（商品 ID、有效期、创建人、变更原因）。
    async fn build_sku_edit_item(
        &self,
        current_by_signature: &mut HashMap<String, Sku>,
        signatures: &mut SpecificationSignatureSet,
        line_ctx: &SkuLineContext<'_>,
        sku_input: ProductSkuInput,
        executor: &mut dyn Executor,
    ) -> Result<SkuEditItem> {
        let signature = specification_signature_for(&sku_input.spec_entries)?;
        signatures.register_signature(signature.clone())?;

        if let Some(mut existing_sku) = current_by_signature.remove(&signature) {
            let identity = sku_edit_identity(&sku_input, line_ctx.change_reason);
            let action = existing_sku.classify_edit(&identity).map_err(map_sku_edit_error)?;
            self.ensure_barcode_available(&sku_input.barcode, Some(existing_sku.base.id.as_str()), executor)
                .await?;

            let sku_id = SkuId::new(existing_sku.base.id.clone());
            let revision_no = self.next_sku_revision_no(&sku_id, executor).await?;
            let revision = super::sku_edit::sku_revision_for_input(
                sku_id,
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
                executor,
            )
            .await
        }
    }

    /// 规格全量编辑的写入阶段：在原事务中完成对象授权、版本和关联事实复验，再登记附件并写商品与 SKU 修订。
    /// 按编辑动作创建、追加或停用 SKU，保留历史；同一事务登记来源字段的人工维护责任及审计，遇并发或写入失败交由调用方回滚。
    pub async fn write_spec_edit_plan(
        &self,
        plan: SpecEditPlan,
        actor: &AuditActor,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        executor: &mut dyn Executor,
    ) -> Result<Product> {
        let product = self
            .access()
            .require_product(actor, &catalog_permissions::PRODUCT_PRODUCT_UPDATE, &plan.id, executor)
            .await?;
        product.ensure_has_responsibility()?;
        ensure_version(product.base.version, plan.request.version)?;

        let plan = self
            .build_spec_edit_plan(product, plan.request, actor, pending_assets.as_ref(), executor)
            .await?;
        let PreparedSpecEditPlan { change_reason, mut product, revision, media, mut sku_items, mut disable } =
            plan;
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "product.update",
            "product",
            product.base.id.clone(),
            change_reason,
        )?;
        let db = self.db.clone();
        let audit_port = self.audit.clone();

        pending_assets.persist(&db, executor).await?;
        db.products().update(&mut product, executor).await?;
        db.catalog().create_product_revision_with_media(&revision, &media, executor).await?;

        for item in &mut sku_items {
            match item.action {
                SkuEditAction::Create => {
                    db.catalog().create_sku_with_revision(&item.sku, &item.revision, &[], executor).await?;
                },
                SkuEditAction::Keep | SkuEditAction::Reactivate => {
                    db.catalog().create_sku_revision(&item.revision, executor).await?;
                    db.skus().update(&mut item.sku, executor).await?;
                },
            }
        }
        for sku in &mut disable {
            db.skus().update(sku, executor).await?;
        }

        self.record_source_company_edit(&product.base.id, executor).await?;
        audit_port.persist(&audit, executor).await?;
        Ok(product)
    }

    // 计算某商品已有修订的最大序号 + 1。返回下一个修订序号。
    //
    // 沿用调用方的执行器，不另开事务。
    pub(super) async fn next_product_revision_no(
        &self,
        product_id: &ProductId,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        let latest = self.db.catalog().latest_product_revision_no(product_id, executor).await?;
        Ok(next_revision_no(latest)?)
    }
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::super::testing::{actor, denied, service};
    use super::*;

    #[tokio::test]
    async fn edit_input_defers_current_versions_and_rejects_before_reading_original_product() {
        let service = service().await;
        let actor = actor();
        let pending: Arc<dyn PendingAttachmentBatch> = Arc::new(EmptyPendingAttachments);
        for version in [1, 999] {
            let request = serde_json::from_value(json!({
                "version":version, "name":"product", "category_id":"missing-category", "brand_id":"missing-brand",
                "status":"active", "effective_from":"2026-01-01",
                "skus":[{"name":"sku", "base_unit_id":"missing-unit"}]
            })).unwrap();
            let plan =
                service.prepare_product_update("missing-product", request, &pending, &actor).await.unwrap();
            assert_eq!(plan.id, "missing-product");
            assert_eq!(plan.request.version, version);
            denied(
                service
                    .write_spec_edit_plan(plan, &actor, pending.clone(), &mut SnapshotExecutor::default())
                    .await,
            );
        }
    }
}
