//! 域 D16 `fulfillment` 仓储：purchase_receipt(+_line)、delivery(+_line)、
//! electronic_delivery、service_fulfillment、customer_acceptance(+_line)、
//! acceptance_fulfillment_allocation（页面：W06、W09）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `FulfillmentExt` 关联常量导入。
//!
//! 软删除边界（§4.5）：草稿单据（采购入库单等）可逻辑删除；已过账/已发货/
//! 已确认/已冲正及分配（`electronic_delivery`、`service_fulfillment`、
//! `acceptance_fulfillment_allocation`）是正式事实，**不提供软删除方法**
//! （基类通用方法不属于本域契约）。
//!
//! 筛选/行类型定义在本文件，经 `FulfillmentExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use entities::common::time::Instant;
use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, AcceptanceResult, CustomerAcceptance, CustomerAcceptanceLine,
    CustomerAcceptanceState, Delivery, DeliveryLine, DeliveryState, DeliveryType, ElectronicDelivery,
    ElectronicDeliveryState, FulfillmentFactType, FulfillmentResult, PurchaseReceipt, PurchaseReceiptLine,
    PurchaseReceiptState, ServiceFulfillment, ServiceFulfillmentState,
};
use entities::ids::{
    CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId, PurchaseOrderId, PurchaseReceiptId,
    SalesOrderId, SalesOrderLineId, WarehouseId,
};

use super::extensions::FulfillmentExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `purchase_receipt_line` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const PURCHASE_RECEIPT_LINES: &str = <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPT_LINES;
/// `delivery_line` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const DELIVERY_LINES: &str = <mongodb::Database as FulfillmentExt>::DELIVERY_LINES;
/// `customer_acceptance_line` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const CUSTOMER_ACCEPTANCE_LINES: &str = <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCE_LINES;
/// `acceptance_fulfillment_allocation` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const ACCEPTANCE_FULFILLMENT_ALLOCATIONS: &str =
    <mongodb::Database as FulfillmentExt>::ACCEPTANCE_FULFILLMENT_ALLOCATIONS;

/// 采购入库单列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReceiptRow {
    /// 实体主键。
    pub id: String,
    /// 采购入库单号。
    pub receipt_no: String,
    /// 来源采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 入库仓。
    pub warehouse_id: WarehouseId,
    /// 当前状态。
    pub status: PurchaseReceiptState,
    /// 入库过账时间。
    pub posted_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购入库单列表筛选条件。
