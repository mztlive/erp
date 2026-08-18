use std::collections::HashMap;

use database::{AccessControlExt, FulfillmentExt, NoTransaction, PurchaseOrderExt, Transactional};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::fulfillment::{ElectronicDelivery, ElectronicDeliveryData, ElectronicDeliveryState};
use entities::ids::ElectronicDeliveryId;
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
    CreateElectronicDeliveryRequest, ElectronicDeliveryListParams, ElectronicDeliveryView,
    FulfillmentService, PageView,
};

/// 电子交付记录列表筛选条件类型。
type ElectronicDeliveryFilter = <mongodb::Database as FulfillmentExt>::ElectronicDeliveryFilter;

impl FulfillmentService {
    // ----------------------------------------------------------- electronic_delivery

    /// 分页查询电子交付记录列表（W09 电子交付视图）。
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
    pub async fn electronic_delivery_list(
        &self,
        params: &ElectronicDeliveryListParams,
    ) -> Result<PageView<ElectronicDeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ElectronicDeliveryFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .electronic_deliveries()
            .search_electronic_deliveries(&filter, &mut NoTransaction)
            .await?;
        let page_ids: Vec<String> = page.items.iter().map(|row| row.id.clone()).collect();
        let allocation_ids = load_electronic_allocation_ids(&self.db, &page_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ElectronicDeliveryView {
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

    /// 创建电子交付记录（草稿；单集合写入，无事务）。
    ///
    /// 交付对象快照以不透明值传入，服务端用指纹密钥计算查询指纹后落库；
    /// 快照的字段级加密由边界（前端/接入层）完成。
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
    pub async fn create_electronic_delivery(
        &self,
        req: CreateElectronicDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        req.validate()?;
        let occurred_at = Instant::from_unix_secs(req.occurred_at);
        let recorded_at = Instant::now();
        let record = ElectronicDelivery::new(
            ElectronicDeliveryId::new(next_id()),
            ElectronicDeliveryData {
                fulfillment_no: req.fulfillment_no,
                sales_order_line_id: req.sales_order_line_id,
                purchase_order_id: req.purchase_order_id,
                purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
                recipient_snapshot: req.recipient_snapshot.clone(),
                recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                    &req.recipient_snapshot,
                    &self.fingerprint_key,
                ),
                quantity: req.quantity,
                result: req.result,
                evidence_attachment_id: req.evidence_attachment_id,
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
            "electronic_delivery.create",
            "electronic_delivery",
            record.base.id.clone(),
        )?;
        let document = new_registered_document(
            &record.base.id,
            DocumentType::ElectronicDelivery,
            record.fulfillment_no.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let record_for_tx = record.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.electronic_deliveries().create(&record_for_tx, session).await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(record.into())
    }

    /// 确认电子交付（草稿 → 已确认；§8.1.5 + §6.7 跨集合事务）。
    ///
    /// 在同一事务内：校验采购单可履约与 `PREPAY` 门槛、校验采购销售分配的
    /// 有效性（采购行归属当前生效版本、销售行归属本明细）、迁移记录状态、写
    /// 审计。重复确认由状态守卫（仅草稿）防护。
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
    pub async fn confirm_electronic_delivery(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        let record_id = ElectronicDeliveryId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let confirmed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut record = db
                        .electronic_deliveries()
                        .find_by_id(record_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("电子交付记录不存在".to_string()))?;
                    if record.status != ElectronicDeliveryState::Draft {
                        return Err(Error::ConflictError(
                            "只有草稿状态的电子交付记录可以确认".to_string(),
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
                    db.electronic_deliveries().update(&mut record, session).await?;
                    let audit = actor.resource_log(
                        "electronic_delivery.confirm",
                        "electronic_delivery",
                        record_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ElectronicDelivery, crate::errors::Error>(record)
                })
            })
            .await?;
        Ok(confirmed.into())
    }
}

/// 批量取电子交付记录的采购销售分配（P2 投影行未含该字段）。
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
async fn load_electronic_allocation_ids(
    db: &Database,
    record_ids: &[String],
) -> Result<HashMap<String, String>> {
    if record_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut map = HashMap::new();
    for record in db
        .electronic_deliveries()
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

impl From<ElectronicDelivery> for ElectronicDeliveryView {
    /// 从电子交付记录实体构造视图。
    fn from(record: ElectronicDelivery) -> Self {
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
