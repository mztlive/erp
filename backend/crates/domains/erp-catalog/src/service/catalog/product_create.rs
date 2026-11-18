use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_permissions::catalog;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, next_id};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CatalogService;
use super::product_shared::resolve_product_file_references;
use super::sku_edit::{NewSkuContext, SkuEditItem, specification_signature_for};
use crate::dto::{CreateProductRequest, ProductSkuInput, ProductView};
use crate::entity::catalog::product::{Product, ProductData};
use crate::entity::catalog::product_revision::{ProductRevision, ProductRevisionData};
use crate::entity::catalog::product_revision_media::ProductRevisionMedia;
use crate::entity::catalog::{EnableStatus, ProductId, ProductRevisionId, SpecificationSignatureSet};
use crate::error::{Error, Result};
use crate::ports::{EmptyPendingAttachments, PendingAttachmentBatch};
use crate::repository::CatalogExt;

/// 已规范化的商品创建输入；业务事实、编号和实体只在写事务中构造。
pub struct ProductDraft {
    request: CreateProductRequest,
}

struct PreparedProduct {
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

impl CatalogService {
    /// 创建商品（SPU + 首个商品修订 + 媒体 + 全部 SKU 行，跨集合事务）。
    ///
    /// 数据模型 §6.3：`product_no`/`sku_no`/`(product_id, specification_signature)`唯一由唯一索引兜底（`DuplicateKey`
    /// → 409）；未指定 `product_no` 时自动取号，未填写编号的 SKU 按 `SkuNoSequence` 生成 `SKU` 前缀编号；新签名分配新
    /// `sku_id`；条码冲突阻断；分类必须允许商品类型；规格名和值在所属 SPU 内直接生效。
    ///
    /// 失败情形如下：
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 分类/品牌/基础单位/媒体文件不存在
    /// * `BusinessLogicError` - 分类不允许商品类型、规格不适用于分类、条码冲突等
    /// * `ConflictError` - 唯一约束冲突或并发事务冲突
    pub async fn product_create(&self, req: CreateProductRequest, actor: &AuditActor) -> Result<ProductView> {
        self.product_create_with_assets(req, Arc::new(EmptyPendingAttachments), actor).await
    }