#[derive(Debug, Clone)]
pub struct PurchaseReceiptFilter {
    /// 来源采购单；`None` 表示不筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 单据状态；`None` 表示不筛选。
    pub status: Option<PurchaseReceiptState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PurchaseReceiptFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(purchase_order_id) = &self.purchase_order_id {
            filter.insert("purchase_order_id", purchase_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PurchaseReceiptFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 发货单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 履约发货单号。
    pub delivery_no: String,
    /// 发货类型。
    pub delivery_type: DeliveryType,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 入库仓（仓发）。
    pub warehouse_id: Option<WarehouseId>,
    /// 当前状态。
    pub status: DeliveryState,
    /// 物流承运方。
    pub carrier: Option<String>,
    /// 物流单号。
    pub tracking_no: Option<String>,
    /// 发货时间。
    pub shipped_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发货单列表筛选条件。
#[derive(Debug, Clone)]
pub struct DeliveryFilter {
    /// 销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态；`None` 表示不筛选。
    pub status: Option<DeliveryState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for DeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for DeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 电子交付记录列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ElectronicDeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 交付数量。
    pub quantity: entities::money::Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 当前状态。
    pub status: ElectronicDeliveryState,
    /// 实际交付时间。
    pub occurred_at: Instant,
    /// ERP 记录时间。
    pub recorded_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
}

/// 电子交付记录列表筛选条件。
#[derive(Debug, Clone)]
pub struct ElectronicDeliveryFilter {
    /// 销售责任明细；`None` 表示不筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 记录状态；`None` 表示不筛选。
    pub status: Option<ElectronicDeliveryState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `occurred_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ElectronicDeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_line_id) = &self.sales_order_line_id {
            filter.insert("sales_order_line_id", sales_order_line_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ElectronicDeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 线下服务履约记录列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceFulfillmentRow {
    /// 实体主键。
    pub id: String,
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 服务数量。
    pub quantity: entities::money::Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 当前状态。
    pub status: ServiceFulfillmentState,
    /// 实际服务时间。
    pub occurred_at: Instant,
    /// ERP 记录时间。
    pub recorded_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
}

/// 线下服务履约记录列表筛选条件。
#[derive(Debug, Clone)]
pub struct ServiceFulfillmentFilter {
    /// 销售责任明细；`None` 表示不筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 记录状态；`None` 表示不筛选。
    pub status: Option<ServiceFulfillmentState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `occurred_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ServiceFulfillmentFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_line_id) = &self.sales_order_line_id {
            filter.insert("sales_order_line_id", sales_order_line_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ServiceFulfillmentFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 客户验收单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAcceptanceRow {
    /// 实体主键。
    pub id: String,
    /// 客户验收单号。
    pub acceptance_no: String,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收时间。
    pub accepted_at: Instant,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 当前状态。
    pub status: CustomerAcceptanceState,
    /// 误录验收的反向事实。
    pub reversal_of_acceptance_id: Option<CustomerAcceptanceId>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户验收单列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerAcceptanceFilter {
    /// 销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态；`None` 表示不筛选。
    pub status: Option<CustomerAcceptanceState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `accepted_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CustomerAcceptanceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for CustomerAcceptanceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PurchaseReceipt> {
    /// 分页检索采购入库单列表（投影查询）。
    ///
    /// 只返回 [`PurchaseReceiptRow`] 所需的列表字段，不加载整文档；排序字段
    /// 走白名单映射（`created_at`/`posted_at`），白名单外的字段名回落默认值。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_purchase_receipts(
        &self,
        filter: &PurchaseReceiptFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseReceiptRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "posted_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(purchase_receipt_projection())
            .build();
        let collection = self.collection().clone_with_type::<PurchaseReceiptRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按采购入库单号查找入库单（唯一单号，详情查询）。
    ///
    /// # 参数
    /// * `receipt_no` - 采购入库单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除入库单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_receipt_no(
        &self,
        receipt_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseReceipt>> {
        self.find_one_by_field("receipt_no", receipt_no, executor).await
    }
}

impl<'a> Repository<'a, Delivery> {
    /// 分页检索发货单列表（投影查询）。
    ///
    /// 只返回 [`DeliveryRow`] 所需的列表字段（敏感履约地址字段不进投影）；
    /// 排序字段走白名单映射（`created_at`/`shipped_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_deliveries(
        &self,
        filter: &DeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<DeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "shipped_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(delivery_projection())
            .build();
        let collection = self.collection().clone_with_type::<DeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按物流单号查找发货单（`idx_deliveries_tracking_no`，详情查询）。
    ///
    /// 物流单号不保证全局唯一，返回全部未删除匹配项。
    ///
    /// # 参数
    /// * `tracking_no` - 物流单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除发货单集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_tracking_no(
        &self,
        tracking_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<Delivery>> {
        self.find_many(doc! { "tracking_no": tracking_no }, executor)
            .await
    }
}

impl<'a> Repository<'a, ElectronicDelivery> {
    /// 分页检索电子交付记录列表（投影查询）。
    ///
    /// 只返回 [`ElectronicDeliveryRow`] 所需的列表字段（交付对象快照及其指纹
    /// 不进投影）；排序字段走白名单映射（`occurred_at`/`recorded_at`/`created_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_electronic_deliveries(
        &self,
        filter: &ElectronicDeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ElectronicDeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["occurred_at", "recorded_at", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(electronic_delivery_projection())
            .build();
        let collection = self.collection().clone_with_type::<ElectronicDeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, ServiceFulfillment> {
    /// 分页检索线下服务履约记录列表（投影查询）。
    ///
    /// 只返回 [`ServiceFulfillmentRow`] 所需的列表字段（交付对象快照、服务
    /// 地点及其指纹不进投影）；排序字段走白名单映射
    /// （`occurred_at`/`recorded_at`/`created_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_service_fulfillments(
        &self,
        filter: &ServiceFulfillmentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ServiceFulfillmentRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["occurred_at", "recorded_at", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(service_fulfillment_projection())
            .build();
        let collection = self.collection().clone_with_type::<ServiceFulfillmentRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, CustomerAcceptance> {
    /// 分页检索客户验收单列表（投影查询）。
    ///
    /// 只返回 [`CustomerAcceptanceRow`] 所需的列表字段，不加载整文档；排序
    /// 字段走白名单映射（`accepted_at`/`created_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_customer_acceptances(
        &self,
        filter: &CustomerAcceptanceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerAcceptanceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["accepted_at", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(customer_acceptance_projection())
            .build();
        let collection = self.collection().clone_with_type::<CustomerAcceptanceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按客户验收单号查找验收单（唯一单号，详情查询）。
    ///
    /// # 参数
    /// * `acceptance_no` - 客户验收单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除验收单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_acceptance_no(
        &self,
        acceptance_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAcceptance>> {
        self.find_one_by_field("acceptance_no", acceptance_no, executor)
            .await
    }
}

/// D16 域专用仓储：跨集合批量查询与多步骤事务写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型承载按表头批量取行（`$in`
/// 一次取回，禁止 N+1）与依赖事务的跨集合原子写入入口，由
/// `FulfillmentExt::fulfillment()` 访问。
pub struct FulfillmentRepository<'a> {
    db: &'a Database,
}

impl<'a> FulfillmentRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
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
        let mut lines = find_lines_in(
            self.db,
            DELIVERY_LINES,
            "delivery_id",
            &ids_to_strings(delivery_ids),
            executor,
        )
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
        let collection = self
            .db
            .collection::<AcceptanceFulfillmentAllocation>(ACCEPTANCE_FULFILLMENT_ALLOCATIONS);
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

    /// 创建采购入库单及全部行（跨集合多步骤写入）。
    ///
    /// 依次写入 `purchase_receipts` 与 `purchase_receipt_lines`，保证表头与行
    /// 原子可见（§6.7）。**必须收到事务执行器**：本方法不构成原子边界，传入
    /// `NoTransaction` 时两笔写入各自自动提交，中途失败会留下只有表头没有行的
    /// 半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `receipt` - 待写入的入库单表头
    /// * `lines` - 待写入的入库行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_purchase_receipt_with_lines(
        &self,
        receipt: &PurchaseReceipt,
        lines: &[PurchaseReceiptLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PurchaseReceipt>(<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS),
            receipt,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<PurchaseReceiptLine>(PURCHASE_RECEIPT_LINES),
            lines.to_vec(),
            executor,
        )
        .await
    }

    /// 创建发货单及全部行（跨集合多步骤写入）。
    ///
    /// 依次写入 `deliveries` 与 `delivery_lines`，保证表头与行原子可见（§6.7）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，中途失败会留下只有表头没有行的半成品；Service
    /// 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `delivery` - 待写入的发货单表头
    /// * `lines` - 待写入的发货行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_delivery_with_lines(
        &self,
        delivery: &Delivery,
        lines: &[DeliveryLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<Delivery>(<mongodb::Database as FulfillmentExt>::DELIVERIES),
            delivery,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<DeliveryLine>(DELIVERY_LINES),
            lines.to_vec(),
            executor,
        )
        .await
    }

    /// 创建客户验收单及全部行（跨集合多步骤写入）。
    ///
    /// 依次写入 `customer_acceptances` 与 `customer_acceptance_lines`，保证
    /// 表头与行原子可见（§6.7）。**必须收到事务执行器**：本方法不构成原子
    /// 边界，传入 `NoTransaction` 时两笔写入各自自动提交，中途失败会留下只有
    /// 表头没有行的半成品；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `acceptance` - 待写入的验收单表头
    /// * `lines` - 待写入的验收行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_customer_acceptance_with_lines(
        &self,
        acceptance: &CustomerAcceptance,
        lines: &[CustomerAcceptanceLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<CustomerAcceptance>(
                <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCES,
            ),
            acceptance,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<CustomerAcceptanceLine>(CUSTOMER_ACCEPTANCE_LINES),
            lines.to_vec(),
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

/// 构建排序文档（字段名白名单映射）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    doc! { field: direction }
}

/// 采购入库单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn purchase_receipt_projection() -> Document {
    doc! {
        "id": 1,
        "receipt_no": 1,
        "purchase_order_id": 1,
        "warehouse_id": 1,
        "status": 1,
        "posted_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 发货单列表投影字段（履约地址敏感字段不进投影）。
///
/// # 返回
/// 返回投影条件文档。
fn delivery_projection() -> Document {
    doc! {
        "id": 1,
        "delivery_no": 1,
        "delivery_type": 1,
        "sales_order_id": 1,
        "warehouse_id": 1,
        "status": 1,
        "carrier": 1,
        "tracking_no": 1,
        "shipped_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 电子交付记录列表投影字段（交付对象快照及指纹不进投影）。
///
/// # 返回
/// 返回投影条件文档。
fn electronic_delivery_projection() -> Document {
    doc! {
        "id": 1,
        "fulfillment_no": 1,
        "sales_order_line_id": 1,
        "purchase_order_id": 1,
        "quantity": 1,
        "result": 1,
        "status": 1,
        "occurred_at": 1,
        "recorded_at": 1,
        "version": 1,
    }
}

/// 服务履约记录列表投影字段（交付对象快照、服务地点及指纹不进投影）。
///
/// # 返回
/// 返回投影条件文档。
fn service_fulfillment_projection() -> Document {
    doc! {
        "id": 1,
        "fulfillment_no": 1,
        "sales_order_line_id": 1,
        "purchase_order_id": 1,
        "quantity": 1,
        "result": 1,
        "status": 1,
        "occurred_at": 1,
        "recorded_at": 1,
        "version": 1,
    }
}

/// 客户验收单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_acceptance_projection() -> Document {
    doc! {
        "id": 1,
        "acceptance_no": 1,
        "sales_order_id": 1,
        "accepted_at": 1,
        "result": 1,
        "status": 1,
        "reversal_of_acceptance_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{ids_to_strings, sort_doc, Pagination, PurchaseReceiptFilter, QueryFilter};
    use mongodb::bson::doc;

    use entities::fulfillment::PurchaseReceiptState;
    use entities::ids::PurchaseOrderId;

    #[test]
    fn receipt_filter_applies_optional_fields_and_deleted_filter() {
        let filter = PurchaseReceiptFilter {
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            status: Some(PurchaseReceiptState::Posted),
            page: 2,
            page_size: 10,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("purchase_order_id").unwrap(), "po-1");
        assert_eq!(document.get_str("status").unwrap(), "POSTED");
        assert_eq!(filter.skip(), 10);
        assert_eq!(filter.limit(), 10);
    }

    #[test]
    fn sort_doc_maps_whitelisted_fields_and_defaults_otherwise() {
        let allowed = ["created_at", "posted_at"];
        assert_eq!(sort_doc(None, false, &allowed), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("posted_at"), true, &allowed),
            doc! { "posted_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false, &allowed),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序"
        );
    }

    #[test]
    fn ids_to_strings_converts_newtype_collection() {
        let ids = vec![PurchaseOrderId::new("po-1"), PurchaseOrderId::new("po-2")];
        assert_eq!(ids_to_strings(&ids), vec!["po-1".to_string(), "po-2".to_string()]);
        assert!(ids_to_strings::<PurchaseOrderId>(&[]).is_empty());
    }
}
