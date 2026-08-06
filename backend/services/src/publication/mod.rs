//! 域 D26 `publication` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建稳定发布：单集合无跨步骤原子性要求 → `&mut NoTransaction`（审计独立写入）；
//! - 形成发布修订 + 媒体 + 发布状态推进：跨集合原子写入 → 事务；
//! - 发布投递（二期专属，P3 §3/§7）：**外部 HTTP 调用在事务之外完成**——
//!   事务 1 先落 `inbox_message`（Received）+ 审计；事务外经 [`MallConnector`]
//!   尝试投递（超时/重试上限/错误分类）；事务 2 把结果落 `product_publication_delivery`
//!   与 `inbox_message`（Processed/Failed）+ `integration_error_task`（失败时），
//!   商城确认后在同一事务把发布推进为「商城生效」并写入当前生效版本
//!   （§6.15「商城成功确认前不得把该版标记为商城已生效」）。
//!
//! 跨域只调对方 Repository（P3 §2）：D10 `skus`/`sku_revisions` 商品与商品版本
//! 存在性；D24 `supplier_offering_revisions` 供给修订存在性。业务规则（必填快照、
//! 生效区间、金额约束、媒体角色）都在 entities（已冻结只读），Service 只编排。

use std::pin::Pin;
use std::sync::Arc;

use database::{
    AccessControlExt, CatalogExt, IntegrationOpsExt, NoTransaction, PublicationExt, SupplierCatalogExt,
    Transactional,
};
use entities::ids::{
    InboxMessageId, IntegrationErrorTaskId, ProductPublicationDeliveryId, ProductPublicationRevisionId,
    ProductPublicationRevisionMediaId, SourceSystemId,
};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, MessageType,
};
use entities::publication::{
    MediaRole, ProductPublication, ProductPublicationDelivery, ProductPublicationDeliveryData,
    ProductPublicationRevision, ProductPublicationRevisionData, ProductPublicationRevisionMedia,
    ProductPublicationRevisionMediaData, ProductPublicationStatus, ProductPublicationUpdate,
    PublicationDeliveryStatus,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::publication::dto::SortDir;

mod dto;

pub(crate) use self::dto::publication_content_hash;
pub use self::dto::{
    CreateProductPublicationRequest, CreateProductPublicationRevisionRequest,
    DeliverPublicationRevisionRequest, MediaItemRequest, PageView, ProductPublicationDeliveryListParams,
    ProductPublicationDeliveryView, ProductPublicationListParams, ProductPublicationRevisionMediaView,
    ProductPublicationRevisionView, ProductPublicationView, PublicationDeliveryResultView,
    UpdateProductPublicationRequest,
};

/// 发布列表筛选条件类型（经 `PublicationExt` 关联类型跨 crate 可达）。
type ProductPublicationFilter = <mongodb::Database as PublicationExt>::ProductPublicationFilter;
/// 发布投递列表筛选条件类型。
type ProductPublicationDeliveryFilter =
    <mongodb::Database as PublicationExt>::ProductPublicationDeliveryFilter;

/// 外部调用错误分类（错误分类：临时故障/限流可自动重试，其余转人工，§7.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedError {
    /// 错误分类。
    pub class: ErrorClass,
    /// 稳定错误码。
    pub code: String,
    /// 脱敏错误摘要。
    pub summary: String,
}

/// 商城投递确认（成功响应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishAck {
    /// 商城确认版本。
    pub mall_version: String,
}

/// 商城连接器（外部投递调用统一入口）。
///
/// 实现要求（P3 §7、AGENTS.md 外部依赖容错）：统一设置超时（5 秒）、重试上限
/// （2 次）与错误分类；依赖失败降级为可观测错误。默认实现
/// [`UnavailableMallConnector`] 在目标商城端点不可解析时以分类错误失败关闭
/// （当前无端点注册表），测试注入 mock 验证成功与失败两条路径。
pub trait MallConnector: Send + Sync {
    /// 投递发布修订到目标商城。
    ///
    /// # 参数
    /// * `revision` - 待投递的发布修订
    /// * `target_mall_id` - 目标商城
    ///
    /// # 返回
    /// 投递成功返回商城确认（`mall_version`）；失败返回分类错误。
    fn publish_revision<'a>(
        &'a self,
        revision: &'a ProductPublicationRevision,
        target_mall_id: &'a SourceSystemId,
    ) -> Pin<
        Box<dyn std::future::Future<Output = std::result::Result<PublishAck, ClassifiedError>> + Send + 'a>,
    >;
}

