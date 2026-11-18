//! 域 D16 `fulfillment` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS` 等值。

use mongodb::Database;

use super::super::fulfillment::{
    CustomerAcceptanceFilter, DeliveryFilter, ElectronicDeliveryFilter, FulfillmentRepository,
    PurchaseReceiptFilter, ServiceFulfillmentFilter,
};
use crate::entity::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptanceLine, DeliveryLine, PurchaseReceiptLine,
};
use crate::repository::owned::{
    CustomerAcceptanceRepository, DeliveryRepository, ElectronicDeliveryRepository,
    PurchaseReceiptRepository, ServiceFulfillmentRepository,
};

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

    /// 取得 `purchase_receipt` 集合的仓储，供采购收货表头的读写；由调用方提供执行器。
    fn purchase_receipts(&self) -> PurchaseReceiptRepository<'_>;

    /// 取得 `purchase_receipt_line` 集合的仓储，供采购收货明细的读写；由调用方提供执行器。
    fn purchase_receipt_lines(&self) -> persistence_core::Repository<'_, PurchaseReceiptLine>;

    /// 取得 `delivery` 集合的仓储，供实物发货表头的读写；由调用方提供执行器。
    fn deliveries(&self) -> DeliveryRepository<'_>;

    /// 取得 `delivery_line` 集合的仓储，供实物发货明细的读写；由调用方提供执行器。
    fn delivery_lines(&self) -> persistence_core::Repository<'_, DeliveryLine>;

    /// 取得 `electronic_delivery` 集合的仓储，供电子交付记录的读写；由调用方提供执行器。
    fn electronic_deliveries(&self) -> ElectronicDeliveryRepository<'_>;

    /// 取得 `service_fulfillment` 集合的仓储，供服务履约记录的读写；由调用方提供执行器。
    fn service_fulfillments(&self) -> ServiceFulfillmentRepository<'_>;

    /// 取得 `customer_acceptance` 集合的仓储，供客户验收表头的读写；由调用方提供执行器。
    fn customer_acceptances(&self) -> CustomerAcceptanceRepository<'_>;

    /// 取得 `customer_acceptance_line` 集合的仓储，供客户验收明细的读写；由调用方提供执行器。
    fn customer_acceptance_lines(&self) -> persistence_core::Repository<'_, CustomerAcceptanceLine>;

    /// 取得 `acceptance_fulfillment_allocation` 集合的仓储，供验收分配与反向分配的读写；由调用方提供执行器。
    fn acceptance_fulfillment_allocations(
        &self,
    ) -> persistence_core::Repository<'_, AcceptanceFulfillmentAllocation>;

    /// 取得履约表头、明细和验收分配的跨集合操作入口；原子写入沿用用例事务，不在仓储内开启事务。
    fn fulfillment(&self) -> FulfillmentRepository<'_>;
}

impl FulfillmentExt for Database {
    type PurchaseReceiptFilter = PurchaseReceiptFilter;
    type DeliveryFilter = DeliveryFilter;
    type ElectronicDeliveryFilter = ElectronicDeliveryFilter;
    type ServiceFulfillmentFilter = ServiceFulfillmentFilter;
    type CustomerAcceptanceFilter = CustomerAcceptanceFilter;

    fn purchase_receipts(&self) -> PurchaseReceiptRepository<'_> {
        PurchaseReceiptRepository::new(self, Self::PURCHASE_RECEIPTS)
    }

    fn purchase_receipt_lines(&self) -> persistence_core::Repository<'_, PurchaseReceiptLine> {
        persistence_core::Repository::new(self, Self::PURCHASE_RECEIPT_LINES)
    }

    fn deliveries(&self) -> DeliveryRepository<'_> {
        DeliveryRepository::new(self, Self::DELIVERIES)
    }

    fn delivery_lines(&self) -> persistence_core::Repository<'_, DeliveryLine> {
        persistence_core::Repository::new(self, Self::DELIVERY_LINES)
    }

    fn electronic_deliveries(&self) -> ElectronicDeliveryRepository<'_> {
        ElectronicDeliveryRepository::new(self, Self::ELECTRONIC_DELIVERIES)
    }

    fn service_fulfillments(&self) -> ServiceFulfillmentRepository<'_> {
        ServiceFulfillmentRepository::new(self, Self::SERVICE_FULFILLMENTS)
    }

    fn customer_acceptances(&self) -> CustomerAcceptanceRepository<'_> {
        CustomerAcceptanceRepository::new(self, Self::CUSTOMER_ACCEPTANCES)
    }

    fn customer_acceptance_lines(&self) -> persistence_core::Repository<'_, CustomerAcceptanceLine> {
        persistence_core::Repository::new(self, Self::CUSTOMER_ACCEPTANCE_LINES)
    }

    fn acceptance_fulfillment_allocations(
        &self,
    ) -> persistence_core::Repository<'_, AcceptanceFulfillmentAllocation> {
        persistence_core::Repository::new(self, Self::ACCEPTANCE_FULFILLMENT_ALLOCATIONS)
    }

    fn fulfillment(&self) -> FulfillmentRepository<'_> {
        FulfillmentRepository::new(self)
    }
}
