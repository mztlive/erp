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
    AccessControlExt, CatalogExt, Executor, NoTransaction, PublicationExt, SupplierOfferingExt,
    Transactional, WorkItemExt,
};
use entities::ids::{
    ProductPublicationDeliveryId, ProductPublicationRevisionId, ProductPublicationRevisionMediaId,
    SourceSystemId, SupplierOfferingId, SupplierOfferingRevisionId, WorkItemId,
};
use entities::integration_ops::ErrorClass;
use entities::publication::{
    MediaRole, ProductPublication, ProductPublicationDelivery, ProductPublicationDeliveryData,
    ProductPublicationRevision, ProductPublicationRevisionData, ProductPublicationRevisionMedia,
    ProductPublicationRevisionMediaData, ProductPublicationUpdate, PublicationDeliveryStatus,
    SafetyPauseAffectedPublication, SafetyPauseBlocker, SafetyPauseCause, SafetyPauseFollowUp,
    SafetyPauseSourceObjectType, SafetyPauseWorkItemRef, SystemSafetyPauseOperation,
    SystemSafetyPauseOperationData,
};
use entities::supplier_offering::{AvailabilityStatus, OfferingStatus, SupplierOfferingRevision};
use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::publication::dto::SortDir;

mod delivery;
mod dto;

pub(crate) use self::dto::publication_content_hash;
pub use self::dto::{
    CreateProductPublicationRequest, CreateProductPublicationRevisionRequest,
    DeliverPublicationRevisionRequest, MediaItemRequest, PageView, ProcessPublicationDeliveriesRequest,
    ProcessPublicationDeliveriesResult, ProductPublicationDeliveryListParams, ProductPublicationDeliveryView,
    ProductPublicationListParams, ProductPublicationRevisionCommitView, ProductPublicationRevisionMediaView,
    ProductPublicationRevisionView, ProductPublicationView, PublicationDeliveryAction,
    PublicationDeliveryActionResult, PublicationDeliveryActionResultView, PublicationDeliveryCommand,
    PublicationDeliveryResultView, RetryPublicationDeliveryRequest, RetryPublicationDeliveryResultView,
    SystemSafetyPauseOperationView, SystemSafetyPauseTrigger, UpdateProductPublicationRequest,
};

/// 发布列表筛选条件类型（经 `PublicationExt` 关联类型跨 crate 可达）。
type ProductPublicationFilter = <mongodb::Database as PublicationExt>::ProductPublicationFilter;
/// 发布投递列表筛选条件类型。
type ProductPublicationDeliveryFilter =
    <mongodb::Database as PublicationExt>::ProductPublicationDeliveryFilter;

const SAFETY_PAUSE_ACTOR: &str = "system:w22_safety_pause";
const SUPPLIER_EXCEPTION_HANDLER: &str = "supplier_supply_exception";
const SUPPLIER_EXCEPTION_OWNER_ROLE: &str = "role-procurement";
const DEFAULT_OWNER_ORGANIZATION: &str = "company";

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