/// 默认商城连接器：端点不可解析时失败关闭（可观测降级）。
pub struct UnavailableMallConnector;

impl MallConnector for UnavailableMallConnector {
    /// 投递发布修订到目标商城（默认实现恒失败关闭）。
    ///
    /// # 参数
    /// * `revision` - 待投递的发布修订
    /// * `target_mall_id` - 目标商城
    ///
    /// # 返回
    /// 恒返回 `TransientFailure` 分类错误（商城端点未注册）。
    fn publish_revision<'a>(
        &'a self,
        revision: &'a ProductPublicationRevision,
        target_mall_id: &'a SourceSystemId,
    ) -> Pin<
        Box<dyn std::future::Future<Output = std::result::Result<PublishAck, ClassifiedError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(ClassifiedError {
                class: ErrorClass::TransientFailure,
                code: "MALL_ENDPOINT_UNRESOLVED".to_string(),
                summary: format!(
                    "发布修订 {} 投递目标商城 {} 失败关闭：端点未注册",
                    revision.base.id, target_mall_id
                ),
            })
        })
    }
}

/// 商品发布服务。
pub struct PublicationService {
    db: Database,
    connector: Arc<dyn MallConnector>,
}

impl PublicationService {
    /// 创建商品发布服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `connector` - 商城连接器
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, connector: Arc<dyn MallConnector>) -> Self {
        Self { db, connector }
    }

    /// 创建稳定发布（单集合写入，无事务）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发布的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - ERP SKU 不存在
    /// * `ConflictError` - `(sku_id, target_mall_id)` 已存在（唯一索引透出）
    pub async fn create_publication(
        &self,
        req: CreateProductPublicationRequest,
        actor: &AuditActor,
    ) -> Result<ProductPublicationView> {
        req.validate()?;
        self.db
            .skus()
            .find_by_id(&req.sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("ERP SKU 不存在".to_string()))?;

        let publication = ProductPublication::new(
            entities::ids::ProductPublicationId::new(next_id()),
            entities::publication::ProductPublicationData {
                sku_id: req.sku_id,
                target_mall_id: req.target_mall_id,
                status: ProductPublicationStatus::Draft,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "product_publication.create",
            "product_publication",
            publication.base.id.clone(),
        )?;
        self.db
            .product_publications()
            .create(&publication, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        Ok(publication.into())
    }

    /// 分页查询商品发布列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn publication_list(
        &self,
        params: &ProductPublicationListParams,
    ) -> Result<PageView<ProductPublicationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductPublicationFilter {
            sku_id: query.sku_id,
            target_mall_id: query.target_mall_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .product_publications()
            .search_product_publications(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProductPublicationView {
                id: row.id,
                sku_id: row.sku_id,
                target_mall_id: row.target_mall_id,
                status: row.status,
                current_revision_id: row.current_revision_id,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询商品发布详情。
    ///
    /// # 参数
    /// * `id` - 发布 ID
    ///
    /// # 返回
    /// 返回发布详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 发布不存在
    pub async fn publication_detail(&self, id: &str) -> Result<ProductPublicationView> {
        let publication = self
            .db
            .product_publications()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))?;
        Ok(publication.into())
    }

    /// 更新商品发布（乐观锁语义，跨集合事务写入 + 审计）。
    ///
    /// # 参数
    /// * `id` - 发布 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后发布的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 发布不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 置为商城生效但缺少当前生效版本（实体校验）
    pub async fn update_publication(
        &self,
        id: &str,
        req: UpdateProductPublicationRequest,
        actor: &AuditActor,
    ) -> Result<ProductPublicationView> {
        req.validate()?;
        let mut publication = self
            .db
            .product_publications()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))?;
        if publication.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        publication.update(
            ProductPublicationUpdate {
                status: req.status,
                current_revision_id: req.current_revision_id,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "product_publication.update",
            "product_publication",
            publication.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_publications()
                        .update(&mut publication, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ProductPublication, crate::errors::Error>(publication)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 形成发布修订（不可变版本 + 受控媒体 + 发布状态推进，跨集合事务写入）。
    ///
    /// 修订号取当前最大序号 + 1；提交发布必填快照（`category_id`、`sales_description`
    /// 与至少一张主图，§6.15）与销售不变式由实体 `new()` 校验；媒体主图跨行约束
    /// 与供给修订存在性在 Service 校验。成功后把发布推进为「待发布」。
    ///
    /// # 参数
    /// * `publication_id` - 所属稳定发布
    /// * `req` - 形成修订请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发布修订的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 发布/供给修订/商品版本不存在
    /// * `ValidationError` - 必填快照缺失或缺少主图
    /// * `ConflictError` - `(publication_id, revision_no)` 唯一索引冲突
    pub async fn create_revision(
        &self,
        publication_id: &str,
        req: CreateProductPublicationRevisionRequest,
        actor: &AuditActor,
    ) -> Result<ProductPublicationRevisionView> {
        req.validate()?;
        let mut publication = self
            .db
            .product_publications()
            .find_by_id(publication_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))?;
        self.db
            .supplier_offering_revisions()
            .find_by_id(&req.supplier_offering_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给修订不存在".to_string()))?;
        self.db
            .sku_revisions()
            .find_by_id(&req.sku_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品版本不存在".to_string()))?;
        ensure_media_invariant(&req.media)?;

        let revision_no = self.next_revision_no(publication_id).await?;
        let revision = ProductPublicationRevision::new(
            ProductPublicationRevisionId::new(next_id()),
            revision_no,
            ProductPublicationRevisionData {
                product_publication_id: entities::ids::ProductPublicationId::new(publication_id.to_string()),
                sku_revision_id: req.sku_revision_id,
                supplier_offering_revision_id: req.supplier_offering_revision_id,
                category_id: req.category_id,
                name: req.name,
                specification: req.specification,
                sales_description: req.sales_description,
                minimum_purchase_quantity: req.minimum_purchase_quantity,
                sales_price_gross: req.sales_price_gross,
                sales_tax_rate: req.sales_tax_rate,
                base_unit_code: req.base_unit_code,
                sales_region: req.sales_region,
                sale_status: req.sale_status,
                product_capabilities: req.product_capabilities,
                valid_from: entities::common::time::Instant::from_unix_secs(req.valid_from as i64),
                valid_to: req
                    .valid_to
                    .map(|secs| entities::common::time::Instant::from_unix_secs(secs as i64)),
                content_hash: "placeholder".to_string(),
            },
        )?;
        let mut revision = revision;
        revision.content_hash = publication_content_hash(&revision);
        let media = req
            .media
            .into_iter()
            .map(|item| {
                ProductPublicationRevisionMedia::new(
                    ProductPublicationRevisionMediaId::new(next_id()),
                    revision.base.id.clone().into(),
                    ProductPublicationRevisionMediaData {
                        file_asset_id: item.file_asset_id,
                        media_role: item.media_role,
                        sort_no: item.sort_no,
                        alt_text: item.alt_text,
                    },
                )
                .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        publication.update(
            ProductPublicationUpdate {
                status: Some(ProductPublicationStatus::PendingPublish),
                current_revision_id: None,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "product_publication_revision.submit",
            "product_publication_revision",
            revision.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut publication_tx = publication;
        let revision_tx = revision.clone();
        let media_tx = media.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.publication()
                        .create_revision_with_media(&revision_tx, &media_tx, session)
                        .await?;
                    db.product_publications()
                        .update(&mut publication_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProductPublicationRevisionView {
            id: revision.base.id,
            product_publication_id: revision.product_publication_id.to_string(),
            revision_no: revision.revision.revision_no,
            name: revision.name,
            sale_status: revision.sale_status,
            sales_price_gross: revision.sales_price_gross,
            valid_from: revision.valid_from.unix_secs(),
            valid_to: revision.valid_to.map(|at| at.unix_secs()),
            version: revision.base.version,
            created_at: revision.base.created_at,
        })
    }

    /// 列出发布修订（修订号降序）。
    ///
    /// # 参数
    /// * `publication_id` - 所属稳定发布
    ///
    /// # 返回
    /// 返回修订视图列表。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn revision_list(&self, publication_id: &str) -> Result<Vec<ProductPublicationRevisionView>> {
        let rows = self
            .db
            .product_publication_revisions()
            .list_revisions_by_publication(
                &entities::ids::ProductPublicationId::new(publication_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| ProductPublicationRevisionView {
                id: row.id,
                product_publication_id: row.product_publication_id,
                revision_no: row.revision_no,
                name: row.name,
                sale_status: row.sale_status,
                sales_price_gross: row.sales_price_gross,
                valid_from: row.valid_from,
                valid_to: row.valid_to,
                version: row.version,
                created_at: row.created_at,
            })
            .collect())
    }

    /// 列出发布修订媒体（按角色、展示顺序排序）。
    ///
    /// # 参数
    /// * `revision_id` - 所属发布修订
    ///
    /// # 返回
    /// 返回媒体视图列表。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn revision_media_list(
        &self,
        revision_id: &str,
    ) -> Result<Vec<ProductPublicationRevisionMediaView>> {
        let media = self
            .db
            .product_publication_revision_media()
            .find_media_by_revision(
                &ProductPublicationRevisionId::new(revision_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(media
            .into_iter()
            .map(|item| ProductPublicationRevisionMediaView {
                id: item.base.id,
                product_publication_revision_id: item.product_publication_revision_id.to_string(),
                file_asset_id: item.file_asset_id.to_string(),
                media_role: item.media_role,
                sort_no: item.sort_no,
                alt_text: item.alt_text,
            })
            .collect())
    }

    /// 分页查询发布投递记录。
    ///
    /// # 参数
    /// * `params` - 查询参数（`target_mall_id`/`delivery_status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn delivery_list(
        &self,
        params: &ProductPublicationDeliveryListParams,
    ) -> Result<PageView<ProductPublicationDeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductPublicationDeliveryFilter {
            target_mall_id: query.target_mall_id,
            delivery_status: query.delivery_status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .product_publication_deliveries()
            .search_product_publication_deliveries(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProductPublicationDeliveryView {
                id: row.id,
                publication_revision_id: row.publication_revision_id,
                target_mall_id: row.target_mall_id,
                delivery_status: row.delivery_status,
                attempt_count: row.attempt_count,
                mall_version: row.mall_version,
                error_code: row.error_code,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 投递发布修订到目标商城（外部 HTTP 调用在事务之外完成）。
    ///
    /// 流程（二期专属，P3 §3/§7）：
    /// 1. 事务 1：落 `inbox_message`（`Received`）+ 审计；
    /// 2. 事务外：经 [`MallConnector`] 尝试投递；
    /// 3. 事务 2：成功 → 投递记录 `Confirmed` + 发布推进「商城生效」并写入当前
    ///    生效版本 + `inbox_message` 置 `Processed`；失败 → 投递记录 `Failed`
    ///    （错误码/摘要）+ `inbox_message` 置 `Failed` + `integration_error_task`。
    ///
    /// 幂等：`(publication_revision_id, target_mall_id)` 唯一索引承接——已确认的
    /// 投递重复提交直接返回既有结果；未确认的重复投递返回 409。
    ///
    /// # 参数
    /// * `publication_id` - 所属稳定发布
    /// * `revision_no` - 修订序号
    /// * `req` - 投递请求（含幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回投递结果视图。
    ///
    /// # 错误
    /// * `NotFound` - 发布或修订不存在
    /// * `ConflictError` - 该版本已在投递中
    pub async fn deliver_revision(
        &self,
        publication_id: &str,
        revision_no: u32,
        req: DeliverPublicationRevisionRequest,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryResultView> {
        req.validate()?;
        let mut publication = self
            .db
            .product_publications()
            .find_by_id(publication_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))?;
        let revision = self
            .db
            .product_publication_revisions()
            .find_revision_by_no(
                &entities::ids::ProductPublicationId::new(publication_id.to_string()),
                revision_no,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("发布修订不存在".to_string()))?;
        let target_mall_id = publication.target_mall_id.clone();
        let existing = self
            .db
            .product_publication_deliveries()
            .find_delivery_by_revision_and_mall(
                &ProductPublicationRevisionId::new(revision.base.id.clone()),
                &target_mall_id,
                &mut NoTransaction,
            )
            .await?;
        if let Some(existing) = existing {
            return self.idempotent_delivery_result(existing, publication);
        }

        let message = InboxMessage::new(
            InboxMessageId::new(next_id()),
            InboxMessageData {
                source_system_id: target_mall_id.clone(),
                source_event_id: format!(
                    "publication_delivery:{}:{}:{}",
                    revision.base.id, target_mall_id, req.idempotency_key
                ),
                message_type: MessageType::MallActionRequest,
                business_fact_key: format!("publication_delivery:{}:{}", revision.base.id, target_mall_id),
                payload_schema_version: "v1".to_string(),
                payload_reference: Some(revision.content_hash.clone()),
                status: InboxMessageStatus::Received,
                source_sent_at: None,
                received_at: entities::common::time::Instant::now(),
                processed_at: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "product_publication_delivery.deliver",
            "product_publication_delivery",
            revision.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let message_tx = message.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.inbox_messages().create(&message_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        let now = entities::common::time::Instant::now();
        match self.connector.publish_revision(&revision, &target_mall_id).await {
            Ok(ack) => {
                self.settle_delivery_success(&mut publication, revision, message, now, ack, actor)
                    .await
            }
            Err(error) => {
                self.settle_delivery_failure(&mut publication, revision, message, now, error, actor)
                    .await
            }
        }
    }

    /// 计算发布的下一个修订序号（当前最大序号 + 1，首个修订为 1）。
    ///
    /// # 参数
    /// * `publication_id` - 所属稳定发布
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn next_revision_no(&self, publication_id: &str) -> Result<u32> {
        let rows = self
            .db
            .product_publication_revisions()
            .list_revisions_by_publication(
                &entities::ids::ProductPublicationId::new(publication_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(rows.first().map(|row| row.revision_no + 1).unwrap_or(1))
    }

    /// 已存在投递记录的幂等返回（已确认直接返回既有结果，未确认返回冲突）。
    ///
    /// # 参数
    /// * `existing` - 既有投递记录
    /// * `publication` - 所属发布
    ///
    /// # 返回
    /// 返回投递结果视图。
    ///
    /// # 错误
    /// 投递尚未确认时返回 `ConflictError`。
    fn idempotent_delivery_result(
        &self,
        existing: ProductPublicationDelivery,
        publication: ProductPublication,
    ) -> Result<PublicationDeliveryResultView> {
        if existing.delivery_status != PublicationDeliveryStatus::Confirmed {
            return Err(Error::ConflictError(
                "该版本已在投递中，请查询投递状态".to_string(),
            ));
        }
        Ok(PublicationDeliveryResultView {
            delivery_id: existing.base.id,
            delivery_status: existing.delivery_status,
            inbox_message_id: String::new(),
            error_task_id: None,
            mall_version: existing.mall_version,
            publication_version: publication.base.version,
        })
    }

    /// 把成功投递落库（事务 2：投递记录 + 发布推进商城生效 + 消息已处理）。
    ///
    /// # 参数
    /// * `publication` - 待推进的发布实体
    /// * `revision` - 已投递的发布修订
    /// * `message` - 待置 `Processed` 的消息
    /// * `at` - 投递时间
    /// * `ack` - 商城确认
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回投递结果视图。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_delivery_success(
        &self,
        publication: &mut ProductPublication,
        revision: ProductPublicationRevision,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        ack: PublishAck,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryResultView> {
        let delivery = ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new(next_id()),
            ProductPublicationDeliveryData {
                publication_revision_id: revision.base.id.clone().into(),
                target_mall_id: publication.target_mall_id.clone(),
                delivery_status: PublicationDeliveryStatus::Confirmed,
                attempt_count: 1,
                last_attempt_at: Some(at),
                mall_ack_at: Some(at),
                mall_version: Some(ack.mall_version.clone()),
                error_code: None,
                error_summary: None,
            },
        )?;
        publication.update(
            ProductPublicationUpdate {
                status: Some(ProductPublicationStatus::MallEffective),
                current_revision_id: Some(revision.base.id.clone()),
            },
            actor.id(),
        )?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processed),
            processed_at: Some(at),
        })?;
        let audit = actor.clone().resource_log(
            "product_publication_delivery.acked",
            "product_publication_delivery",
            delivery.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let publication_version = publication.base.version;
        let inbox_id = message.base.id.clone();
        let mut publication_tx = publication.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_publication_deliveries()
                        .create(&delivery, session)
                        .await?;
                    db.product_publications()
                        .update(&mut publication_tx, session)
                        .await?;
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PublicationDeliveryResultView {
            delivery_id,
            delivery_status: PublicationDeliveryStatus::Confirmed,
            inbox_message_id: inbox_id,
            error_task_id: None,
            mall_version: Some(ack.mall_version),
            publication_version,
        })
    }

    /// 把失败投递落库（事务 2：投递记录失败 + 消息失败 + 错误任务）。
    ///
    /// # 参数
    /// * `publication` - 所属发布实体（状态保持待发布）
    /// * `revision` - 投递失败的发布修订
    /// * `message` - 待置 `Failed` 的消息
    /// * `at` - 投递时间
    /// * `error` - 分类错误
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回投递结果视图（含错误任务 ID）。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_delivery_failure(
        &self,
        publication: &mut ProductPublication,
        revision: ProductPublicationRevision,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        error: ClassifiedError,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryResultView> {
        let delivery = ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new(next_id()),
            ProductPublicationDeliveryData {
                publication_revision_id: revision.base.id.clone().into(),
                target_mall_id: publication.target_mall_id.clone(),
                delivery_status: PublicationDeliveryStatus::Failed,
                attempt_count: 1,
                last_attempt_at: Some(at),
                mall_ack_at: None,
                mall_version: None,
                error_code: Some(error.code.clone()),
                error_summary: Some(error.summary.clone()),
            },
        )?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Failed),
            processed_at: Some(at),
        })?;
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: Some(message.base.id.clone().into()),
                business_object_id: None,
                error_class: error.class,
                owner_role: Some("integration_ops".to_string()),
                owner_user_id: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "product_publication_delivery.failed",
            "product_publication_delivery",
            delivery.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let task_id = task.base.id.clone();
        let inbox_id = message.base.id.clone();
        let publication_version = publication.base.version;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_publication_deliveries()
                        .create(&delivery, session)
                        .await?;
                    db.integration_ops()
                        .create_error_task_with_message_failure(&task, &mut message, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PublicationDeliveryResultView {
            delivery_id,
            delivery_status: PublicationDeliveryStatus::Failed,
            inbox_message_id: inbox_id,
            error_task_id: Some(task_id),
            mall_version: None,
            publication_version,
        })
    }
}

/// 校验媒体行不变式：提交发布必须有至少一张主图（§6.15 跨行约束）。
///
/// # 参数
/// * `media` - 媒体行请求
///
/// # 错误
/// 缺少主图时返回 `ValidationError`。
fn ensure_media_invariant(media: &[MediaItemRequest]) -> Result<()> {
    if media.is_empty() || !media.iter().any(|item| item.media_role == MediaRole::Main) {
        return Err(Error::ValidationError(
            "提交发布必须包含至少一张主图媒体".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use entities::ids::{ProductPublicationId, SkuId, SourceSystemId};
    use entities::integration_ops::ErrorClass;
    use entities::publication::{
        MediaRole, ProductPublication, ProductPublicationData, ProductPublicationStatus,
    };

    use super::{ClassifiedError, MallConnector, PublishAck, UnavailableMallConnector};

    fn sample_revision() -> entities::publication::ProductPublicationRevision {
        entities::publication::ProductPublicationRevision::new(
            entities::ids::ProductPublicationRevisionId::new("rev-1"),
            1,
            entities::publication::ProductPublicationRevisionData {
                product_publication_id: ProductPublicationId::new("pub-1"),
                sku_revision_id: entities::ids::SkuRevisionId::new("sku-rev-1"),
                supplier_offering_revision_id: entities::ids::SupplierOfferingRevisionId::new("off-rev-1"),
                category_id: entities::ids::ProductCategoryId::new("cat-1"),
                name: "福利商城卡".to_string(),
                specification: None,
                sales_description: "员工福利采购".to_string(),
                minimum_purchase_quantity: std::str::FromStr::from_str("1.000000").unwrap(),
                sales_price_gross: std::str::FromStr::from_str("100.00").unwrap(),
                sales_tax_rate: std::str::FromStr::from_str("0.130000").unwrap(),
                base_unit_code: "张".to_string(),
                sales_region: None,
                sale_status: entities::publication::SaleStatus::OnSale,
                product_capabilities: vec![entities::publication::ProductCapability::Cancel],
                valid_from: entities::common::time::Instant::from_unix_secs(1_700_000_000),
                valid_to: None,
                content_hash: "abc".to_string(),
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn default_connector_fails_closed_with_classified_error() {
        let connector = UnavailableMallConnector;
        let revision = sample_revision();
        let error: ClassifiedError = connector
            .publish_revision(&revision, &SourceSystemId::new("mall-1"))
            .await
            .expect_err("默认连接器必须失败关闭");
        assert_eq!(error.class, ErrorClass::TransientFailure);
        assert_eq!(error.code, "MALL_ENDPOINT_UNRESOLVED");
    }

    #[tokio::test]
    async fn mock_connector_success_returns_ack() {
        struct MockConnector;
        impl MallConnector for MockConnector {
            fn publish_revision<'a>(
                &'a self,
                revision: &'a entities::publication::ProductPublicationRevision,
                _target_mall_id: &'a SourceSystemId,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = std::result::Result<PublishAck, ClassifiedError>>
                        + Send
                        + 'a,
                >,
            > {
                Box::pin(async move {
                    Ok(PublishAck {
                        mall_version: format!("v-{}", revision.base.id),
                    })
                })
            }
        }
        let connector = MockConnector;
        let revision = sample_revision();
        let ack = connector
            .publish_revision(&revision, &SourceSystemId::new("mall-1"))
            .await
            .expect("mock 连接器必须成功");
        assert!(ack.mall_version.starts_with("v-rev-1"));
    }

    #[test]
    fn media_invariant_requires_main_image() {
        let media = vec![super::MediaItemRequest {
            file_asset_id: entities::ids::FileAssetId::new("file-1"),
            media_role: MediaRole::Detail,
            sort_no: 1,
            alt_text: None,
        }];
        assert!(super::ensure_media_invariant(&media).is_err());
    }

    #[test]
    fn publication_view_flattens_stable_base() {
        let publication = ProductPublication::new(
            ProductPublicationId::new("pub-1"),
            ProductPublicationData {
                sku_id: SkuId::new("sku-1"),
                target_mall_id: SourceSystemId::new("mall-1"),
                status: ProductPublicationStatus::Draft,
            },
            "admin-1",
        )
        .unwrap();
        let view: super::ProductPublicationView = publication.into();
        assert_eq!(view.status, ProductPublicationStatus::Draft);
        assert_eq!(view.sku_id, "sku-1");
    }
}
