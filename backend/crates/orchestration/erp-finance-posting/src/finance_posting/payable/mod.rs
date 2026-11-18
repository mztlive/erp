//! 供应商往来 Process：协调付款、付款申请与进项发票的跨域写入。
//!
//! 命令入口负责事务边界；事务内步骤沿用调用方 Executor，原子保存财务过账、
//! 单据注册、任务状态及审计。供应商付款在付款执行事务中注册无审批的
//! `BusinessDocument`；付款单号、规范化发票号码及状态迁移保留各入口的去重合同。
//!
//! 财务实体、分配与资金规则归 `erp-finance`；采购来源、供应商主体与结算事实
//! 分别由 `erp-procurement`、`erp-supplier`、`erp-supply` 的公开读取合同提供。
//! 展示读取复用 `erp-read-models`，文件上传与其他外部 I/O 在数据库事务外执行。

use erp_finance::dto::payable::{self as dto, SupplierPaymentView};
use erp_read_models::finance::payable::PayableReadService;
use erp_support::FileAssetView;
use mongodb::Database;

use crate::{FinancePostingPorts, Result};

mod account;
mod invoice;
pub use invoice::PurchaseInvoiceWithAssetsResult;
mod payment;
pub mod payment_request;
mod posting;

pub use payment_request::PurchasePaymentRequestProcess;

/// 供应商往来跨域写入流程。
///
/// 提供应付台账、付款单与进项发票登记编排。
pub struct PayableProcess {
    db: Database,
    /// 付款任务和单据绑定读取复用的授权实例。
    authorization_rbac: FinancePostingPorts,
}

/// 已确认付款结果的详情读取与本次银行回单提交状态。
///
/// 调用方必须先按附件提交状态完成补偿，再展开详情读取结果；
/// 详情读取失败不得改变已提交或已恢复的付款事实。
pub struct SupplierPaymentWithAssetsResult {
    /// 提交或回执查证完成后的详情读取结果；失败保留原错误分类。
    pub view: Result<SupplierPaymentView>,
    /// 本次上传对象是否已随业务事务登记；幂等重放时为 `false`。
    pub assets_committed: bool,
}

/// 受控银行回单预览的文件元数据和私有完整读取资格快照。
pub struct SupplierPaymentBankReceiptSnapshot {
    /// 已完成对象资格校验与预览审计的文件元数据。
    pub asset: FileAssetView,
    /// 当前付款及全部来源版本、授权状态形成的资格指纹。
    qualification: (u64, String),
}

impl PayableProcess {
    /// 创建供应商往来流程实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回流程实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: FinancePostingPorts) -> Self {
        Self { db, authorization_rbac: rbac }
    }

    /// 取得应付跨领域读模型，命令回读复用相同装配实现。
    fn read(&self) -> PayableReadService {
        PayableReadService::new(self.db.clone())
    }
}
#[cfg(test)]
mod supplier_payment_execution_tests {
    use erp_read_models::finance::payable::mapping::masked_bank_account_number;

    /// 收款账号掩码不得泄露除末四位外的内容。
    #[test]
    fn recipient_mask_only_contains_last_four() {
        assert_eq!(masked_bank_account_number("1234"), "********1234");
        assert_eq!(masked_bank_account_number(""), "********");
    }
}