/// 按原稳定消息身份查询商城最终结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryPublicationResult {
    /// 商城权威确认。
    Confirmed(PublishAck),
    /// 商城权威返回明确失败。
    Failed(ClassifiedError),
    /// 商城仍无法给出最终结论。
    StillUnknown,
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

    /// 按原消息身份查询发布投递的商城最终结果。
    fn query_publication<'a>(
        &'a self,
        _revision: &'a ProductPublicationRevision,
        _target_mall_id: &'a SourceSystemId,
        _message_key: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = QueryPublicationResult> + Send + 'a>> {
        Box::pin(async { QueryPublicationResult::StillUnknown })
    }
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

    fn query_publication<'a>(
        &'a self,
        _revision: &'a ProductPublicationRevision,
        _target_mall_id: &'a SourceSystemId,
        _message_key: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = QueryPublicationResult> + Send + 'a>> {
        Box::pin(async { QueryPublicationResult::StillUnknown })
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

    /// 按原幂等键查询已落库的系统安全暂停结果。
    ///
    /// # 错误
    /// 幂等键为空、操作不存在或数据库查询失败时返回错误。
    pub async fn safety_pause_operation(
        &self,
        idempotency_key: &str,
    ) -> Result<SystemSafetyPauseOperationView> {
        let idempotency_key = idempotency_key.trim();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("安全暂停幂等键不能为空".to_string()));
        }
        let operation = self
            .db
            .system_safety_pause_operations()
            .find_safety_pause_by_idempotency_key(idempotency_key, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("安全暂停操作不存在".to_string()))?;
        Ok(SystemSafetyPauseOperationView::committed(operation))
    }

    /// 在调用方事务内执行可信来源触发的系统安全暂停。
    ///
    /// 本方法不启动、提交或回滚事务。它在同一传入执行器内冻结完整在售影响集，
    /// 为每个发布复制当前商城生效内容形成不可变 `PAUSE_ORDER` 修订与媒体，写入
    /// `PENDING_SEND` 投递并把稳定发布置为 `PAUSED`，最后写不可变 operation；
    /// `SUPPLIER_STOPPED` 额外创建或复用唯一开放 `BUSINESS_EXCEPTION`，其它原因
    /// 只写强类型 blocker。没有受影响在售发布时返回 `None` 且不伪造已提交操作。
    ///
    /// # 错误
    /// 传入非事务执行器、未知/不受支持来源、来源事实与原因/版本不匹配、幂等键
    /// 被其它事件占用，或任一原子写入失败时返回错误。
    pub async fn system_safety_pause_in_transaction(
        &self,
        trigger: &SystemSafetyPauseTrigger,
        executor: &mut dyn Executor,
    ) -> Result<Option<SystemSafetyPauseOperationView>> {
        trigger.validate_contract()?;
        if executor.session().is_none() {
            return Err(Error::BusinessLogicError(
                "系统安全暂停必须加入可信来源事实的同一事务".to_string(),
            ));
        }

        if let Some(existing) = self
            .db
            .system_safety_pause_operations()
            .find_safety_pause_by_event(
                trigger.source_object_type,
                trigger.source_object_id.trim(),
                trigger.cause,
                trigger.source_version.trim(),
                executor,
            )
            .await?
        {
            return Ok(Some(SystemSafetyPauseOperationView::already_safe(existing)));
        }
        if self
            .db
            .system_safety_pause_operations()
            .find_safety_pause_by_idempotency_key(trigger.idempotency_key.trim(), executor)
            .await?
            .is_some()
        {
            return Err(Error::ConflictError(
                "安全暂停幂等键已被其它来源事件使用".to_string(),
            ));
        }

        let source = self.validate_safety_pause_source(trigger, executor).await?;
        let mut publications = self
            .affected_on_sale_publications(&source.revisions, executor)
            .await?;
        if publications.is_empty() {
            return Ok(None);
        }
        publications.sort_by(|left, right| left.publication.base.id.cmp(&right.publication.base.id));

        let committed_at = entities::common::time::Instant::now();
        let operation_id = next_id();
        let follow_up = self
            .safety_pause_follow_up(trigger, &operation_id, committed_at, executor)
            .await?;
        let mut affected = Vec::with_capacity(publications.len());
        for affected_publication in publications {
            let mut publication = affected_publication.publication;
            let current_revision = affected_publication.current_revision;
            let current_revision_id = publication
                .current_revision_id()
                .map_err(|error| Error::BusinessLogicError(error.to_string()))?
                .to_string();
            if current_revision.base.id != current_revision_id {
                return Err(Error::BusinessLogicError(
                    "安全暂停影响集与当前生效供给修订不一致".to_string(),
                ));
            }
            let revision_no = self
                .next_revision_no_in_transaction(&publication.base.id, executor)
                .await?;
            let mut pause_revision = current_revision.safety_pause_copy(
                ProductPublicationRevisionId::new(next_id()),
                revision_no,
                committed_at,
            )?;
            pause_revision.content_hash = publication_content_hash(&pause_revision);
            let current_media = self
                .db
                .product_publication_revision_media()
                .find_media_by_revision(&ProductPublicationRevisionId::new(current_revision_id), executor)
                .await?;
            MediaRole::ensure_main_present(current_media.iter().map(|item| item.media_role))
                .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
            let pause_revision_id = ProductPublicationRevisionId::new(&pause_revision.base.id);
            let pause_media = current_media
                .into_iter()
                .map(|item| {
                    item.copy_to_revision(
                        ProductPublicationRevisionMediaId::new(next_id()),
                        pause_revision_id.clone(),
                    )
                    .map_err(Into::into)
                })
                .collect::<Result<Vec<_>>>()?;
            let delivery = ProductPublicationDelivery::new(
                ProductPublicationDeliveryId::new(next_id()),
                ProductPublicationDeliveryData {
                    publication_revision_id: pause_revision.base.id.clone().into(),
                    target_mall_id: publication.target_mall_id.clone(),
                    delivery_status: PublicationDeliveryStatus::PendingSend,
                    attempt_count: 0,
                    last_attempt_at: None,
                    next_attempt_at: None,
                    mall_ack_at: None,
                    mall_version: None,
                    error_class: None,
                    error_code: None,
                    error_summary: None,
                },
            )?;
            publication.mark_safety_paused(SAFETY_PAUSE_ACTOR)?;

            self.db
                .publication()
                .create_revision_with_media(&pause_revision, &pause_media, executor)
                .await?;
            self.db
                .product_publication_deliveries()
                .create(&delivery, executor)
                .await?;
            self.db
                .product_publications()
                .update(&mut publication, executor)
                .await?;
            affected.push(SafetyPauseAffectedPublication {
                publication_id: publication.base.id.clone().into(),
                pause_revision_id: pause_revision.base.id.into(),
                delivery_id: delivery.base.id.into(),
            });
        }

        let operation = SystemSafetyPauseOperation::new(
            operation_id,
            SystemSafetyPauseOperationData {
                cause: trigger.cause,
                source_object_type: trigger.source_object_type,
                source_object_id: trigger.source_object_id.trim().to_string(),
                source_version: trigger.source_version.trim().to_string(),
                idempotency_key: trigger.idempotency_key.trim().to_string(),
                affected_publications: affected,
                follow_up,
                occurred_at: trigger.occurred_at,
                committed_at,
            },
        )?;
        self.db
            .system_safety_pause_operations()
            .create(&operation, executor)
            .await?;

        Ok(Some(SystemSafetyPauseOperationView::committed(operation)))
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
            .find_product_publication(id, &mut NoTransaction)
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
            .find_product_publication(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))?;
        if publication.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let has_safety_pause = self
            .db
            .system_safety_pause_operations()
            .has_safety_pause_for_publication(id, &mut NoTransaction)
            .await?;
        let update = ProductPublicationUpdate {
            status: req.status,
            current_revision_id: req.current_revision_id,
        };
        if has_safety_pause {
            publication
                .ensure_safety_pause_update_allowed(&update)
                .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        }
        publication.update(update, actor.id())?;
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
    /// 返回发布修订与固定待发送投递的原子提交结果。
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
    ) -> Result<ProductPublicationRevisionCommitView> {
        req.validate()?;
        let mut publication = self
            .db
            .product_publications()
            .find_product_publication(publication_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))?;
        let has_safety_pause = self
            .db
            .system_safety_pause_operations()
            .has_safety_pause_for_publication(publication_id, &mut NoTransaction)
            .await?;
        if req.sale_status.is_on_sale() && has_safety_pause {
            return Err(Error::BusinessLogicError(
                "RECOVERY_RESPONSIBILITY_UNCONFIRMED：系统安全暂停发布禁止提交 ON_SALE 修订".to_string(),
            ));
        }
        let offering_revision = self
            .db
            .supplier_offering_revisions()
            .find_publication_offering_revision(&req.supplier_offering_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给修订不存在".to_string()))?;
        if req.sale_status.is_on_sale() {
            self.ensure_offering_can_publish_on_sale(&offering_revision)
                .await?;
        }
        self.db
            .sku_revisions()
            .find_publication_sku_revision(&req.sku_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品版本不存在".to_string()))?;
        MediaRole::ensure_main_present(req.media.iter().map(|item| item.media_role))
            .map_err(|error| Error::ValidationError(error.to_string()))?;

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
        let delivery = ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new(next_id()),
            ProductPublicationDeliveryData {
                publication_revision_id: revision.base.id.clone().into(),
                target_mall_id: publication.target_mall_id.clone(),
                delivery_status: PublicationDeliveryStatus::PendingSend,
                attempt_count: 0,
                last_attempt_at: None,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_version: None,
                error_class: None,
                error_code: None,
                error_summary: None,
            },
        )?;
        publication.mark_revision_submitted(has_safety_pause, actor.id())?;
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
        let delivery_tx = delivery.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.publication()
                        .create_revision_with_media(&revision_tx, &media_tx, session)
                        .await?;
                    db.product_publication_deliveries()
                        .create(&delivery_tx, session)
                        .await?;
                    db.product_publications()
                        .update(&mut publication_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        let revision_view = ProductPublicationRevisionView {
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
        };
        Ok(ProductPublicationRevisionCommitView {
            revision: revision_view,
            delivery_id: delivery.base.id,
            delivery_status: delivery.delivery_status,
            operation_id: delivery.message_key,
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
                message_key: row.message_key,
                delivery_status: row.delivery_status,
                attempt_count: row.attempt_count,
                last_attempt_at: row.last_attempt_at,
                next_attempt_at: row.next_attempt_at,
                mall_ack_at: row.mall_ack_at,
                mall_version: row.mall_version,
                error_class: row.error_class,
                error_code: row.error_code,
                error_summary: row.error_summary,
                inbox_message_id: row.inbox_message_id,
                error_task_id: row.error_task_id,
                work_item_id: row.work_item_id,
                allowed_actions: delivery::publication_delivery_actions(row.delivery_status, row.error_class),
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

    /// 校验安全暂停触发与当前权威供给事实一致并加载全部供给修订。
    ///
    /// # 参数
    /// * `trigger` - 已通过 DTO 契约校验的安全暂停触发
    /// * `executor` - 可信来源事实所在事务的数据库执行器
    ///
    /// # 返回
    /// 返回用于解析在售发布影响集的供给修订事实。
    ///
    /// # 错误
    /// 来源类型、原因、版本或供给事实不匹配，或仓储查询失败时返回错误。
    async fn validate_safety_pause_source(
        &self,
        trigger: &SystemSafetyPauseTrigger,
        executor: &mut dyn Executor,
    ) -> Result<ValidatedSafetyPauseSource> {
        if trigger.source_object_type != SafetyPauseSourceObjectType::SupplierOffering {
            return Err(Error::BusinessLogicError(
                "系统安全暂停只接受权威供应关系来源".to_string(),
            ));
        }
        let offering_id = SupplierOfferingId::new(trigger.source_object_id.trim());
        let offering = self
            .db
            .supplier_offerings()
            .find_publication_supplier_offering(&offering_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("安全暂停来源供给不存在".to_string()))?;
        let availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&offering_id, executor)
            .await?;
        let source_version = trigger.source_version.trim();
        let offering_version_matches = source_version == format!("offering:{}", offering.base.version);
        let availability_version_matches = availability
            .as_ref()
            .is_some_and(|value| source_version == format!("availability:{}", value.base.version));
        let valid = match trigger.cause {
            SafetyPauseCause::SupplierStopped => {
                (offering_version_matches && offering.stable.status == OfferingStatus::Stopped)
                    || (availability_version_matches
                        && availability
                            .as_ref()
                            .is_some_and(|value| value.availability_status == AvailabilityStatus::Stopped))
            }
            SafetyPauseCause::ZeroInventory => {
                availability_version_matches
                    && availability.as_ref().is_some_and(|value| {
                        value
                            .available_quantity
                            .is_some_and(|quantity| quantity.to_decimal().is_zero())
                    })
            }
            SafetyPauseCause::SupplyUnavailable => {
                availability_version_matches
                    && availability
                        .as_ref()
                        .is_some_and(|value| value.availability_status == AvailabilityStatus::Unavailable)
            }
            SafetyPauseCause::AvailabilityStale => {
                availability_version_matches
                    && availability
                        .as_ref()
                        .is_some_and(|value| value.availability_status == AvailabilityStatus::Stale)
            }
            SafetyPauseCause::CostChangeUnconfirmed | SafetyPauseCause::CriticalSupplyChangeUnconfirmed => {
                offering
                    .stable
                    .current_revision_id
                    .as_ref()
                    .is_some_and(|revision_id| source_version == format!("revision:{revision_id}"))
            }
            SafetyPauseCause::Unknown => false,
        };
        if !valid {
            return Err(Error::ConflictError(
                "安全暂停原因或来源版本与当前可信供给事实不一致".to_string(),
            ));
        }

        let revisions = self
            .db
            .supplier_offering_revisions()
            .list_publication_offering_revisions(&offering_id, executor)
            .await?;
        if revisions.is_empty() {
            return Err(Error::BusinessLogicError(
                "来源供给没有不可变商业条款版本，安全暂停失败关闭".to_string(),
            ));
        }
        Ok(ValidatedSafetyPauseSource { revisions })
    }

    /// 校验供给修订当前仍允许形成商城上架修订。
    ///
    /// # 参数
    /// * `revision` - 待绑定的不可变供给修订
    ///
    /// # 返回
    /// 稳定供给活跃、可供事实可用且没有安全暂停历史时返回 `Ok(())`。
    ///
    /// # 错误
    /// 供给或可供事实缺失、恢复责任未确认，或仓储查询失败时返回错误。
    async fn ensure_offering_can_publish_on_sale(&self, revision: &SupplierOfferingRevision) -> Result<()> {
        let offering = self
            .db
            .supplier_offerings()
            .find_publication_supplier_offering(&revision.supplier_offering_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给不存在".to_string()))?;
        let availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&revision.supplier_offering_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("供给可供事实不存在，发布失败关闭".to_string()))?;
        let blocked_by_history = self
            .db
            .system_safety_pause_operations()
            .has_safety_pause_for_source(
                SafetyPauseSourceObjectType::SupplierOffering,
                &offering.base.id,
                &mut NoTransaction,
            )
            .await?;
        if offering.stable.status != OfferingStatus::Active
            || !availability.is_available()
            || blocked_by_history
        {
            return Err(Error::BusinessLogicError(
                "RECOVERY_RESPONSIBILITY_UNCONFIRMED：供给仍处于安全暂停或恢复责任未确认".to_string(),
            ));
        }
        Ok(())
    }

    /// 解析指定供给修订当前影响的全部在售稳定发布。
    ///
    /// # 参数
    /// * `offering_revisions` - 安全暂停来源供给的全部不可变修订
    /// * `executor` - 可信来源事实所在事务的数据库执行器
    ///
    /// # 返回
    /// 返回稳定发布与其当前在售修订的配对集合。
    ///
    /// # 错误
    /// 发布缺少当前版本、发布与修订不一致，或仓储查询失败时返回错误。
    async fn affected_on_sale_publications(
        &self,
        offering_revisions: &[SupplierOfferingRevision],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AffectedOnSalePublication>> {
        let offering_revision_ids = offering_revisions
            .iter()
            .map(|revision| SupplierOfferingRevisionId::new(&revision.base.id))
            .collect::<Vec<_>>();
        let current_revisions = self
            .db
            .product_publication_revisions()
            .list_on_sale_by_offering_revisions(&offering_revision_ids, executor)
            .await?;
        if current_revisions.is_empty() {
            return Ok(Vec::new());
        }
        let revision_ids = current_revisions
            .iter()
            .map(|revision| ProductPublicationRevisionId::new(&revision.base.id))
            .collect::<Vec<_>>();
        let publications = self
            .db
            .product_publications()
            .list_safety_pause_candidate_publications(&revision_ids, executor)
            .await?;
        let revisions = current_revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<std::collections::HashMap<_, _>>();
        publications
            .into_iter()
            .map(|publication| {
                let revision_id = publication.stable.current_revision_id.as_ref().ok_or_else(|| {
                    Error::BusinessLogicError("商城生效发布缺少当前版本，安全暂停失败关闭".to_string())
                })?;
                let current_revision = revisions.get(revision_id).cloned().ok_or_else(|| {
                    Error::BusinessLogicError("商城生效发布与在售修订不一致，安全暂停失败关闭".to_string())
                })?;
                Ok(AffectedOnSalePublication {
                    publication,
                    current_revision,
                })
            })
            .collect()
    }

    /// 形成与安全暂停原因严格匹配的后续任务或固定阻断证据。
    ///
    /// # 参数
    /// * `trigger` - 当前安全暂停触发
    /// * `operation_id` - 不可变安全暂停操作 ID
    /// * `committed_at` - 本地事务统一业务时间
    /// * `executor` - 可信来源事实所在事务的数据库执行器
    ///
    /// # 返回
    /// 供应停止返回唯一开放任务引用，其它已注册原因返回固定 blocker。
    ///
    /// # 错误
    /// 原因未知、工作项构造或仓储写入失败时返回错误。
    async fn safety_pause_follow_up(
        &self,
        trigger: &SystemSafetyPauseTrigger,
        operation_id: &str,
        committed_at: entities::common::time::Instant,
        executor: &mut dyn Executor,
    ) -> Result<SafetyPauseFollowUp> {
        if trigger.cause != SafetyPauseCause::SupplierStopped {
            let (code, message) = trigger
                .cause
                .follow_up_blocker()
                .ok_or_else(|| Error::BusinessLogicError("未知安全暂停原因必须失败关闭".to_string()))?;
            return Ok(SafetyPauseFollowUp::Blocker(SafetyPauseBlocker {
                code,
                message: message.to_string(),
                evidence_reference: operation_id.to_string(),
            }));
        }

        let business_object_type = trigger.source_object_type.as_str();
        let mut work_item = self
            .db
            .work_items()
            .find_open_business_exception_for_object(
                business_object_type,
                trigger.source_object_id.trim(),
                executor,
            )
            .await?;
        if work_item.is_none() {
            let created = WorkItem::new_at(
                WorkItemId::new(next_id()),
                WorkItemData {
                    work_item_type: WorkItemType::BusinessException,
                    business_object_type: business_object_type.to_string(),
                    business_object_id: trigger.source_object_id.trim().to_string(),
                    subject_version: trigger.source_version.trim().to_string(),
                    owner_role: SUPPLIER_EXCEPTION_OWNER_ROLE.to_string(),
                    owner_organization_id: DEFAULT_OWNER_ORGANIZATION.to_string(),
                    owner_user_id: trigger.owner_user_id.clone(),
                    assignment_source: AssignmentSource::SystemRule,
                    priority: WorkItemPriority::High,
                    due_at: None,
                    reason_code: Some(SafetyPauseCause::SupplierStopped.as_str().to_string()),
                    impact_summary: Some("供应停止已触发商城在售发布安全暂停".to_string()),
                },
                committed_at,
            )?;
            self.db.work_items().create(&created, executor).await?;
            work_item = Some(created);
        }
        let work_item = work_item.expect("安全暂停任务已创建或复用");
        Ok(SafetyPauseFollowUp::WorkItem(SafetyPauseWorkItemRef {
            work_item_id: work_item.base.id,
            task_version: work_item.base.version,
            business_object_type: work_item.business_object_type,
            business_object_id: work_item.business_object_id,
            subject_version: work_item.subject_version,
            handler_key: SUPPLIER_EXCEPTION_HANDLER.to_string(),
        }))
    }

    /// 在调用方事务内计算发布的下一个修订序号。
    ///
    /// # 参数
    /// * `publication_id` - 所属稳定发布 ID
    /// * `executor` - 当前发布写事务的数据库执行器
    ///
    /// # 返回
    /// 返回当前最大修订序号加一；没有修订时返回一。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn next_revision_no_in_transaction(
        &self,
        publication_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        let rows = self
            .db
            .product_publication_revisions()
            .list_revisions_by_publication(
                &entities::ids::ProductPublicationId::new(publication_id.to_string()),
                executor,
            )
            .await?;
        Ok(rows.first().map(|row| row.revision_no + 1).unwrap_or(1))
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
}

/// 已通过来源版本校验的安全暂停供给事实。
struct ValidatedSafetyPauseSource {
    /// 来源稳定供给的全部不可变商业条款修订。
    revisions: Vec<SupplierOfferingRevision>,
}

/// 安全暂停影响集中的稳定发布与当前在售修订配对。
struct AffectedOnSalePublication {
    /// 待推进为暂停状态的稳定发布。
    publication: ProductPublication,
    /// 当前商城生效且允许下单的发布修订。
    current_revision: ProductPublicationRevision,
}

#[cfg(test)]
mod tests {

    use entities::ids::{ProductPublicationId, SkuId, SourceSystemId};
    use entities::integration_ops::ErrorClass;
    use entities::publication::{ProductPublication, ProductPublicationData, ProductPublicationStatus};

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
