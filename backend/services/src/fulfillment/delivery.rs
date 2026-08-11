use std::collections::HashMap;

use database::{AccessControlExt, FulfillmentExt, NoTransaction, Transactional};
use entities::fulfillment::{Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryType};
use entities::ids::{DeliveryId, DeliveryLineId};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::SortDir;
use super::{
    CreateDeliveryRequest, DeliveryDetailView, DeliveryLineInput, DeliveryLineView, DeliveryListParams,
    DeliveryView, FulfillmentService, PageView, UpdateDeliveryRequest,
};

/// 发货单列表筛选条件类型。
type DeliveryFilter = <mongodb::Database as FulfillmentExt>::DeliveryFilter;

impl FulfillmentService {
    // ------------------------------------------------------------------- delivery

    /// 分页查询发货单列表（W09 发货视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn delivery_list(&self, params: &DeliveryListParams) -> Result<PageView<DeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DeliveryFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .deliveries()
            .search_deliveries(&filter, &mut NoTransaction)
            .await?;
        let direct_ids: Vec<String> = page
            .items
            .iter()
            .filter(|row| row.delivery_type == DeliveryType::SupplierDirect)
            .map(|row| row.id.clone())
            .collect();
        let direct_po_ids = load_direct_po_ids(&self.db, &direct_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| DeliveryView {
                id: row.id.clone(),
                delivery_no: row.delivery_no,
                delivery_type: row.delivery_type,
                sales_order_id: row.sales_order_id.to_string(),
                purchase_order_id: direct_po_ids.get(&row.id).cloned().flatten(),
                warehouse_id: row.warehouse_id.map(|id| id.to_string()),
                status: row.status,
                carrier: row.carrier,
                tracking_no: row.tracking_no,
                shipped_at: row.shipped_at.map(|instant| instant.unix_secs()),
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

    /// 查询发货单详情（表头 + 行）。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    ///
    /// # 返回
    /// 返回发货单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn delivery_detail(&self, id: &str) -> Result<DeliveryDetailView> {
        let delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&[delivery.base.id.clone().into()], &mut NoTransaction)
            .await?;
        Ok(DeliveryDetailView {
            delivery: delivery.clone().into(),
            lines: lines.into_iter().map(Into::into).collect(),
        })
    }

    /// 创建发货单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 仓发/直发的表头与行归属由实体按发货类型校验；预占与采购分配的存在性
    /// 在过账时校验。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发货单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_delivery(
        &self,
        req: CreateDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<DeliveryView> {
        req.validate()?;
        let id = DeliveryId::new(next_id());
        let delivery = Delivery::new(
            id.clone(),
            DeliveryData {
                delivery_no: req.delivery_no,
                delivery_type: req.delivery_type,
                sales_order_id: req.sales_order_id,
                purchase_order_id: req.purchase_order_id,
                warehouse_id: req.warehouse_id,
                carrier: req.carrier,
                tracking_no: req.tracking_no,
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )?;
        let lines = build_delivery_lines(&id, delivery.delivery_type, &req.lines)?;
        let audit = actor
            .clone()
            .resource_log("delivery.create", "delivery", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_for_tx = delivery.clone();
        let lines_for_tx = lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.fulfillment()
                        .create_delivery_with_lines(&delivery_for_tx, &lines_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(delivery.into())
    }

    /// 更新发货单（仅草稿；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后发货单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_delivery(
        &self,
        id: &str,
        req: UpdateDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<DeliveryView> {
        req.validate()?;
        let mut delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        if delivery.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        delivery.update(entities::fulfillment::DeliveryUpdate {
            carrier: req.carrier,
            tracking_no: req.tracking_no,
        })?;
        let audit = actor
            .clone()
            .resource_log("delivery.update", "delivery", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.deliveries().update(&mut delivery, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Delivery, crate::errors::Error>(delivery)
                })
            })
            .await?;
        Ok(updated.into())
    }
}

// ------------------------------------------------------------------ private helpers

/// 批量取供应商直发货单的采购来源（P2 投影行未含 `purchase_order_id`）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `delivery_ids` - 供应商直发的主键集合（仓发无需查询）
///
/// # 返回
/// 返回「发货单 → 采购来源」映射。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn load_direct_po_ids(
    db: &Database,
    delivery_ids: &[String],
) -> Result<HashMap<String, Option<String>>> {
    if delivery_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut map = HashMap::new();
    for delivery in db
        .deliveries()
        .find_many(doc! { "id": { "$in": delivery_ids } }, &mut NoTransaction)
        .await?
    {
        map.insert(
            delivery.base.id.clone(),
            delivery.purchase_order_id.map(|id| id.to_string()),
        );
    }
    Ok(map)
}

