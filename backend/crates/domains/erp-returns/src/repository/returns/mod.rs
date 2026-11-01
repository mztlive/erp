//! 域 D21 `returns` 仓储：sales_return_case、sales_return_line、
//! purchase_return_order、purchase_return_line、customer_refund、supplier_refund、
//! receipt_reversal、payment_reversal。
//!
//! 单一集合 CRUD 直接复用 [`persistence_core::Repository`] 基类；本模块只补充域特有查询与
//! 跨集合多步骤事务写入入口。集合名常量统一从 `ReturnsExt` 关联常量获取。
//!
//! 本域全部集合是退货/退款/冲正事实与处理单（§4.5），**不提供软删除方法**
//! （纠错用反向事实表达）。筛选/行类型定义在本模块，经 `ReturnsExt` 的关联
//! 类型对外暴露；调用方也可经公开的 `repository::returns` 路径访问。

mod cross_collection;
mod customer_refund;
mod purchase_return;
mod reversals;
mod sales_return;
mod search;
mod supplier_refund;

pub use cross_collection::ReturnsRepository;
pub use customer_refund::{CustomerRefundFilter, CustomerRefundRepositoryExt, CustomerRefundRow};
pub use purchase_return::{
    PurchaseReturnLineRepositoryExt, PurchaseReturnOrderFilter, PurchaseReturnOrderRepositoryExt,
    PurchaseReturnOrderRow, PurchaseReturnVersion,
};
pub use reversals::{PaymentReversalRepositoryExt, ReceiptReversalRepositoryExt};
pub use sales_return::{
    SalesReturnCaseFilter, SalesReturnCaseRepositoryExt, SalesReturnCaseRow, SalesReturnLineRepositoryExt,
};
pub use supplier_refund::SupplierRefundRepositoryExt;
