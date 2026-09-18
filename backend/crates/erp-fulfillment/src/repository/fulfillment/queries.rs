//! 跨集合批量查询：`list_*`、按字段 `$in` 取行、草稿查找。
//!
//! 空 `$in` 短路、查询文档与 tracing span 名与拆分前一致。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{
    CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId, ElectronicDeliveryId, PurchaseReceiptId,
    SalesOrderId, SalesOrderLineId, ServiceFulfillmentId, WarehouseId,
};
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use super::{
    ACCEPTANCE_FULFILLMENT_ALLOCATIONS, CUSTOMER_ACCEPTANCE_LINES, DELIVERY_LINES, FulfillmentRepository,
    PURCHASE_RECEIPT_LINES,
};
use crate::entity::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptance, CustomerAcceptanceLine, Delivery, DeliveryLine,
    DeliveryState, DeliveryType, ElectronicDelivery, ElectronicDeliveryState, FulfillmentFactType,
    PurchaseReceiptLine, ServiceFulfillment, ServiceFulfillmentState,
};
use crate::repository::extensions::FulfillmentExt;

impl FulfillmentRepository<'_> {
    /// 查询销售单下可进入客户验收的发货单。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回状态为已发货或已签收的发货单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.list_acceptance_eligible_deliveries",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.operation.name = "find"
        )
    )]
    pub async fn list_acceptance_eligible_deliveries(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<Delivery>> {
        let states: Vec<&str> =
            DeliveryState::acceptance_eligible_states().iter().map(DeliveryState::as_str).collect();
        mongo_ops::find_many(
            &self.db.collection::<Delivery>(<mongodb::Database as FulfillmentExt>::DELIVERIES),
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "status": { "$in": states },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 查询销售明细集合下已确认的电子交付事实。
    ///
    /// # 参数
    /// * `sales_order_line_ids` - 销售稳定明细主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回已确认电子交付记录；空主键集合直接返回空列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.list_confirmed_electronic_deliveries",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.collection.name = "electronic_deliveries",
            db.operation.name = "find"
        )
    )]
    pub async fn list_confirmed_electronic_deliveries(
        &self,
        sales_order_line_ids: &[SalesOrderLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ElectronicDelivery>> {
        list_by_ids_and_status(
            self.db,
            <mongodb::Database as FulfillmentExt>::ELECTRONIC_DELIVERIES,
            "sales_order_line_id",
            &ids_to_strings(sales_order_line_ids),
            ElectronicDeliveryState::Confirmed.as_str(),
            executor,
        )
        .await
    }

    /// 查询销售明细集合下已确认的服务履约事实。
    ///
    /// # 参数
    /// * `sales_order_line_ids` - 销售稳定明细主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回已确认服务履约记录；空主键集合直接返回空列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.list_confirmed_service_fulfillments",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.collection.name = "service_fulfillments",
            db.operation.name = "find"
        )
    )]
    pub async fn list_confirmed_service_fulfillments(
        &self,
        sales_order_line_ids: &[SalesOrderLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ServiceFulfillment>> {
        list_by_ids_and_status(
            self.db,
            <mongodb::Database as FulfillmentExt>::SERVICE_FULFILLMENTS,
            "sales_order_line_id",
            &ids_to_strings(sales_order_line_ids),
            ServiceFulfillmentState::Confirmed.as_str(),
            executor,
        )
        .await
    }

    /// 查询销售单的客户验收历史。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按验收时间倒序排列的验收单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.list_customer_acceptance_history",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.operation.name = "find"
        )
    )]
    pub async fn list_customer_acceptance_history(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAcceptance>> {
        mongo_ops::find_many(
            &self.db.collection::<CustomerAcceptance>(
                <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCES,
            ),
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "accepted_at": -1 }).build(),
            executor,
        )
        .await
    }

    /// 按主键批量读取发货单。
    ///
    /// # 参数
    /// * `delivery_ids` - 发货单主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的发货单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_deliveries_by_ids(
        &self,
        delivery_ids: &[DeliveryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Delivery>> {
        find_lines_in(
            self.db,
            <mongodb::Database as FulfillmentExt>::DELIVERIES,
            "id",
            &ids_to_strings(delivery_ids),
            executor,
        )
        .await
    }

    /// 按主键批量读取电子交付记录。
    ///
    /// # 参数
    /// * `record_ids` - 电子交付主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的电子交付记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_electronic_deliveries_by_ids(
        &self,
        record_ids: &[ElectronicDeliveryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ElectronicDelivery>> {
        find_lines_in(
            self.db,
            <mongodb::Database as FulfillmentExt>::ELECTRONIC_DELIVERIES,
            "id",
            &ids_to_strings(record_ids),
            executor,
        )
        .await
    }

    /// 按主键批量读取服务履约记录。
    ///
    /// # 参数
    /// * `record_ids` - 服务履约主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的服务履约记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_service_fulfillments_by_ids(
        &self,
        record_ids: &[ServiceFulfillmentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ServiceFulfillment>> {
        find_lines_in(
            self.db,
            <mongodb::Database as FulfillmentExt>::SERVICE_FULFILLMENTS,
            "id",
            &ids_to_strings(record_ids),
            executor,
        )
        .await
    }

    /// 查询销售单现有仓发草稿。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回任一未删除草稿发货单；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.draft_delivery_for_sales_order",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.operation.name = "find"
        )
    )]
    pub async fn draft_delivery_for_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<Delivery>> {
        mongo_ops::find_one(
            &self.db.collection::<Delivery>(<mongodb::Database as FulfillmentExt>::DELIVERIES),
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "status": DeliveryState::Draft.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 查询销售单与仓库维度的现有仓发草稿。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单主键
    /// * `warehouse_id` - 发货仓库主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配的未删除仓发草稿；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.draft_warehouse_delivery",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.operation.name = "find"
        )
    )]
    pub async fn draft_warehouse_delivery(
        &self,
        sales_order_id: &SalesOrderId,
        warehouse_id: &WarehouseId,
        executor: &mut dyn Executor,
    ) -> Result<Option<Delivery>> {
        mongo_ops::find_one(
            &self.db.collection::<Delivery>(<mongodb::Database as FulfillmentExt>::DELIVERIES),
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "warehouse_id": warehouse_id.to_string(),
                "delivery_type": DeliveryType::WarehouseShip.as_str(),
                "status": DeliveryState::Draft.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 批量读取采购入库行（`$in` 一次取回，按行号升序）。
    ///
    /// 供单据详情/过账计算一次性加载全部行，禁止按表头逐条查询造成 N+1。
    ///
    /// # 参数
    /// * `receipt_ids` - 入库单主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配行，按 `line_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn receipt_lines_by_receipt_ids(
        &self,
        receipt_ids: &[PurchaseReceiptId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReceiptLine>> {
        let mut lines = find_lines_in(
            self.db,
            PURCHASE_RECEIPT_LINES,
            "purchase_receipt_id",
            &ids_to_strings(receipt_ids),
            executor,
        )
        .await?;
        lines.sort_by_key(|line: &PurchaseReceiptLine| (line.purchase_receipt_id.to_string(), line.line_no));
        Ok(lines)
    }

    /// 批量读取发货行（`$in` 一次取回，按行号升序）。
    ///
    /// 供单据详情/发货过账计算一次性加载全部行，禁止按表头逐条查询造成 N+1。
    ///
    /// # 参数
    /// * `delivery_ids` - 发货单主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配行，按 `line_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn delivery_lines_by_delivery_ids(
        &self,
        delivery_ids: &[DeliveryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<DeliveryLine>> {
        let mut lines =
            find_lines_in(self.db, DELIVERY_LINES, "delivery_id", &ids_to_strings(delivery_ids), executor)
                .await?;
        lines.sort_by_key(|line: &DeliveryLine| (line.delivery_id.to_string(), line.line_no));
        Ok(lines)
    }

    /// 批量读取客户验收行（`$in` 一次取回，按行号升序）。
    ///
    /// 供单据详情/验收过账计算一次性加载全部行，禁止按表头逐条查询造成 N+1。
    ///
    /// # 参数
    /// * `acceptance_ids` - 验收单主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配行，按 `line_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn acceptance_lines_by_acceptance_ids(
        &self,
        acceptance_ids: &[CustomerAcceptanceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAcceptanceLine>> {
        let mut lines = find_lines_in(
            self.db,
            CUSTOMER_ACCEPTANCE_LINES,
            "customer_acceptance_id",
            &ids_to_strings(acceptance_ids),
            executor,
        )
        .await?;
        lines.sort_by_key(|line: &CustomerAcceptanceLine| {
            (line.customer_acceptance_id.to_string(), line.line_no)
        });
        Ok(lines)
    }

    /// 批量读取验收履约分配（按验收行 `$in` 一次取回）。
    ///
    /// 供净验收数量（`APPLY - REVERSE`）计算一次性取回全部分配，禁止 N+1。
    ///
    /// # 参数
    /// * `acceptance_line_ids` - 验收行主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn allocations_by_acceptance_lines(
        &self,
        acceptance_line_ids: &[CustomerAcceptanceLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AcceptanceFulfillmentAllocation>> {
        find_lines_in(
            self.db,
            ACCEPTANCE_FULFILLMENT_ALLOCATIONS,
            "customer_acceptance_line_id",
            &ids_to_strings(acceptance_line_ids),
            executor,
        )
        .await
    }

    /// 批量读取验收履约分配（按履约事实 `$in` 一次取回）。
    ///
    /// 供关单「每履约事实净验收数量不超过净成功履约数量」校验取数，禁止 N+1。
    ///
    /// # 参数
    /// * `fact_type` - 履约事实类型（发货/电子交付/服务履约）
    /// * `fulfillment_line_ids` - 履约事实行主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn allocations_by_fulfillment_fact(
        &self,
        fact_type: FulfillmentFactType,
        fulfillment_line_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AcceptanceFulfillmentAllocation>> {
        if fulfillment_line_ids.is_empty() {
            return Ok(Vec::new());
        }
        let collection =
            self.db.collection::<AcceptanceFulfillmentAllocation>(ACCEPTANCE_FULFILLMENT_ALLOCATIONS);
        mongo_ops::find_many(
            &collection,
            doc! {
                "fulfillment_fact_type": fact_type.as_str(),
                "fulfillment_line_id": { "$in": fulfillment_line_ids },
            },
            FindOptions::default(),
            executor,
        )
        .await
    }
}

/// 把 ID newtype 集合转为字符串集合（用于 `$in` 查询）。
///
/// # 参数
/// * `ids` - ID newtype 集合
///
/// # 返回
/// 返回字符串集合。
fn ids_to_strings<T: AsRef<str>>(ids: &[T]) -> Vec<String> {
    ids.iter().map(|id| id.as_ref().to_string()).collect()
}

/// 按给定字段与状态批量读取实体。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `collection_name` - 业务实体集合名
/// * `field` - 主键或关联字段名
/// * `values` - 待匹配的字段值集合
/// * `status` - 待匹配的稳定状态代码
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回字段值与状态同时匹配且未删除的实体；空值集合直接返回空列表。
///
/// # 错误
/// 当 MongoDB 查询或游标读取失败时返回错误。
async fn list_by_ids_and_status<T>(
    db: &Database,
    collection_name: &str,
    field: &str,
    values: &[String],
    status: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }
    mongo_ops::find_many(
        &db.collection::<T>(collection_name),
        doc! {
            field: { "$in": values },
            "status": status,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        },
        FindOptions::default(),
        executor,
    )
    .await
}

/// 按给定字段 `$in` 批量读取行实体（空集合直接返回空列表）。
async fn find_lines_in<T>(
    db: &Database,
    collection_name: &str,
    field: &str,
    values: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let collection = db.collection::<T>(collection_name);
    mongo_ops::find_many(
        &collection,
        doc! {
            field: { "$in": values },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        },
        FindOptions::default(),
        executor,
    )
    .await
}

#[cfg(test)]
mod tests {
    use erp_core::ids::PurchaseOrderId;

    use super::ids_to_strings;

    #[test]
    fn ids_to_strings_converts_newtype_collection() {
        let ids = vec![PurchaseOrderId::new("po-1"), PurchaseOrderId::new("po-2")];
        assert_eq!(ids_to_strings(&ids), vec!["po-1".to_string(), "po-2".to_string()]);
        assert!(ids_to_strings::<PurchaseOrderId>(&[]).is_empty());
    }
}