/// 构建发货行实体集合（行号从 1 递增）。
///
/// # 参数
/// * `delivery_id` - 发货单主键
/// * `delivery_type` - 发货类型（决定行级归属校验）
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行归属与发货类型不一致或数量非正时返回错误（实体构造）。
fn build_delivery_lines(
    delivery_id: &DeliveryId,
    delivery_type: DeliveryType,
    inputs: &[DeliveryLineInput],
) -> Result<Vec<DeliveryLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        lines.push(
            DeliveryLine::new(
                DeliveryLineId::new(next_id()),
                DeliveryLineData {
                    delivery_id: delivery_id.clone(),
                    line_no: index as u32 + 1,
                    sales_order_line_id: input.sales_order_line_id.clone(),
                    quantity: input.quantity,
                    stock_reservation_id: input.stock_reservation_id.clone(),
                    purchase_line_sales_allocation_id: input.purchase_line_sales_allocation_id.clone(),
                },
                delivery_type,
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

impl From<Delivery> for DeliveryView {
    /// 从发货单实体构造视图。
    fn from(delivery: Delivery) -> Self {
        Self {
            id: delivery.base.id,
            delivery_no: delivery.delivery_no,
            delivery_type: delivery.delivery_type,
            sales_order_id: delivery.sales_order_id.to_string(),
            purchase_order_id: delivery.purchase_order_id.map(|id| id.to_string()),
            warehouse_id: delivery.warehouse_id.map(|id| id.to_string()),
            status: delivery.status,
            carrier: delivery.carrier,
            tracking_no: delivery.tracking_no,
            shipped_at: delivery.shipped_at.map(|instant| instant.unix_secs()),
            version: delivery.base.version,
            created_at: delivery.base.created_at,
        }
    }
}

impl From<DeliveryLine> for DeliveryLineView {
    /// 从发货行实体构造视图。
    fn from(line: DeliveryLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            sales_order_line_id: line.sales_order_line_id.to_string(),
            quantity: line.quantity,
            stock_reservation_id: line.stock_reservation_id.map(|id| id.to_string()),
            purchase_line_sales_allocation_id: line
                .purchase_line_sales_allocation_id
                .map(|id| id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_delivery_lines;
    use crate::fulfillment::DeliveryLineInput;
    use entities::fulfillment::DeliveryType;
    use entities::ids::{DeliveryId, PurchaseLineSalesAllocationId, SalesOrderLineId, StockReservationId};
    use entities::money::Quantity;
    use std::str::FromStr;

    #[test]
    fn delivery_lines_enforce_type_ownership() {
        let ok = build_delivery_lines(
            &DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            &[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: Some(StockReservationId::new("rsv-1")),
                purchase_line_sales_allocation_id: None,
            }],
        )
        .unwrap();
        assert_eq!(ok.len(), 1);
        let wrong = build_delivery_lines(
            &DeliveryId::new("d-2"),
            DeliveryType::WarehouseShip,
            &[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("a-1")),
            }],
        );
        assert!(wrong.is_err(), "仓发不得携带直发分配");
    }
}
