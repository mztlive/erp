use std::collections::HashMap;

use database::{AccessControlExt, FulfillmentExt, NoTransaction, PurchaseOrderExt, Transactional};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::fulfillment::{ServiceFulfillment, ServiceFulfillmentData, ServiceFulfillmentState};
use entities::ids::ServiceFulfillmentId;
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use entities::document_registry::DocumentType;

use super::dto::SortDir;
use super::purchase_context::{ensure_allocation_valid, ensure_po_fulfillable, ensure_prepay_gate};
use super::{
    CreateServiceFulfillmentRequest, FulfillmentService, PageView, ServiceFulfillmentListParams,
    ServiceFulfillmentView,
};

/// 线下服务履约记录列表筛选条件类型。
type ServiceFulfillmentFilter = <mongodb::Database as FulfillmentExt>::ServiceFulfillmentFilter;

impl FulfillmentService {
    // ---------------------------------------------------------- service_fulfillment

    /// 分页查询线下服务履约记录列表（W09 服务视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_line_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn service_fulfillment_list(
        &self,
        params: &ServiceFulfillmentListParams,
    ) -> Result<PageView<ServiceFulfillmentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ServiceFulfillmentFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .service_fulfillments()
            .search_service_fulfillments(&filter, &mut NoTransaction)
            .await?;
        let page_ids: Vec<String> = page.items.iter().map(|row| row.id.clone()).collect();
        let allocation_ids = load_service_allocation_ids(&self.db, &page_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ServiceFulfillmentView {
                id: row.id.clone(),
                fulfillment_no: row.fulfillment_no,
                sales_order_line_id: row.sales_order_line_id.to_string(),
                purchase_order_id: row.purchase_order_id.to_string(),
                purchase_line_sales_allocation_id: allocation_ids.get(&row.id).cloned().unwrap_or_default(),
                quantity: row.quantity,
                result: row.result,
                status: row.status,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建线下服务履约记录（草稿；单集合写入，无事务）。
    ///
    /// 服务地点与交付对象快照以不透明值传入，服务端计算查询指纹后落库。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建记录的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 履约记录号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_service_fulfillment(
        &self,
        req: CreateServiceFulfillmentRequest,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        req.validate()?;
        let occurred_at = Instant::from_unix_secs(req.occurred_at);
        let recorded_at = Instant::now();
        let record = ServiceFulfillment::new(
            ServiceFulfillmentId::new(next_id()),
            ServiceFulfillmentData {
                fulfillment_no: req.fulfillment_no,
                sales_order_line_id: req.sales_order_line_id,
                purchase_order_id: req.purchase_order_id,
                purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
                recipient_snapshot: req.recipient_snapshot.clone(),
                recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                    &req.recipient_snapshot,
                    &self.fingerprint_key,
                ),
                quantity: req.quantity,
                result: req.result,
                evidence_attachment_id: req.evidence_attachment_id,
                service_location_encrypted: req.service_location.clone(),
                service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                    &req.service_location,
                    &self.fingerprint_key,
                ),
                service_started_at: req.service_started_at.map(Instant::from_unix_secs),
                service_ended_at: req.service_ended_at.map(Instant::from_unix_secs),
                completion_note: req.completion_note,
                fact_no: next_id(),
                occurred_at,
                recorded_at,
                recorded_by: actor.id().to_string(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "service_fulfillment.create",
            "service_fulfillment",
            record.base.id.clone(),
        )?;
        let document = new_registered_document(
            &record.base.id,
            DocumentType::ServiceFulfillment,
            record.fulfillment_no.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let record_for_tx = record.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.service_fulfillments().create(&record_for_tx, session).await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(record.into())
    }

    /// 确认服务履约（草稿 → 已确认；§8.1.5 + §6.7 跨集合事务）。
    ///
    /// 与电子交付确认相同的门槛与分配有效性校验；重复确认由状态守卫防护。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 记录/采购单/分配不存在
    /// * `ConflictError` - 状态不允许确认或重复确认
    /// * `BusinessLogicError` - 门槛未满足或分配无效
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn confirm_service_fulfillment(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        let record_id = ServiceFulfillmentId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let confirmed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut record = db
                        .service_fulfillments()
                        .find_by_id(record_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("服务履约记录不存在".to_string()))?;
                    if record.status != ServiceFulfillmentState::Draft {
                        return Err(Error::ConflictError(
                            "只有草稿状态的服务履约记录可以确认".to_string(),
                        ));
                    }
                    let po = db
                        .purchase_orders()
                        .find_by_id(record.purchase_order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                    ensure_po_fulfillable(&po)?;
                    ensure_prepay_gate(&db, session, &po).await?;
                    ensure_allocation_valid(
                        &db,
                        session,
                        &po,
                        &record.purchase_line_sales_allocation_id,
                        &record.sales_order_line_id,
                    )
                    .await?;
                    record.confirm()?;
                    db.service_fulfillments().update(&mut record, session).await?;
                    let audit = actor.resource_log(
                        "service_fulfillment.confirm",
                        "service_fulfillment",
                        record_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ServiceFulfillment, crate::errors::Error>(record)
                })
            })
            .await?;
        Ok(confirmed.into())
    }
}

/// 批量取服务履约记录的采购销售分配（P2 投影行未含该字段）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_ids` - 记录主键集合
///
/// # 返回
/// 返回「记录 → 采购销售分配」映射。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn load_service_allocation_ids(
    db: &Database,
    record_ids: &[String],
) -> Result<HashMap<String, String>> {
    if record_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut map = HashMap::new();
    for record in db
        .service_fulfillments()
        .find_many(doc! { "id": { "$in": record_ids } }, &mut NoTransaction)
        .await?
    {
        map.insert(
            record.base.id.clone(),
            record.purchase_line_sales_allocation_id.to_string(),
        );
    }
    Ok(map)
}

impl From<ServiceFulfillment> for ServiceFulfillmentView {
    /// 从服务履约记录实体构造视图。
    fn from(record: ServiceFulfillment) -> Self {
        Self {
            id: record.base.id,
            fulfillment_no: record.fulfillment_no,
            sales_order_line_id: record.sales_order_line_id.to_string(),
            purchase_order_id: record.purchase_order_id.to_string(),
            purchase_line_sales_allocation_id: record.purchase_line_sales_allocation_id.to_string(),
            quantity: record.quantity,
            result: record.result,
            status: record.status,
            occurred_at: record.fact.occurred_at.unix_secs(),
            recorded_at: record.fact.recorded_at.unix_secs(),
            version: record.base.version,
        }
    }
}
