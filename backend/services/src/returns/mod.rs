//! 域 D21 `returns` 服务编排（页面：W05 销售单、W09 收货发货、W11 客户往来、
//! W12 供应商往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 客户退款、供应商退款、回款冲正与付款冲正创建必须在同一事务注册
//!   `BusinessDocument` 并绑定发布定义；
//! - 跨集合写入（处理单 + 明细行、退款/冲正过账）→
//!   `database::Transactional::with_transaction`；
//! - 单集合草稿写入 → `&mut NoTransaction`。
//! - 资金类入口（退款、冲正过账）以业务单号（退款单号/冲正单号）唯一索引 +
//!   状态迁移构成去重机制，重复提交只产生一条正式事实。
//!   客户退款、供应商退款、回款冲正与付款冲正过账只能作为审批最终通过动作。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：D18 回款/应收分录/核销分配，
//! D19 付款/应付分录/核销分配（退款、冲正事务内写入反向事实与反向核销，
//! §8.3-3）。

mod adapter;
mod cancel_approval;
mod customer_refund;
mod dto;
mod payment_reversal;
mod purchase_return;
mod receipt_reversal;
mod reversal_plan;
mod sales_return;
mod start_approval;
mod supplier_refund;

use mongodb::Database;

pub use self::adapter::{
    customer_refund_object_readable, payment_reversal_object_readable, receipt_reversal_object_readable,
    supplier_refund_object_readable,
};
pub use self::dto::{
    CancelCustomerRefundApprovalRequest, CancelPaymentReversalApprovalRequest,
    CancelReceiptReversalApprovalRequest, CancelSupplierRefundApprovalRequest, CreateCustomerRefundRequest,
    CreatePaymentReversalRequest, CreatePurchaseReturnOrderRequest, CreateReceiptReversalRequest,
    CreateSalesReturnCaseRequest, CreateSupplierRefundRequest, CustomerRefundListParams, CustomerRefundView,
    DocumentApprovalView, PageView, PaymentReversalView, PostCustomerRefundRequest,
    PostPaymentReversalRequest, PostReceiptReversalRequest, PostSupplierRefundRequest,
    PurchaseReturnOrderListParams, PurchaseReturnOrderView, ReceiptReversalView, SalesReturnCaseListParams,
    SalesReturnCaseView, SubmitCustomerRefundRequest, SubmitPaymentReversalRequest,
    SubmitReceiptReversalRequest, SubmitSupplierRefundRequest, SupplierRefundView,
};
use crate::iam::{self, SharedRbacService};

/// 退货退款服务。
///
/// 提供退货/拒收处理单、采购退货单、客户/供应商退款与回款/付款冲正编排。
pub struct ReturnsService {
    db: Database,
    rbac: SharedRbacService,
}

impl ReturnsService {
    /// 创建退货退款服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        let rbac = iam::shared_rbac_service(db.clone());
        Self { db, rbac }
    }
}