    /// 创建商品，并把同一次 multipart 命令携带的文件资产与商品聚合原子登记。临时引用无效、业务校验失败或事务写入失败时返回错误。
    ///
    /// - `req`：创建请求，文件字段可使用本次请求内临时引用。
    /// - `asset_requests`：已写入对象存储、尚未登记的文件资产。
    pub async fn product_create_with_assets(
        &self,
        req: CreateProductRequest,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        let draft = self.prepare_product_create(req, &pending_assets, actor).await?;

        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let product =
                        service.write_product_draft(draft, &actor, pending_assets, executor).await?;
                    service.product_view(product, executor).await
                })
            })
            .await
    }

    /// 校验输入和临时附件引用；数据库事实由原写事务读取。
    ///
    /// 返回尚未持久化的商品草稿。输入、临时附件引用或未使用附件校验失败时返回错误。
    pub async fn prepare_product_create(
        &self,
        mut req: CreateProductRequest,
        pending_assets: &Arc<dyn PendingAttachmentBatch>,
        _actor: &AuditActor,
    ) -> Result<ProductDraft> {
        req.validate()?;
        let used = resolve_product_file_references(
            &mut req.carousel_media,
            &mut req.detail_media,
            &mut req.skus,
            pending_assets,
        )?;
        pending_assets.ensure_all_used(&used)?;
        Ok(ProductDraft { request: req })
    }

    // 使用原写事务读取字典、归属及条码，构造待写入实体。
    //
    // 返回待写入的草稿。字典/媒体/规格校验失败时返回对应错误。
    async fn build_product_draft(
        &self,
        mut req: CreateProductRequest,
        actor: &AuditActor,
        pending_assets: &dyn PendingAttachmentBatch,
        executor: &mut dyn Executor,
    ) -> Result<PreparedProduct> {
        let product_id = ProductId::new(next_id());
        let revision_id = ProductRevisionId::new(next_id());
        let status = req.status.unwrap_or(EnableStatus::Active);
        let media = self.create_media(&revision_id, &req, pending_assets, executor).await?;

        let (maintainer_user_id, business_org_unit_id) =
            self.bind_create_maintainer(req.maintainer_user_id.as_deref(), actor, executor).await?;
        let product_no = self.resolve_product_no(req.take_explicit_product_no(), executor).await?;
        let mut product = Product::new(
            product_id.clone(),
            ProductData {
                product_no,
                product_kind: req.product_kind,
                status,
                maintainer_user_id,
                business_org_unit_id,
            },
            actor.id(),
        )?;

        ProductSkuInput::fill_blank_sku_nos(&product.product_no, Vec::new(), &mut req.skus);
        let sku_items = self
            .build_create_sku_items(
                &product_id,
                req.effective_from,
                req.effective_to,
                actor.id(),
                req.skus,
                executor,
            )
            .await?;

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
        Ok(PreparedProduct { change_reason: req.change_reason, product, revision, media, sku_items })
    }

    // 在原创建快照核验全部字典和图片引用，再构造媒体行。
    async fn create_media(
        &self,
        revision_id: &ProductRevisionId,
        request: &CreateProductRequest,
        pending: &dyn PendingAttachmentBatch,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevisionMedia>> {
        self.ensure_product_dictionaries(
            &request.category_id,
            &request.brand_id,
            &request.skus,
            request.product_kind,
            pending,
            executor,
        )
        .await?;

        self.build_spu_media(revision_id, &request.carousel_media, &request.detail_media, pending, executor)
            .await
    }

    // 确定新商品编号：请求已指定时沿用，否则按 `SPU` 序列取号。
    //
    // 授权后取号；计数器独立提交，商品事务回滚时已消费的序号不回收。
    //
    // 返回待写入的商品编号，规范化由实体完成。计数器写入失败时返回内部错误。
    //
    // - `requested`：请求显式指定的非空白商品编号。
    // - `executor`：当前授权事务执行器；计数器不加入此事务。
    async fn resolve_product_no(
        &self,
        requested: Option<String>,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        if let Some(product_no) = requested {
            return Ok(product_no);
        }
        DocumentNumberGenerator::new(self.db.clone())
            .next_number(DocumentNumberKind::Product, BusinessDate::today().as_naive_date(), executor)
            .await
            .map_err(|error| Error::Internal(error.to_string()))
    }

    // 构造创建草稿中的 SKU 行并登记规格签名。
    //
    // 返回待写入的 SKU 编辑项。规格签名冲突或 SKU 实体校验失败时拒绝。
    //
    // * `effective_from` / `effective_to` - 生效区间。
    async fn build_create_sku_items(
        &self,
        product_id: &ProductId,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
        created_by: &str,
        skus: Vec<ProductSkuInput>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuEditItem>> {
        let mut sku_items = Vec::with_capacity(skus.len());
        let mut signatures = SpecificationSignatureSet::new();
        for sku_input in skus {
            let signature = specification_signature_for(&sku_input.spec_entries)?;
            signatures.register_signature(signature)?;
            sku_items.push(
                self.build_new_sku_item(
                    NewSkuContext { product_id, effective_from, effective_to, created_by },
                    sku_input,
                    executor,
                )
                .await?,
            );
        }
        Ok(sku_items)
    }

    /// 使用调用方事务写入商品创建草稿并返回 SPU，不开启嵌套事务。
    ///
    /// 依次写入 `products`、`product_revisions` + 媒体、每个 SKU 的`skus` + `sku_revisions` + 规格属性值，
    /// 以及审计日志。
    ///
    /// 唯一索引冲突（409）或事务失败时返回错误并整体回滚。
    pub async fn write_product_draft(
        &self,
        draft: ProductDraft,
        actor: &AuditActor,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        executor: &mut dyn Executor,
    ) -> Result<Product> {
        if !executor
            .boundary()
            .is_some_and(|boundary| boundary.is_snapshot() && boundary.is_current(executor))
        {
            return Err(Error::ValidationError("商品写入必须使用原快照事务".into()));
        }
        self.access().ensure_writable(actor, &catalog::PRODUCT_PRODUCT_CREATE, executor).await?;

        let draft = self.build_product_draft(draft.request, actor, pending_assets.as_ref(), executor).await?;
        let PreparedProduct { change_reason, product, revision, media, sku_items } = draft;
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "product.create",
            "product",
            product.base.id.clone(),
            change_reason,
        )?;

        let db = self.db.clone();
        let audit_port = self.audit.clone();
        let access = self.access();
        let actor = actor.clone();
        access.require_create(&actor, &product, executor).await?;

        pending_assets.persist(&db, executor).await?;
        db.products().create(&product, executor).await?;
        db.catalog().create_product_revision_with_media(&revision, &media, executor).await?;

        for item in &sku_items {
            db.catalog().create_sku_with_revision(&item.sku, &item.revision, &[], executor).await?;
        }
        audit_port.persist(&audit, executor).await?;
        Ok(product)
    }
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::super::testing::{actor, denied, service};
    use super::*;

    #[tokio::test]
    async fn prepared_input_defers_database_facts_and_requires_snapshot_authorization_at_write() {
        let service = service().await;
        let actor = actor();
        let pending: Arc<dyn PendingAttachmentBatch> = Arc::new(EmptyPendingAttachments);
        let request: CreateProductRequest = serde_json::from_value(json!({
            "product_kind":"PHYSICAL", "name":"product", "category_id":"missing-category",
            "brand_id":"missing-brand", "effective_from":"2026-01-01",
            "skus":[{"name":"sku", "base_unit_id":"missing-unit", "main_image_asset_id":"missing-image"}]
        }))
        .unwrap();

        let draft = service.prepare_product_create(request.clone(), &pending, &actor).await.unwrap();
        assert_eq!(draft.request.category_id.as_ref(), "missing-category");
        assert!(matches!(
            service.write_product_draft(draft, &actor, pending.clone(), &mut NoTransaction).await,
            Err(Error::ValidationError(message)) if message == "商品写入必须使用原快照事务"
        ));

        let draft = service.prepare_product_create(request, &pending, &actor).await.unwrap();
        denied(service.write_product_draft(draft, &actor, pending, &mut SnapshotExecutor::default()).await);
    }
}
