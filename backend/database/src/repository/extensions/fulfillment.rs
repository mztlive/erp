//! 域 D16 `fulfillment` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS` 等值。

use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptance, CustomerAcceptanceLine, Delivery, DeliveryLine,
    ElectronicDelivery, PurchaseReceipt, PurchaseReceiptLine, ServiceFulfillment,
};
use mongodb::Database;

use super::super::fulfillment::{
    CustomerAcceptanceFilter, DeliveryFilter, ElectronicDeliveryFilter, FulfillmentRepository,
    PurchaseReceiptFilter, ServiceFulfillmentFilter,
};
use crate::Repository;

/// 域 D16 仓储访问器。
pub trait FulfillmentExt {
    /// `purchase_receipt` 集合名。
    const PURCHASE_RECEIPTS: &'static str = "purchase_receipts";
    /// `purchase_receipt_line` 集合名。
    const PURCHASE_RECEIPT_LINES: &'static str = "purchase_receipt_lines";
    /// `delivery` 集合名。
    const DELIVERIES: &'static str = "deliveries";
    /// `delivery_line` 集合名。
    const DELIVERY_LINES: &'static str = "delivery_lines";
    /// `electronic_delivery` 集合名。
    const ELECTRONIC_DELIVERIES: &'static str = "electronic_deliveries";
    /// `service_fulfillment` 集合名。
    const SERVICE_FULFILLMENTS: &'static str = "service_fulfillments";
    /// `customer_acceptance` 集合名。
    const CUSTOMER_ACCEPTANCES: &'static str = "customer_acceptances";
    /// `customer_acceptance_line` 集合名。
    const CUSTOMER_ACCEPTANCE_LINES: &'static str = "customer_acceptance_lines";
    /// `acceptance_fulfillment_allocation` 集合名。
    const ACCEPTANCE_FULFILLMENT_ALLOCATIONS: &'static str = "acceptance_fulfillment_allocations";

    /// 采购入库单列表筛选条件类型（定义见 `repository::fulfillment`）。
    type PurchaseReceiptFilter;

    /// 发货单列表筛选条件类型（定义见 `repository::fulfillment`）。
    type DeliveryFilter;

    /// 电子交付记录列表筛选条件类型（定义见 `repository::fulfillment`）。
    type ElectronicDeliveryFilter;

    /// 线下服务履约记录列表筛选条件类型（定义见 `repository::fulfillment`）。
    type ServiceFulfillmentFilter;

    /// 客户验收单列表筛选条件类型（定义见 `repository::fulfillment`）。
    type CustomerAcceptanceFilter;

    /// 获取 `purchase_receipt` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::PurchaseReceipt>`。
    fn purchase_receipts(&self) -> Repository<'_, PurchaseReceipt>;

    /// 获取 `purchase_receipt_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::PurchaseReceiptLine>`。
    fn purchase_receipt_lines(&self) -> Repository<'_, PurchaseReceiptLine>;

    /// 获取 `delivery` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::Delivery>`。
    fn deliveries(&self) -> Repository<'_, Delivery>;

    /// 获取 `delivery_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::DeliveryLine>`。
    fn delivery_lines(&self) -> Repository<'_, DeliveryLine>;

    /// 获取 `electronic_delivery` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::ElectronicDelivery>`。
    fn electronic_deliveries(&self) -> Repository<'_, ElectronicDelivery>;

    /// 获取 `service_fulfillment` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::ServiceFulfillment>`。
    fn service_fulfillments(&self) -> Repository<'_, ServiceFulfillment>;

    /// 获取 `customer_acceptance` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::CustomerAcceptance>`。
    fn customer_acceptances(&self) -> Repository<'_, CustomerAcceptance>;

    /// 获取 `customer_acceptance_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::CustomerAcceptanceLine>`。
    fn customer_acceptance_lines(&self) -> Repository<'_, CustomerAcceptanceLine>;

    /// 获取 `acceptance_fulfillment_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::fulfillment::AcceptanceFulfillmentAllocation>`。
    fn acceptance_fulfillment_allocations(&self) -> Repository<'_, AcceptanceFulfillmentAllocation>;

    /// 获取承载跨集合写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `FulfillmentRepository` 实例。
    fn fulfillment(&self) -> FulfillmentRepository<'_>;
}

impl FulfillmentExt for Database {
    type PurchaseReceiptFilter = PurchaseReceiptFilter;
    type DeliveryFilter = DeliveryFilter;
    type ElectronicDeliveryFilter = ElectronicDeliveryFilter;
    type ServiceFulfillmentFilter = ServiceFulfillmentFilter;
    type CustomerAcceptanceFilter = CustomerAcceptanceFilter;

    fn purchase_receipts(&self) -> Repository<'_, PurchaseReceipt> {
        Repository::new(self, Self::PURCHASE_RECEIPTS)
    }

    fn purchase_receipt_lines(&self) -> Repository<'_, PurchaseReceiptLine> {
        Repository::new(self, Self::PURCHASE_RECEIPT_LINES)
    }

    fn deliveries(&self) -> Repository<'_, Delivery> {
        Repository::new(self, Self::DELIVERIES)
    }

    fn delivery_lines(&self) -> Repository<'_, DeliveryLine> {
        Repository::new(self, Self::DELIVERY_LINES)
    }

    fn electronic_deliveries(&self) -> Repository<'_, ElectronicDelivery> {
        Repository::new(self, Self::ELECTRONIC_DELIVERIES)
    }

    fn service_fulfillments(&self) -> Repository<'_, ServiceFulfillment> {
        Repository::new(self, Self::SERVICE_FULFILLMENTS)
    }

    fn customer_acceptances(&self) -> Repository<'_, CustomerAcceptance> {
        Repository::new(self, Self::CUSTOMER_ACCEPTANCES)
    }

    fn customer_acceptance_lines(&self) -> Repository<'_, CustomerAcceptanceLine> {
        Repository::new(self, Self::CUSTOMER_ACCEPTANCE_LINES)
    }

    fn acceptance_fulfillment_allocations(&self) -> Repository<'_, AcceptanceFulfillmentAllocation> {
        Repository::new(self, Self::ACCEPTANCE_FULFILLMENT_ALLOCATIONS)
    }

    fn fulfillment(&self) -> FulfillmentRepository<'_> {
        FulfillmentRepository::new(self)
    }
}
