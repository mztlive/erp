//! 域 D21 `returns` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as ReturnsExt>::SALES_RETURN_CASES` 等值。

use crate::repository::owned::{
    CustomerRefundRepository, PaymentReversalRepository, PurchaseReturnLineRepository,
    PurchaseReturnOrderRepository, ReceiptReversalRepository, SalesReturnCaseRepository,
    SalesReturnLineRepository, SupplierRefundRepository,
};
use mongodb::Database;

use super::super::returns::{
    CustomerRefundFilter, PurchaseReturnOrderFilter, ReturnsRepository, SalesReturnCaseFilter,
};

/// 域 D21 仓储访问器。
pub trait ReturnsExt {
    /// `sales_return_case` 集合名。
    const SALES_RETURN_CASES: &'static str = "sales_return_cases";
    /// `sales_return_line` 集合名。
    const SALES_RETURN_LINES: &'static str = "sales_return_lines";
    /// `purchase_return_order` 集合名。
    const PURCHASE_RETURN_ORDERS: &'static str = "purchase_return_orders";
    /// `purchase_return_line` 集合名。
    const PURCHASE_RETURN_LINES: &'static str = "purchase_return_lines";
    /// `customer_refund` 集合名。
    const CUSTOMER_REFUNDS: &'static str = "customer_refunds";
    /// `supplier_refund` 集合名。
    const SUPPLIER_REFUNDS: &'static str = "supplier_refunds";
    /// `receipt_reversal` 集合名。
    const RECEIPT_REVERSALS: &'static str = "receipt_reversals";
    /// `payment_reversal` 集合名。
    const PAYMENT_REVERSALS: &'static str = "payment_reversals";

    /// 销售退货处理单列表筛选条件类型（定义见 `repository::returns`）。
    type SalesReturnCaseFilter;

    /// 采购退货单列表筛选条件类型（定义见 `repository::returns`）。
    type PurchaseReturnOrderFilter;

    /// 客户退款列表筛选条件类型（定义见 `repository::returns`）。
    type CustomerRefundFilter;

    /// 获取 `sales_return_case` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `SalesReturnCaseRepository<'_>`。
    fn sales_return_cases(&self) -> SalesReturnCaseRepository<'_>;

    /// 获取 `sales_return_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `SalesReturnLineRepository<'_>`。
    fn sales_return_lines(&self) -> SalesReturnLineRepository<'_>;

    /// 获取 `purchase_return_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseReturnOrderRepository<'_>`。
    fn purchase_return_orders(&self) -> PurchaseReturnOrderRepository<'_>;

    /// 获取 `purchase_return_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseReturnLineRepository<'_>`。
    fn purchase_return_lines(&self) -> PurchaseReturnLineRepository<'_>;

    /// 获取 `customer_refund` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `CustomerRefundRepository<'_>`。
    fn customer_refunds(&self) -> CustomerRefundRepository<'_>;

    /// 获取 `supplier_refund` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `SupplierRefundRepository<'_>`。
    fn supplier_refunds(&self) -> SupplierRefundRepository<'_>;

    /// 获取 `receipt_reversal` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `ReceiptReversalRepository<'_>`。
    fn receipt_reversals(&self) -> ReceiptReversalRepository<'_>;

    /// 获取 `payment_reversal` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PaymentReversalRepository<'_>`。
    fn payment_reversals(&self) -> PaymentReversalRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `ReturnsRepository` 实例。
    fn returns(&self) -> ReturnsRepository<'_>;
}

impl ReturnsExt for Database {
    type SalesReturnCaseFilter = SalesReturnCaseFilter;
    type PurchaseReturnOrderFilter = PurchaseReturnOrderFilter;
    type CustomerRefundFilter = CustomerRefundFilter;

    fn sales_return_cases(&self) -> SalesReturnCaseRepository<'_> {
        SalesReturnCaseRepository::new(self, Self::SALES_RETURN_CASES)
    }

    fn sales_return_lines(&self) -> SalesReturnLineRepository<'_> {
        SalesReturnLineRepository::new(self, Self::SALES_RETURN_LINES)
    }

    fn purchase_return_orders(&self) -> PurchaseReturnOrderRepository<'_> {
        PurchaseReturnOrderRepository::new(self, Self::PURCHASE_RETURN_ORDERS)
    }

    fn purchase_return_lines(&self) -> PurchaseReturnLineRepository<'_> {
        PurchaseReturnLineRepository::new(self, Self::PURCHASE_RETURN_LINES)
    }

    fn customer_refunds(&self) -> CustomerRefundRepository<'_> {
        CustomerRefundRepository::new(self, Self::CUSTOMER_REFUNDS)
    }

    fn supplier_refunds(&self) -> SupplierRefundRepository<'_> {
        SupplierRefundRepository::new(self, Self::SUPPLIER_REFUNDS)
    }

    fn receipt_reversals(&self) -> ReceiptReversalRepository<'_> {
        ReceiptReversalRepository::new(self, Self::RECEIPT_REVERSALS)
    }

    fn payment_reversals(&self) -> PaymentReversalRepository<'_> {
        PaymentReversalRepository::new(self, Self::PAYMENT_REVERSALS)
    }

    fn returns(&self) -> ReturnsRepository<'_> {
        ReturnsRepository::new(self)
    }
}
