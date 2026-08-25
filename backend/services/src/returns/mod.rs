//! 域 D21 `returns` 服务编排（页面：W05 销售单、W09 收货发货、W11 客户往来、
//! W12 供应商往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 客户退款、供应商退款、回款冲正与付款冲正创建必须在同一事务注册
//!   `BusinessDocument` 并绑定发布定义；
//! - 跨集合写入（处理单 + 明细行、退款/冲正过账）→
//!   `database::Transactional::with_transaction`；
//! - 单集合草稿写入 → `&mut NoTransaction`。
//! - 资金类根入口以操作人 + 操作号的事务内命令收据回放首次结果，业务单号
//!   唯一索引只承担最终唯一性兜底。
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
use sha2::{Digest, Sha256};

pub use self::adapter::{
    customer_refund_object_readable, payment_reversal_object_readable, receipt_reversal_object_readable,
    supplier_refund_object_readable,
};
pub use self::dto::{
    CancelCustomerRefundApprovalRequest, CancelPaymentReversalApprovalRequest,
    CancelReceiptReversalApprovalRequest, CancelSupplierRefundApprovalRequest, CommitCustomerRefundRequest,
    CommitPaymentReversalRequest, CommitReceiptReversalRequest, CommitSupplierRefundRequest,
    CreateCustomerRefundRequest, CreatePaymentReversalRequest, CreatePurchaseReturnOrderRequest,
    CreateReceiptReversalRequest, CreateSalesReturnCaseRequest, CreateSupplierRefundRequest,
    CustomerRefundListParams, CustomerRefundView, DocumentApprovalView, PageView, PaymentReversalView,
    PostCustomerRefundRequest, PostPaymentReversalRequest, PostReceiptReversalRequest,
    PostSupplierRefundRequest, PurchaseReturnOrderListParams, PurchaseReturnOrderView, ReceiptReversalView,
    SalesReturnCaseListParams, SalesReturnCaseView, SubmitCustomerRefundRequest,
    SubmitPaymentReversalRequest, SubmitReceiptReversalRequest, SubmitSupplierRefundRequest,
    SupplierRefundView,
};
use crate::{
    errors::{Error, Result},
    iam::{self, SharedRbacService},
};

/// 退货退款服务。
///
/// 提供退货/拒收处理单、采购退货单、客户/供应商退款与回款/付款冲正编排。
pub struct ReturnsService {
    db: Database,
    rbac: SharedRbacService,
}

/// 由操作者与幂等键生成不泄露原键的稳定纠错单号。
pub(super) fn return_command_no(prefix: &str, actor_id: &str, idempotency_key: &str) -> String {
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{actor_id}|{}", idempotency_key.trim()).as_bytes())
    );
    format!("{prefix}-{}", &digest[..8])
}

/// 校验纠错命令的原资金事实仍为同一版本且已经过账。
///
/// # 参数
/// * `actual_version` - 事务内重读所得版本
/// * `expected_version` - 命令准备阶段读取的版本
/// * `is_posted` - 原事实是否处于已过账状态
/// * `not_posted_message` - 当前纠错类型对应的业务说明
///
/// # 返回
/// 版本和状态均满足时返回成功。
///
/// # 错误
/// 版本变化返回冲突；原事实未过账返回业务规则错误。
pub(super) fn ensure_posted_source(
    actual_version: u64,
    expected_version: u64,
    is_posted: bool,
    not_posted_message: &str,
) -> Result<()> {
    if actual_version != expected_version {
        return Err(Error::ConflictError("原资金记录已变化，请刷新后重试".to_string()));
    }
    if !is_posted {
        return Err(Error::BusinessLogicError(not_posted_message.to_string()));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::ensure_posted_source;
    use crate::errors::Error;

    #[test]
    fn correction_source_requires_same_posted_fact() {
        assert!(ensure_posted_source(3, 3, true, "必须已过账").is_ok());
        assert!(matches!(
            ensure_posted_source(4, 3, true, "必须已过账"),
            Err(Error::ConflictError(_))
        ));
        assert!(matches!(
            ensure_posted_source(3, 3, false, "必须已过账"),
            Err(Error::BusinessLogicError(_))
        ));
    }
}
