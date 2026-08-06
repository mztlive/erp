//! 域 D27 `projection` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 建立投影 + 首个投影版本 + 下发记录：跨集合原子写入（`create_projection_revision`
//!   + 下发记录 + 审计）→ `database::Transactional::with_transaction`；
//! - 推进投影版本 + 下发记录：跨集合原子写入 → 事务；
//! - 投影下发（二期专属，P3 §3/§7）：**外部 HTTP 调用在事务之外完成**——
//!   事务 1 先落 `inbox_message`（Received）+ 审计；事务外经 [`MallConnector`]
//!   尝试下发（超时/重试上限/错误分类）；事务 2 把结果落
//!   `sales_order_projection_delivery` 与 `inbox_message`（Processed/Failed）+
//!   `integration_error_task`（失败时），商城确认后在同一事务推进投影的
//!   `current_acked_revision_id`。
//!
//! 跨域只调对方 Repository（P3 §2）：D13 `sales_orders`/`sales_order_revisions`/
//! `sales_order_revision_lines`/`sales_order_voucher_line_revisions` 由 ERP 销售单
//! 当前版本派生投影白名单快照（面额/卡张数/卡形态/履约期限/生效时间，§6.16）。
//! 业务规则（卡券单必填、唯一卡券行、面额/张数约束）在 entities，Service 只编排。

use std::pin::Pin;
use std::sync::Arc;

use database::{
    AccessControlExt, IntegrationOpsExt, NoTransaction, ProjectionExt, SalesOrderExt, Transactional,
};
use entities::ids::{
    InboxMessageId, IntegrationErrorTaskId, SalesOrderProjectionDeliveryId, SalesOrderProjectionId,
    SalesOrderProjectionRevisionId, SalesOrderRevisionId, SourceSystemId,
};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, MessageType,
};
use entities::projection::{
    ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection, SalesOrderProjectionData,
    SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData, SalesOrderProjectionRevision,
    SalesOrderProjectionRevisionData, SalesOrderProjectionUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::projection::dto::SortDir;

mod dto;

pub(crate) use self::dto::projection_content_hash;
pub use self::dto::{
    CreateSalesOrderProjectionRequest, CreateSalesOrderProjectionRevisionRequest,
    DeliverProjectionRevisionRequest, PageView, ProjectionDeliveryResultView,
    SalesOrderProjectionDeliveryListParams, SalesOrderProjectionDeliveryView, SalesOrderProjectionListParams,
    SalesOrderProjectionRevisionView, SalesOrderProjectionView,
};

/// 投影列表筛选条件类型（经 `ProjectionExt` 关联类型跨 crate 可达）。
type SalesOrderProjectionFilter = <mongodb::Database as ProjectionExt>::SalesOrderProjectionFilter;
/// 投影下发列表筛选条件类型。
type SalesOrderProjectionDeliveryFilter =
    <mongodb::Database as ProjectionExt>::SalesOrderProjectionDeliveryFilter;

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

/// 商城下发确认（成功响应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliverAck {
    /// 商城执行基线。
    pub mall_execution_baseline: String,
}

/// 商城连接器（外部下发调用统一入口）。
///
/// 实现要求（P3 §7、AGENTS.md 外部依赖容错）：统一设置超时（5 秒）、重试上限
/// （2 次）与错误分类；依赖失败降级为可观测错误。默认实现
/// [`UnavailableMallConnector`] 在目标商城端点不可解析时以分类错误失败关闭
/// （当前无端点注册表），测试注入 mock 验证成功与失败两条路径。
pub trait MallConnector: Send + Sync {
    /// 下发投影版本到目标商城。
    ///
    /// # 参数
    /// * `revision` - 待下发的投影版本
    /// * `target_mall_id` - 目标商城
    ///
    /// # 返回
    /// 下发成功返回商城确认（`mall_execution_baseline`）；失败返回分类错误。
    fn deliver_projection<'a>(
        &'a self,
        revision: &'a SalesOrderProjectionRevision,
        target_mall_id: &'a SourceSystemId,
    ) -> Pin<
        Box<dyn std::future::Future<Output = std::result::Result<DeliverAck, ClassifiedError>> + Send + 'a>,
    >;
}

/// 默认商城连接器：端点不可解析时失败关闭（可观测降级）。
pub struct UnavailableMallConnector;

