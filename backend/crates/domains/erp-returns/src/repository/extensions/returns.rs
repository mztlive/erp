//! 域 D21 `returns` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as ReturnsExt>::SALES_RETURN_CASES` 等值。

use mongodb::Database;

use super::super::returns::{
    CustomerRefundFilter, PurchaseReturnOrderFilter, ReturnsRepository, SalesReturnCaseFilter,
};
use crate::repository::owned::{
    CustomerRefundRepository, PaymentReversalRepository, PurchaseReturnLineRepository,
    PurchaseReturnOrderRepository, ReceiptReversalRepository, SalesReturnCaseRepository,
    SalesReturnLineRepository, SupplierRefundRepository,
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

    /// 取得 `sales_return_case` 集合的仓储，供销售退货处理单表头的读写；由调用方提供执行器。
    fn sales_return_cases(&self) -> SalesReturnCaseRepository<'_>;

    /// 取得 `sales_return_line` 集合的仓储，供销售退货明细的读写；由调用方提供执行器。
    fn sales_return_lines(&self) -> SalesReturnLineRepository<'_>;

    /// 取得 `purchase_return_order` 集合的仓储，供采购退货表头的读写；由调用方提供执行器。
    fn purchase_return_orders(&self) -> PurchaseReturnOrderRepository<'_>;

    /// 取得 `purchase_return_line` 集合的仓储，供采购退货明细的读写；由调用方提供执行器。
    fn purchase_return_lines(&self) -> PurchaseReturnLineRepository<'_>;

    /// 取得 `customer_refund` 集合的仓储，供客户退款单据的读写；由调用方提供执行器。
    fn customer_refunds(&self) -> CustomerRefundRepository<'_>;

    /// 取得 `supplier_refund` 集合的仓储，供供应商退款单据的读写；由调用方提供执行器。
    fn supplier_refunds(&self) -> SupplierRefundRepository<'_>;

    /// 取得 `receipt_reversal` 集合的仓储，供回款冲正单据的读写；由调用方提供执行器。
    fn receipt_reversals(&self) -> ReceiptReversalRepository<'_>;

    /// 取得 `payment_reversal` 集合的仓储，供付款冲正单据的读写；由调用方提供执行器。
    fn payment_reversals(&self) -> PaymentReversalRepository<'_>;

    /// 取得退货表头与明细等跨集合操作入口；原子写入沿用用例事务，不在仓储内开启事务。
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
