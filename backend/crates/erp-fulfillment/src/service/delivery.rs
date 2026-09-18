//! 发货单查询、草稿构造、领域视图与调用方事务内持久化。

use erp_core::ids::DeliveryId;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::FulfillmentService;
use super::delivery_lines::delivery_line_specs;
use crate::dto::{
    CreateDeliveryRequest, DeliveryDetailView, DeliveryLineView, DeliveryListParams, DeliveryView,
    UpdateDeliveryRequest,
};
use crate::entity::fulfillment::{Delivery, DeliveryData, DeliveryLine, DeliveryLineBatch};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};

/// 发货单列表筛选条件类型。
type DeliveryFilter = <mongodb::Database as FulfillmentExt>::DeliveryFilter;

impl FulfillmentService {
    /// 分页查询发货单列表（W01 履约任务作业面）。
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
    #[tracing::instrument(
        name = "fulfillment.delivery_list",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "delivery_list")
    )]
    pub async fn delivery_list(
        &self,
        params: &DeliveryListParams,
    ) -> Result<crate::dto::PageView<DeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DeliveryFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: super::sort_ascending(query.paging.sort_dir),
        };
        let page = self.db.deliveries().search_deliveries(&filter, &mut NoTransaction).await?;
        super::map_search_page(
            async { Ok(page) },
            |row| DeliveryView {
                id: row.id,
                delivery_no: row.delivery_no,
                delivery_type: row.delivery_type,
                sales_order_id: row.sales_order_id.to_string(),
                purchase_order_id: row.purchase_order_id.map(|id| id.to_string()),
                warehouse_id: row.warehouse_id.map(|id| id.to_string()),
                status: row.status,
                carrier: row.carrier,
                tracking_no: row.tracking_no,
                shipped_at: row.shipped_at.map(|instant| instant.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            },
            filter.page,
            filter.page_size,
        )
        .await
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
    #[tracing::instrument(
        name = "fulfillment.delivery_detail",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "delivery_detail")
    )]
    pub async fn delivery_detail(&self, id: &str) -> Result<DeliveryDetailView> {
        let delivery = super::find_header_or_not_found(
            self.db.deliveries().find_by_id(id, &mut NoTransaction),
            "发货单不存在",
        )
        .await?;
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

    /// 按原请求校验、表头 ID 和逐行 ID 顺序构造发货草稿。
    ///
    /// # 错误
    /// 请求、表头或发货类型与行归属不合法时返回原错误，不执行持久化。
    pub fn prepare_delivery(&self, req: CreateDeliveryRequest) -> Result<(Delivery, Vec<DeliveryLine>)> {
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
        let lines =
            DeliveryLineBatch::build(id.clone(), delivery.delivery_type, 1, delivery_line_specs(&req.lines)?)
                .map_err(Error::Logic)?;
        Ok((delivery, lines))
    }

    /// 在原无事务读取位置加载并修改发货草稿，先校验版本再执行实体更新。
    ///
    /// # 错误
    /// 请求非法、发货单不存在、版本冲突或状态不允许更新时保留原错误。
    pub async fn prepare_delivery_update(&self, id: &str, req: UpdateDeliveryRequest) -> Result<Delivery> {
        req.validate()?;
        let mut delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        if delivery.base.version != req.version {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        delivery.update(crate::entity::fulfillment::DeliveryUpdate {
            carrier: req.carrier,
            tracking_no: req.tracking_no,
        })?;
        Ok(delivery)
    }

    /// 将已构造的发货表头和行写入调用方事务；单据注册必须由组合层先完成。
    ///
    /// # 错误
    /// 表头或行持久化失败时返回原错误，不创建任务或审计。
    pub async fn persist_created_delivery(
        &self,
        delivery: &Delivery,
        lines: &[DeliveryLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.fulfillment().create_delivery_with_lines(delivery, lines, executor).await?;
        Ok(())
    }

    /// 使用调用方执行器 CAS 写回已完成领域修改的发货单。
    ///
    /// # 错误
    /// 版本冲突或仓储写入失败时返回原错误；本方法不创建事务。
    pub async fn persist_delivery(&self, delivery: &mut Delivery, executor: &mut dyn Executor) -> Result<()> {
        self.db.deliveries().update(delivery, executor).await?;
        Ok(())
    }
}

// ------------------------------------------------------------------ private helpers (line rules owned by entities)

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
    use std::str::FromStr;

    use erp_core::ids::{DeliveryId, PurchaseLineSalesAllocationId, SalesOrderLineId, StockReservationId};
    use erp_core::money::Quantity;

    use super::delivery_line_specs;
    use crate::dto::DeliveryLineInput;
    use crate::entity::fulfillment::{DeliveryLineBatch, DeliveryType};

    #[test]
    fn delivery_lines_enforce_type_ownership() {
        let ok = DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            1,
            delivery_line_specs(&[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: Some(StockReservationId::new("rsv-1")),
                purchase_line_sales_allocation_id: None,
            }])
            .unwrap(),
        )
        .unwrap();
        assert_eq!(ok.len(), 1);
        let wrong = DeliveryLineBatch::build(
            DeliveryId::new("d-2"),
            DeliveryType::WarehouseShip,
            1,
            delivery_line_specs(&[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("a-1")),
            }])
            .unwrap(),
        );
        assert!(wrong.is_err(), "仓发不得携带直发分配");
    }

    /// 创建路径经实体批量工厂编号：旧 Service helper 已删除。
    #[test]
    fn delivery_create_uses_entity_batch_factory() {
        let production = include_str!("delivery.rs").split("#[cfg(test)]").next().expect("生产代码");
        assert!(!production.contains("fn build_delivery_lines"), "旧 helper 必须删除");
        assert!(production.contains("DeliveryLineBatch::build"), "创建路径必须调用实体工厂");
    }
}
