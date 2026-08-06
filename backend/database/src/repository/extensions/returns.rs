//! 域 D21 `returns` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as ReturnsExt>::SALES_RETURN_CASES` 等值。

use entities::returns::{
    CustomerRefund, PaymentReversal, PurchaseReturnLine, PurchaseReturnOrder, ReceiptReversal,
    SalesReturnCase, SalesReturnLine, SupplierRefund,
};
use mongodb::Database;

use super::super::returns::{
    CustomerRefundFilter, PurchaseReturnOrderFilter, ReturnsRepository, SalesReturnCaseFilter,
};
use crate::Repository;

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
    /// 返回 `Repository<'_, entities::returns::SalesReturnCase>`。
    fn sales_return_cases(&self) -> Repository<'_, SalesReturnCase>;

    /// 获取 `sales_return_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::returns::SalesReturnLine>`。
    fn sales_return_lines(&self) -> Repository<'_, SalesReturnLine>;

    /// 获取 `purchase_return_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::returns::PurchaseReturnOrder>`。
    fn purchase_return_orders(&self) -> Repository<'_, PurchaseReturnOrder>;

    /// 获取 `purchase_return_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::returns::PurchaseReturnLine>`。
    fn purchase_return_lines(&self) -> Repository<'_, PurchaseReturnLine>;

    /// 获取 `customer_refund` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::returns::CustomerRefund>`。
    fn customer_refunds(&self) -> Repository<'_, CustomerRefund>;

    /// 获取 `supplier_refund` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::returns::SupplierRefund>`。
    fn supplier_refunds(&self) -> Repository<'_, SupplierRefund>;

    /// 获取 `receipt_reversal` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::returns::ReceiptReversal>`。
    fn receipt_reversals(&self) -> Repository<'_, ReceiptReversal>;

    /// 获取 `payment_reversal` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::returns::PaymentReversal>`。
    fn payment_reversals(&self) -> Repository<'_, PaymentReversal>;

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

    fn sales_return_cases(&self) -> Repository<'_, SalesReturnCase> {
        Repository::new(self, Self::SALES_RETURN_CASES)
    }

    fn sales_return_lines(&self) -> Repository<'_, SalesReturnLine> {
        Repository::new(self, Self::SALES_RETURN_LINES)
    }

    fn purchase_return_orders(&self) -> Repository<'_, PurchaseReturnOrder> {
        Repository::new(self, Self::PURCHASE_RETURN_ORDERS)
    }

    fn purchase_return_lines(&self) -> Repository<'_, PurchaseReturnLine> {
        Repository::new(self, Self::PURCHASE_RETURN_LINES)
    }

    fn customer_refunds(&self) -> Repository<'_, CustomerRefund> {
        Repository::new(self, Self::CUSTOMER_REFUNDS)
    }

    fn supplier_refunds(&self) -> Repository<'_, SupplierRefund> {
        Repository::new(self, Self::SUPPLIER_REFUNDS)
    }

    fn receipt_reversals(&self) -> Repository<'_, ReceiptReversal> {
        Repository::new(self, Self::RECEIPT_REVERSALS)
    }

    fn payment_reversals(&self) -> Repository<'_, PaymentReversal> {
        Repository::new(self, Self::PAYMENT_REVERSALS)
    }

    fn returns(&self) -> ReturnsRepository<'_> {
        ReturnsRepository::new(self)
    }
}
