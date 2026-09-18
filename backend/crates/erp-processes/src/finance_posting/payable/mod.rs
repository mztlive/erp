//! 域 D19 `payable` 服务编排（页面：W12 供应商往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 供应商付款必须在付款执行事务注册无审批 `BusinessDocument`；
//! - 跨集合资金/票款过账（§8.3 不变量）→
//!   `persistence_core::Transactional::with_transaction`。
//! - 资金类入口（付款过账、进项发票登记）以业务唯一键
//!   （付款单号/规范化发票号码）与状态迁移构成去重机制。
//!   采购审批形成付款授权，付款任务由当前责任出纳直接登记并过账。
//!
//! 跨域只经各领域 `*Ext` 扩展 trait 调对方域 Repository：
//! - D15 `purchase_orders()` 校验来源采购单存在，并解析采购单号供展示；
//! - D09 `supplier_accounts()` 校验供应商存在并取 `party_id`（进项发票
//!   与应付子账的往来主体相等键）；
//! - D18 `invoices()` 复用发票仓储（`invoice` 由 D18 拥有实体与仓储，
//!   D19 只拥有 `purchase_invoice_allocation`，禁止复制发票实体）；
//! - D33 `supplier_settlement_statements()` 解析结算单号供展示。

use erp_identity::SharedRbacService;
use mongodb::Database;

use crate::adapters::identity::shared_rbac_service;

mod account;

use erp_finance::dto::payable as dto;
mod invoice;
use erp_read_models::finance::payable::mapping;
mod payment;
mod payment_merge;
pub mod payment_task;
mod posting;

use erp_finance::dto::payable::SupplierPaymentView;

/// 供应商往来服务。
///
/// 提供应付台账、付款单与进项发票登记编排。
pub struct PayableService {
    db: Database,
    rbac: SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}

/// 携带银行回单文件资产的付款提交结果。
pub struct SupplierPaymentWithAssetsResult {
    /// 稳定付款单结果。
    pub view: SupplierPaymentView,
    /// 本次上传对象是否已随业务事务登记；幂等重放时为 `false`。
    pub assets_committed: bool,
}

impl PayableService {
    /// 创建供应商往来服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        let rbac = shared_rbac_service(db.clone());
        Self { db, rbac, object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort) }
    }

    /// Inject composition-root object-read for approval binding.
    pub fn with_object_read(
        mut self,
        object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        self.object_read = object_read;
        self
    }
}

impl PayableService {
    /// 取得应付跨领域读模型，命令回读复用相同装配实现。
    fn read(&self) -> erp_read_models::finance::payable::PayableReadService {
        erp_read_models::finance::payable::PayableReadService::new(self.db.clone())
    }
}
#[cfg(test)]
mod supplier_payment_execution_tests {
    use super::mapping::masked_bank_account_number;

    /// 收款账号掩码不得泄露除末四位外的内容。
    #[test]
    fn recipient_mask_only_contains_last_four() {
        assert_eq!(masked_bank_account_number("1234"), "********1234");
        assert_eq!(masked_bank_account_number(""), "********");
    }
}