impl MallConnector for UnavailableMallConnector {
    /// 下发投影版本到目标商城（默认实现恒失败关闭）。
    ///
    /// # 参数
    /// * `revision` - 待下发的投影版本
    /// * `target_mall_id` - 目标商城
    ///
    /// # 返回
    /// 恒返回 `TransientFailure` 分类错误（商城端点未注册）。
    fn deliver_projection<'a>(
        &'a self,
        revision: &'a SalesOrderProjectionRevision,
        target_mall_id: &'a SourceSystemId,
    ) -> Pin<
        Box<dyn std::future::Future<Output = std::result::Result<DeliverAck, ClassifiedError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(ClassifiedError {
                class: ErrorClass::TransientFailure,
                code: "MALL_ENDPOINT_UNRESOLVED".to_string(),
                summary: format!(
                    "投影修订 {} 下发目标商城 {} 失败关闭：端点未注册",
                    revision.base.id, target_mall_id
                ),
            })
        })
    }
}

/// 执行投影服务。
pub struct ProjectionService {
    db: Database,
    connector: Arc<dyn MallConnector>,
}

impl ProjectionService {
    /// 创建执行投影服务实例。
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

    /// 建立执行投影及其首个投影版本与下发记录（跨集合事务写入）。
    ///
    /// 投影来源为存量单切换快照（phase-2 §8.5.4）：以 ERP 销售单当前版本作为
    /// 第一份执行投影版本，不产生新的销售单版本。白名单快照（面额/卡张数/
    /// 卡形态/履约期限/生效时间）由销售单当前版本与唯一卡券行派生。
    ///
    /// # 参数
    /// * `req` - 建立请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建投影的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或当前版本不存在
    /// * `ValidationError` - 非卡券单、卡券行数量不为 1 或商城标识为空
    /// * `ConflictError` - `(sales_order_id, target_mall_id)` 已存在（唯一索引透出）
    pub async fn create_projection(
        &self,
        req: CreateSalesOrderProjectionRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderProjectionView> {
        req.validate()?;
        let (revision, voucher_line) = self.load_current_sales_revision(&req.sales_order_id).await?;

        let projection = SalesOrderProjection::new(
            SalesOrderProjectionId::new(next_id()),
            SalesOrderProjectionData {
                sales_order_id: req.sales_order_id,
                target_mall_id: req.target_mall_id.clone(),
            },
        )?;
        let projection_revision = SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new(next_id()),
            1,
            SalesOrderProjectionRevisionData {
                projection_id: projection.base.id.clone().into(),
                projection_source: ProjectionSource::CutoverSnapshot,
                sales_order_revision_id: revision.base.id.clone().into(),
                customer_external_identity: req.customer_external_identity,
                voucher_category_external_identity: req.voucher_category_external_identity,
                voucher_expiry_at: voucher_expiry(revision.as_ref())?,
                face_value: voucher_line.face_value,
                card_count: voucher_line.card_count,
                card_form: to_projection_card_form(voucher_line.card_form),
                effective_at: revision.effective_at,
                content_hash: "placeholder".to_string(),
            },
        )?;
        let mut projection_revision = projection_revision;
        projection_revision.content_hash = projection_content_hash(&projection_revision);
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: projection_revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::PendingSend,
                attempt_count: 0,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_execution_baseline: None,
                error_code: None,
                error_summary: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_order_projection.create",
            "sales_order_projection",
            projection.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let projection_tx = projection.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.projection()
                        .create_projection_revision(&projection_tx, &projection_revision, session)
                        .await?;
                    db.sales_order_projection_deliveries()
                        .create(&delivery, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(projection.into())
    }

    /// 分页查询执行投影列表。
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
    pub async fn projection_list(
        &self,
        params: &SalesOrderProjectionListParams,
    ) -> Result<PageView<SalesOrderProjectionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderProjectionFilter {
            sales_order_id: query.sales_order_id,
            target_mall_id: query.target_mall_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sales_order_projections()
            .search_sales_order_projections(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SalesOrderProjectionView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                target_mall_id: row.target_mall_id,
                current_acked_revision_id: row.current_acked_revision_id,
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

    /// 查询执行投影详情。
    ///
    /// # 参数
    /// * `id` - 投影 ID
    ///
    /// # 返回
    /// 返回投影详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 投影不存在
    pub async fn projection_detail(&self, id: &str) -> Result<SalesOrderProjectionView> {
        let projection = self
            .db
            .sales_order_projections()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))?;
        Ok(projection.into())
    }

    /// 推进执行投影版本（后续 ERP 销售版本 + 下发记录，跨集合事务写入）。
    ///
    /// 投影来源 `ErpRevision`：以销售单当前版本派生白名单快照；幂等键
    /// 「ERP 销售单号 + ERP 销售单版本 + 目标商城」由
    /// `(sales_order_revision_id, target_mall_id)` 唯一索引承接（§6.16）。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `req` - 推进请求（商城侧标识）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建投影版本的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 投影不存在
    /// * `ValidationError` - 销售单当前版本缺失或非卡券单
    /// * `ConflictError` - 同一销售版本已投影（唯一索引透出）
    pub async fn create_revision(
        &self,
        projection_id: &str,
        req: CreateSalesOrderProjectionRevisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderProjectionRevisionView> {
        req.validate()?;
        let projection = self
            .db
            .sales_order_projections()
            .find_by_id(projection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))?;
        let (revision, voucher_line) = self
            .load_current_sales_revision(&projection.sales_order_id)
            .await?;
        let revision_no = self.next_revision_no(projection_id).await?;
        let projection_revision = SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new(next_id()),
            revision_no,
            SalesOrderProjectionRevisionData {
                projection_id: projection.base.id.clone().into(),
                projection_source: ProjectionSource::ErpRevision,
                sales_order_revision_id: revision.base.id.clone().into(),
                customer_external_identity: req.customer_external_identity,
                voucher_category_external_identity: req.voucher_category_external_identity,
                voucher_expiry_at: voucher_expiry(revision.as_ref())?,
                face_value: voucher_line.face_value,
                card_count: voucher_line.card_count,
                card_form: to_projection_card_form(voucher_line.card_form),
                effective_at: revision.effective_at,
                content_hash: "placeholder".to_string(),
            },
        )?;
        let mut projection_revision = projection_revision;
        projection_revision.content_hash = projection_content_hash(&projection_revision);
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: projection_revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::PendingSend,
                attempt_count: 0,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_execution_baseline: None,
                error_code: None,
                error_summary: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_order_projection_revision.submit",
            "sales_order_projection_revision",
            projection_revision.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let projection_revision_tx = projection_revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.projection()
                        .create_projection_revision_with_delivery(&projection_revision_tx, &delivery, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(SalesOrderProjectionRevisionView {
            id: projection_revision.base.id,
            projection_id: projection_revision.projection_id.to_string(),
            revision_no: projection_revision.revision.revision_no,
            projection_source: projection_revision.projection_source,
            sales_order_revision_id: projection_revision.sales_order_revision_id.to_string(),
            customer_external_identity: projection_revision.customer_external_identity,
            face_value: projection_revision.face_value,
            card_count: projection_revision.card_count,
            card_form: projection_revision.card_form,
            effective_at: projection_revision.effective_at.unix_secs(),
            version: projection_revision.base.version,
            created_at: projection_revision.base.created_at,
        })
    }

    /// 列出投影版本（修订号降序）。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    ///
    /// # 返回
    /// 返回投影版本视图列表。
    pub async fn revision_list(&self, projection_id: &str) -> Result<Vec<SalesOrderProjectionRevisionView>> {
        let rows = self
            .db
            .sales_order_projection_revisions()
            .list_revisions_by_projection(
                &SalesOrderProjectionId::new(projection_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| SalesOrderProjectionRevisionView {
                id: row.id,
                projection_id: row.projection_id,
                revision_no: row.revision_no,
                projection_source: row.projection_source,
                sales_order_revision_id: row.sales_order_revision_id,
                customer_external_identity: row.customer_external_identity,
                face_value: row.face_value,
                card_count: row.card_count,
                card_form: row.card_form,
                effective_at: row.effective_at,
                version: row.version,
                created_at: row.created_at,
            })
            .collect())
    }

    /// 分页查询投影下发记录。
    ///
    /// # 参数
    /// * `params` - 查询参数（`target_mall_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn delivery_list(
        &self,
        params: &SalesOrderProjectionDeliveryListParams,
    ) -> Result<PageView<SalesOrderProjectionDeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderProjectionDeliveryFilter {
            target_mall_id: query.target_mall_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sales_order_projection_deliveries()
            .search_sales_order_projection_deliveries(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SalesOrderProjectionDeliveryView {
                id: row.id,
                projection_revision_id: row.projection_revision_id,
                target_mall_id: row.target_mall_id,
                status: row.status,
                attempt_count: row.attempt_count,
                mall_ack_at: row.mall_ack_at,
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

    /// 下发投影版本到目标商城（外部 HTTP 调用在事务之外完成）。
    ///
    /// 流程（二期专属，P3 §3/§7）：
    /// 1. 事务 1：落 `inbox_message`（`Received`）+ 审计；
    /// 2. 事务外：经 [`MallConnector`] 尝试下发；
    /// 3. 事务 2：成功 → 下发记录 `Confirmed` + 投影 `current_acked_revision_id`
    ///    推进 + `inbox_message` 置 `Processed`；失败 → 下发记录 `Failed`
    ///    （错误码/摘要）+ `inbox_message` 置 `Failed` + `integration_error_task`。
    ///
    /// 幂等：`(projection_revision_id, target_mall_id)` 唯一索引承接——已确认的
    /// 下发重复提交直接返回既有结果；未确认的重复下发返回 409。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `revision_no` - 修订序号
    /// * `req` - 下发请求（含幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回下发结果视图。
    ///
    /// # 错误
    /// * `NotFound` - 投影或投影版本不存在
    /// * `ConflictError` - 该版本已在下发中
    pub async fn deliver_revision(
        &self,
        projection_id: &str,
        revision_no: u32,
        req: DeliverProjectionRevisionRequest,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        req.validate()?;
        let mut projection = self
            .db
            .sales_order_projections()
            .find_by_id(projection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))?;
        let revision = self
            .db
            .sales_order_projection_revisions()
            .find_revision_by_no(
                &SalesOrderProjectionId::new(projection_id.to_string()),
                revision_no,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("投影版本不存在".to_string()))?;
        let target_mall_id = projection.target_mall_id.clone();
        let existing = self
            .db
            .sales_order_projection_deliveries()
            .find_delivery_by_revision_and_mall(
                &SalesOrderProjectionRevisionId::new(revision.base.id.clone()),
                &target_mall_id,
                &mut NoTransaction,
            )
            .await?;
        if let Some(existing) = existing {
            return self.idempotent_delivery_result(existing, projection);
        }

        let message = InboxMessage::new(
            InboxMessageId::new(next_id()),
            InboxMessageData {
                source_system_id: target_mall_id.clone(),
                source_event_id: format!(
                    "projection_delivery:{}:{}:{}",
                    revision.base.id, target_mall_id, req.idempotency_key
                ),
                message_type: MessageType::MallActionRequest,
                business_fact_key: format!("projection_delivery:{}:{}", revision.base.id, target_mall_id),
                payload_schema_version: "v1".to_string(),
                payload_reference: Some(revision.content_hash.clone()),
                status: InboxMessageStatus::Received,
                source_sent_at: None,
                received_at: entities::common::time::Instant::now(),
                processed_at: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_order_projection_delivery.deliver",
            "sales_order_projection_delivery",
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
        match self
            .connector
            .deliver_projection(&revision, &target_mall_id)
            .await
        {
            Ok(ack) => {
                self.settle_delivery_success(&mut projection, revision, message, now, ack, actor)
                    .await
            }
            Err(error) => {
                self.settle_delivery_failure(&mut projection, revision, message, now, error, actor)
                    .await
            }
        }
    }

    /// 计算投影的下一个修订序号（当前最大序号 + 1，首个修订为 1）。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    ///
    /// # 返回
    /// 返回下一个修订序号。
    async fn next_revision_no(&self, projection_id: &str) -> Result<u32> {
        let rows = self
            .db
            .sales_order_projection_revisions()
            .list_revisions_by_projection(
                &SalesOrderProjectionId::new(projection_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(rows.first().map(|row| row.revision_no + 1).unwrap_or(1))
    }

    /// 已存在下发记录的幂等返回（已确认直接返回既有结果，未确认返回冲突）。
    ///
    /// # 参数
    /// * `existing` - 既有下发记录
    /// * `projection` - 所属投影
    ///
    /// # 返回
    /// 返回下发结果视图。
    ///
    /// # 错误
    /// 下发尚未确认时返回 `ConflictError`。
    fn idempotent_delivery_result(
        &self,
        existing: SalesOrderProjectionDelivery,
        projection: SalesOrderProjection,
    ) -> Result<ProjectionDeliveryResultView> {
        if existing.status != ProjectionDeliveryStatus::Confirmed {
            return Err(Error::ConflictError(
                "该版本已在下发中，请查询下发状态".to_string(),
            ));
        }
        Ok(ProjectionDeliveryResultView {
            delivery_id: existing.base.id,
            delivery_status: existing.status,
            inbox_message_id: String::new(),
            error_task_id: None,
            mall_execution_baseline: existing.mall_execution_baseline,
            projection_version: projection.base.version,
        })
    }

    /// 把成功下发落库（事务 2：下发记录 + 投影确认版本推进 + 消息已处理）。
    ///
    /// # 参数
    /// * `projection` - 待推进的投影实体
    /// * `revision` - 已下发的投影版本
    /// * `message` - 待置 `Processed` 的消息
    /// * `at` - 下发时间
    /// * `ack` - 商城确认
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回下发结果视图。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_delivery_success(
        &self,
        projection: &mut SalesOrderProjection,
        revision: SalesOrderProjectionRevision,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        ack: DeliverAck,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::Confirmed,
                attempt_count: 1,
                next_attempt_at: None,
                mall_ack_at: Some(at),
                mall_execution_baseline: Some(ack.mall_execution_baseline.clone()),
                error_code: None,
                error_summary: None,
            },
        )?;
        projection.update(SalesOrderProjectionUpdate {
            current_acked_revision_id: Some(revision.base.id.clone().into()),
        })?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processed),
            processed_at: Some(at),
        })?;
        let audit = actor.clone().resource_log(
            "sales_order_projection_delivery.acked",
            "sales_order_projection_delivery",
            delivery.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let projection_version = projection.base.version;
        let inbox_id = message.base.id.clone();
        let mut projection_tx = projection.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_order_projection_deliveries()
                        .create(&delivery, session)
                        .await?;
                    db.sales_order_projections()
                        .update(&mut projection_tx, session)
                        .await?;
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProjectionDeliveryResultView {
            delivery_id,
            delivery_status: ProjectionDeliveryStatus::Confirmed,
            inbox_message_id: inbox_id,
            error_task_id: None,
            mall_execution_baseline: Some(ack.mall_execution_baseline),
            projection_version,
        })
    }

    /// 把失败下发落库（事务 2：下发记录失败 + 消息失败 + 错误任务）。
    ///
    /// # 参数
    /// * `projection` - 所属投影实体（确认版本不推进）
    /// * `revision` - 下发失败的投影版本
    /// * `message` - 待置 `Failed` 的消息
    /// * `at` - 下发时间
    /// * `error` - 分类错误
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回下发结果视图（含错误任务 ID）。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_delivery_failure(
        &self,
        projection: &mut SalesOrderProjection,
        revision: SalesOrderProjectionRevision,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        error: ClassifiedError,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::Failed,
                attempt_count: 1,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_execution_baseline: None,
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
            "sales_order_projection_delivery.failed",
            "sales_order_projection_delivery",
            delivery.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let task_id = task.base.id.clone();
        let inbox_id = message.base.id.clone();
        let projection_version = projection.base.version;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_order_projection_deliveries()
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

        Ok(ProjectionDeliveryResultView {
            delivery_id,
            delivery_status: ProjectionDeliveryStatus::Failed,
            inbox_message_id: inbox_id,
            error_task_id: Some(task_id),
            mall_execution_baseline: None,
            projection_version,
        })
    }

    /// 加载销售单当前版本与唯一卡券行（跨域读 D13 仓储）。
    ///
    /// # 参数
    /// * `sales_order_id` - 卡券销售单
    ///
    /// # 返回
    /// 返回 `(销售版本, 卡券行版本)` 元组。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或当前版本不存在
    /// * `ValidationError` - 非卡券单或卡券行数量不为 1
    async fn load_current_sales_revision(
        &self,
        sales_order_id: &entities::ids::SalesOrderId,
    ) -> Result<(
        Box<entities::sales_order::SalesOrderRevision>,
        Box<entities::sales_order::SalesOrderVoucherLineRevision>,
    )> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let revision_id = order
            .stable
            .current_revision_id
            .ok_or_else(|| Error::ValidationError("销售单尚未形成生效版本，无法建立投影".to_string()))?;
        let revision = self
            .db
            .sales_order_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
        voucher_expiry(&revision)?;
        let lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(
                &SalesOrderRevisionId::new(revision.base.id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let line_ids: Vec<entities::ids::SalesOrderRevisionLineId> = lines
            .iter()
            .map(|line| entities::ids::SalesOrderRevisionLineId::new(line.base.id.clone()))
            .collect();
        let voucher_lines = self
            .db
            .sales_order_voucher_line_revisions()
            .list_by_revision_line_ids(&line_ids, &mut NoTransaction)
            .await?;
        let voucher_line = match voucher_lines.len() {
            1 => voucher_lines.into_iter().next().expect("长度已判 1"),
            _ => {
                return Err(Error::ValidationError(
                    "卡券销售单必须恰好一条卡券行才能建立执行投影".to_string(),
                ))
            }
        };
        Ok((Box::new(revision), Box::new(voucher_line)))
    }
}

/// 校验销售版本为卡券单并返回表头履约期限。
///
/// # 参数
/// * `revision` - 销售版本
///
/// # 返回
/// 返回表头履约期限。
///
/// # 错误
/// 卡券类目与履约期限缺失时返回 `ValidationError`。
fn voucher_expiry(
    revision: &entities::sales_order::SalesOrderRevision,
) -> Result<entities::common::time::Instant> {
    if revision.voucher_category_sku_id.is_none() || revision.voucher_expiry_at.is_none() {
        return Err(Error::ValidationError("非卡券销售单无法建立执行投影".to_string()));
    }
    Ok(revision.voucher_expiry_at.expect("卡券履约期限必填"))
}

/// 把销售单卡形态映射为投影卡形态（两枚举同构，投影白名单值对象）。
///
/// # 参数
/// * `form` - 销售单卡形态
///
/// # 返回
/// 返回投影卡形态。
fn to_projection_card_form(form: entities::sales_order::CardForm) -> entities::projection::CardForm {
    match form {
        entities::sales_order::CardForm::Electronic => entities::projection::CardForm::Electronic,
        entities::sales_order::CardForm::Physical => entities::projection::CardForm::Physical,
    }
}

#[cfg(test)]
mod tests {
    use entities::integration_ops::ErrorClass;

    use super::{ClassifiedError, DeliverAck, MallConnector, UnavailableMallConnector};

    fn sample_revision() -> entities::projection::SalesOrderProjectionRevision {
        entities::projection::SalesOrderProjectionRevision::new(
            entities::ids::SalesOrderProjectionRevisionId::new("proj-rev-1"),
            1,
            entities::projection::SalesOrderProjectionRevisionData {
                projection_id: entities::ids::SalesOrderProjectionId::new("proj-1"),
                projection_source: entities::projection::ProjectionSource::ErpRevision,
                sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("so-rev-1"),
                customer_external_identity: "mall-customer-001".to_string(),
                voucher_category_external_identity: "mall-voucher-001".to_string(),
                voucher_expiry_at: entities::common::time::Instant::from_unix_secs(1_800_000_000),
                face_value: std::str::FromStr::from_str("100.00").unwrap(),
                card_count: 100,
                card_form: entities::projection::CardForm::Electronic,
                effective_at: entities::common::time::Instant::from_unix_secs(1_700_000_000),
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
            .deliver_projection(&revision, &entities::ids::SourceSystemId::new("mall-1"))
            .await
            .expect_err("默认连接器必须失败关闭");
        assert_eq!(error.class, ErrorClass::TransientFailure);
        assert_eq!(error.code, "MALL_ENDPOINT_UNRESOLVED");
    }

    #[tokio::test]
    async fn mock_connector_success_returns_ack() {
        struct MockConnector;
        impl MallConnector for MockConnector {
            fn deliver_projection<'a>(
                &'a self,
                revision: &'a entities::projection::SalesOrderProjectionRevision,
                _target_mall_id: &'a entities::ids::SourceSystemId,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = std::result::Result<DeliverAck, ClassifiedError>>
                        + Send
                        + 'a,
                >,
            > {
                Box::pin(async move {
                    Ok(DeliverAck {
                        mall_execution_baseline: format!("bl-{}", revision.base.id),
                    })
                })
            }
        }
        let connector = MockConnector;
        let revision = sample_revision();
        let ack = connector
            .deliver_projection(&revision, &entities::ids::SourceSystemId::new("mall-1"))
            .await
            .expect("mock 连接器必须成功");
        assert!(ack.mall_execution_baseline.starts_with("bl-proj-rev-1"));
    }
}
