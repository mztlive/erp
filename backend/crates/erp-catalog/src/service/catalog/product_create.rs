use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use id_generator::next_id;
use persistence_core::Transactional;
use validator::Validate;

use super::CatalogService;
use super::product_shared::resolve_product_file_references;
use super::sku_edit::{NewSkuContext, SkuEditItem, specification_signature_for};
use crate::dto::{CreateProductRequest, ProductSkuInput, ProductView};
use crate::entity::catalog::product::{Product, ProductData};
use crate::entity::catalog::product_revision::{ProductRevision, ProductRevisionData};
use crate::entity::catalog::product_revision_media::{MediaRole, ProductRevisionMedia};
use crate::entity::catalog::{EnableStatus, ProductId, ProductRevisionId, SpecificationSignatureSet};
use crate::error::Result;
use crate::ports::{EmptyPendingAttachments, PendingAttachmentBatch};
use crate::repository::CatalogExt;

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
        self.product_create_with_assets(req, Arc::new(EmptyPendingAttachments), actor).await
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
        let draft = self.build_product_draft(req, actor, &pending_assets).await?;
        let product = self.write_product_draft(draft, actor, pending_assets).await?;
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
        pending_assets: &dyn PendingAttachmentBatch,
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
        let media = self.build_spu_media(&revision_id, &req, pending_assets).await?;
        let (maintainer_user_id, business_org_unit_id) =
            self.bind_create_maintainer(req.maintainer_user_id.as_deref(), actor).await?;
        let product = Product::new(
            product_id.clone(),
            ProductData {
                product_no: req.product_no,
                product_kind: req.product_kind,
                status,
                maintainer_user_id,
                business_org_unit_id,
            },
            actor.id(),
        )?;
        let sku_items = self
            .build_create_sku_items(&product_id, req.effective_from, req.effective_to, actor.id(), req.skus)
            .await?;
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
        Ok(ProductDraft { change_reason: req.change_reason, product, revision, media, sku_items })
    }

    /// 构造 SPU 轮播图与详情图媒体行。
    ///
    /// # 参数
    /// * `revision_id` - 所属商品修订
    /// * `req` - 创建请求中的媒体输入
    /// * `pending_assets` - 本次命令携带的临时文件
    ///
    /// # 返回
    /// 返回轮播图与详情图拼接后的媒体行。
    ///
    /// # 错误
    /// 媒体文件不存在或同用途顺序重复时拒绝。
    async fn build_spu_media(
        &self,
        revision_id: &ProductRevisionId,
        req: &CreateProductRequest,
        pending_assets: &dyn PendingAttachmentBatch,
    ) -> Result<Vec<ProductRevisionMedia>> {
        let carousel = self
            .build_media_rows(revision_id, &req.carousel_media, MediaRole::Carousel, pending_assets)
            .await?;
        let detail =
            self.build_media_rows(revision_id, &req.detail_media, MediaRole::Detail, pending_assets).await?;
        Ok(carousel.into_iter().chain(detail).collect())
    }

    /// 构造创建草稿中的 SKU 行并登记规格签名。
    ///
    /// # 参数
    /// * `product_id` - 所属商品
    /// * `effective_from` / `effective_to` - 生效区间
    /// * `created_by` - 创建人
    /// * `skus` - SKU 输入行
    ///
    /// # 返回
    /// 返回待写入的 SKU 编辑项。
    ///
    /// # 错误
    /// 规格签名冲突或 SKU 实体校验失败时拒绝。
    async fn build_create_sku_items(
        &self,
        product_id: &ProductId,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
        created_by: &str,
        skus: Vec<ProductSkuInput>,
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
                )
                .await?,
            );
        }
        Ok(sku_items)
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
        pending_assets: Arc<dyn PendingAttachmentBatch>,
    ) -> Result<Product> {
        let ProductDraft { change_reason, product, revision, media, sku_items } = draft;
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "product.create",
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
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access
                        .ensure_writable(
                            &actor,
                            "create",
                            &product.maintainer_user_id,
                            &product.business_org_unit_id,
                            executor,
                        )
                        .await?;
                    pending_assets.persist(&db, executor).await?;
                    db.products().create(&product, executor).await?;
                    db.catalog().create_product_revision_with_media(&revision, &media, executor).await?;
                    for item in &sku_items {
                        db.catalog()
                            .create_sku_with_revision(&item.sku, &item.revision, &[], executor)
                            .await?;
                    }
                    audit_port.persist(&audit, executor).await?;
                    Ok::<Product, crate::error::Error>(product)
                })
            })
            .await
    }
}
