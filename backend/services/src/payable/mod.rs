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
//! 跨域只经 `DatabaseExt` 调对方域 Repository：
//! - D15 `purchase_orders()` 校验来源采购单存在，并解析采购单号供展示；
//! - D09 `supplier_accounts()` 校验供应商存在并取 `party_id`（进项发票
//!   与应付子账的往来主体相等键）；
//! - D18 `invoices()` 复用发票仓储（`invoice` 由 D18 拥有实体与仓储，
//!   D19 只拥有 `purchase_invoice_allocation`，禁止复制发票实体）；
//! - D33 `supplier_settlement_statements()` 解析结算单号供展示。

use database::PayableExt;
use mongodb::Database;

use crate::identity_compose::shared_rbac_service;
use erp_identity::SharedRbacService;

mod account;
mod display;
mod dto;
mod invoice;
mod mapping;
mod payment;
pub(crate) mod payment_task;

pub use self::dto::{
    CommitSupplierPaymentRequest, CreatePayableAccountRequest, CreateSupplierPaymentRequest, PageView,
    PayableAccountListParams, PayableAccountSummaryView, PayableAccountView, PaymentAllocationLineRequest,
    PaymentAllocationView, PaymentRecipientRevealView, PaymentRecipientView,
    PurchaseInvoiceAllocationLineRequest, PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView,
    PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest, RevealPaymentRecipientRequest,
    SupplierPaymentBankReceiptView, SupplierPaymentListParams, SupplierPaymentReversalView,
    SupplierPaymentView,
};

/// 应付往来子账列表筛选条件类型（经 `PayableExt` 关联类型跨 crate 可达）。
type PayableAccountFilter = <mongodb::Database as PayableExt>::PayableAccountFilter;
/// 供应商付款单列表筛选条件类型。
type SupplierPaymentFilter = <mongodb::Database as PayableExt>::SupplierPaymentFilter;
/// 进项发票分配服务端分页筛选条件类型（FIN-R06）。
type PurchaseInvoiceAllocationFilter = <mongodb::Database as PayableExt>::PurchaseInvoiceAllocationFilter;

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
        Self {
            db,
            rbac,
            object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort),
        }
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

#[cfg(test)]
mod purchase_invoice_allocation_list_tests {
    use std::str::FromStr;

    use entities::payable::{AllocationAction, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData};
    use erp_core::ids::{InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId};
    use erp_core::money::Amount;

    use super::invoice::purchase_invoice_allocation_view;

    /// 构造具有指定稳定 ID 与秒级创建时间的最小进项发票分配事实。
    ///
    /// 参数提供排序键，返回通过实体校验的正式分配；测试金额固定为 `1.00`，
    /// 构造失败时直接 panic，且不访问数据库。
    fn allocation(id: &str, created_at: u64) -> PurchaseInvoiceAllocation {
        let mut allocation = PurchaseInvoiceAllocation::new(
            PurchaseInvoiceAllocationId::new(id),
            PurchaseInvoiceAllocationData {
                invoice_id: InvoiceId::new("invoice-1"),
                payable_account_id: PayableAccountId::new("account-1"),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_gross_amount: Amount::from_str("1.00").unwrap(),
                allocated_net_amount: Amount::from_str("1.00").unwrap(),
                allocated_tax_amount: Amount::from_str("0.00").unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        allocation.base.created_at = created_at;
        allocation
    }

    /// 按 `(created_at, id)` 同方向排序；升序与降序均使用同一方向并列键。
    fn stable_order(ids: &[(&str, u64)], ascending: bool) -> Vec<String> {
        let mut rows: Vec<(&str, u64)> = ids.to_vec();
        rows.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(right.0)));
        if !ascending {
            rows.reverse();
        }
        rows.into_iter().map(|(id, _)| id.to_string()).collect()
    }

    /// FIN-R06：列表只装载当前页，过滤、稳定排序与总数由 Repository 服务端完成。
    #[test]
    fn allocation_list_uses_server_pagination() {
        let production = include_str!("invoice.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let body = production
            .split("pub async fn purchase_invoice_allocation_list")
            .nth(1)
            .expect("分配列表")
            .split("/// 装配进项发票分配视图")
            .next()
            .expect("列表函数体");
        assert!(body.contains("search_purchase_invoice_allocations"));
        assert!(body.contains("PurchaseInvoiceAllocationFilter"));
        assert!(!body.contains("find_allocations_by_accounts"));
        assert!(!production.contains("fn purchase_invoice_allocation_page"));
    }

    /// 升序按创建时间再按稳定 ID 返回。
    #[test]
    fn allocation_stable_order_sorts_ascending() {
        assert_eq!(
            stable_order(&[("a-3", 30), ("a-1", 10), ("a-2", 20)], true),
            ["a-1", "a-2", "a-3"]
        );
    }

    /// 降序对创建时间与稳定 ID 使用同一方向。
    #[test]
    fn allocation_stable_order_sorts_descending() {
        assert_eq!(
            stable_order(&[("a-1", 10), ("a-3", 30), ("a-2", 20)], false),
            ["a-3", "a-2", "a-1"]
        );
    }

    /// 同秒事实跨页边界保持确定，无重复、无遗漏。
    #[test]
    fn allocation_stable_order_paginates_equal_timestamps_deterministically() {
        let rows = [("a-2", 10), ("a-4", 10), ("a-1", 10), ("a-3", 10)];
        let ascending = stable_order(&rows, true);
        let descending = stable_order(&rows, false);
        assert_eq!(ascending, ["a-1", "a-2", "a-3", "a-4"]);
        assert_eq!(descending, ["a-4", "a-3", "a-2", "a-1"]);
        assert_eq!(&ascending[2..], ["a-3", "a-4"]);
        assert_eq!(&descending[2..], ["a-2", "a-1"]);
    }

    /// 视图映射保留分配身份，不改变金额精度。
    #[test]
    fn allocation_view_mapping_preserves_identity() {
        let view = purchase_invoice_allocation_view(&allocation("a-1", 10));
        assert_eq!(view.id, "a-1");
        assert_eq!(view.invoice_id, "invoice-1");
        assert_eq!(view.payable_account_id, "account-1");
    }
}

#[cfg(test)]
mod supplier_payment_execution_tests {
    use super::mapping::masked_bank_account_number;

    /// 银行回单可用性规则必须由 `BankReceiptEvidencePolicy` 单一入口承担：
    /// pending 待登记与 stored 已落库两条路径都调用该 VO，旧 Service 校验
    /// helper 已删除。
    #[test]
    fn bank_receipt_evidence_uses_single_policy_entry() {
        let production = include_str!("payment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let pending_path = production
            .split("BankReceiptEvidencePolicy::validate(")
            .nth(1)
            .expect("待登记校验入口");
        assert!(pending_path.contains("request.registration.content_type"));
        assert!(pending_path.contains("request.registration.sensitivity_class"));
        assert!(pending_path.contains("request.registration.retention_class"));

        let stored_path = production
            .split("BankReceiptEvidencePolicy::validate(")
            .nth(2)
            .expect("已落库校验入口");
        assert!(stored_path.contains("asset.content_type"));
        assert!(stored_path.contains("asset.sensitivity_class"));
        assert!(stored_path.contains("asset.retention_class"));
        assert!(stored_path.contains("asset.destroyed_at.is_some()"));
        assert!(!production.contains("fn validate_bank_receipt_metadata"));
        assert!(!production.contains("fn validate_bank_receipt_pending_requests"));
    }

    /// 付款登记必须注册无审批 BusinessDocument。
    #[test]
    fn commit_registers_unbound_document() {
        let source = include_str!("payment.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::SupplierPayment"));
        assert!(source.contains("persist_unbound_supplier_payment_document"));
        assert!(source.contains("供应商付款为 NO_APPROVAL"));
    }

    /// 付款必须绑定执行任务并在同一事务直接过账。
    #[test]
    fn commit_records_task_and_posts_without_approval() {
        let production = include_str!("payment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("record_payment_execution"));
        assert!(production.contains("post_supplier_payment_in_transaction"));
        assert!(production.contains("PaymentPostSource::ExecutionTask"));
        assert!(!production.contains("pub async fn submit_supplier_payment"));
        assert!(!production.contains("prepare_start"));
    }

    /// 收款账号掩码不得泄露除末四位外的内容。
    #[test]
    fn recipient_mask_only_contains_last_four() {
        assert_eq!(masked_bank_account_number("1234"), "********1234");
        assert_eq!(masked_bank_account_number(""), "********");
    }

    /// FIN-E02：付款核销 pending 转换、净额、分录余额、序号与实体构造必须由
    /// `PaymentAllocationLedger` 承担；旧 Service 求和/转换 helper 已删除。
    #[test]
    fn payment_allocation_ledger_is_the_only_posting_rule_source() {
        let production = include_str!("payment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("PaymentAllocationLedger::new"));
        assert!(production.contains("ledger.apply("));
        assert!(production.contains("req.pending_allocations()?"));
        assert!(!production.contains("fn pending_allocations_from_request"));
        assert!(!production.contains("fn pending_allocated_total"));
        assert!(!production.contains("fn net_payment_allocated"));
        let dto = include_str!("dto.rs");
        assert!(dto.contains("pub fn pending_allocations("));
        assert!(dto.contains("PaymentAllocationLineRequest::to_pending"));
        assert!(!dto.contains("fn pending_allocated_total"));
        assert!(!dto.contains("fn net_payment_allocated"));
    }

    /// 收款账户唯一默认解析与 expected 身份匹配必须由
    /// `PartyBankAccount` 领域方法承担，旧 Service 手工判断 helper 已删除。
    #[test]
    fn recipient_rules_live_in_party_bank_account_domain() {
        let production = concat!(
            include_str!("account.rs"),
            include_str!("payment.rs"),
            include_str!("mapping.rs"),
        );
        assert!(production.contains("PartyBankAccount::resolve_current_default(&accounts)"));
        assert!(production.contains("recipient.matches_expected("));
        assert!(!production.contains("fn ensure_expected_payment_recipient"));
    }

    /// 列表投影不得逐行解析收款账户；收款账户只允许详情/任务路径加载。
    #[test]
    fn list_views_omit_payment_recipient_lookups() {
        let production = concat!(include_str!("account.rs"), include_str!("payment.rs"));
        let account_list = production
            .split("pub async fn payable_account_list")
            .nth(1)
            .expect("应付列表")
            .split("pub async fn payable_account_detail")
            .next()
            .expect("应付列表函数体");
        assert!(account_list.contains("PayableAccountSummaryView"));
        assert!(!account_list.contains("payable_account_view("));
        assert!(!account_list.contains("resolve_optional_payment_recipient_for_read"));
        assert!(production.contains("assemble_supplier_payment_views(&payment_ids, false)"));
        assert!(!production.contains("supplier_payment_view(row.id, false)"));
        assert!(!production.contains("fn enrich_supplier_payment_view"));
        assert!(production.contains("payable_account_view(id.to_string(), true)"));
        assert!(production.contains("assemble_supplier_payment_views("));
    }

    /// FIN-R02：付款列表与详情经同一批量装载装配，页长不放大查询次数。
    #[test]
    fn supplier_payment_list_uses_constant_batch_loading() {
        let production = include_str!("payment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let list = production
            .split("pub async fn supplier_payment_list")
            .nth(1)
            .expect("付款列表")
            .split("pub async fn supplier_payment_detail")
            .next()
            .expect("列表函数体");
        assert!(list.contains("assemble_supplier_payment_views"));
        assert!(!list.contains("for row in page.items"));
        let batch = production
            .split("async fn assemble_supplier_payment_views")
            .nth(1)
            .expect("批量装配")
            .split("async fn supplier_displays_by_ids")
            .next()
            .expect("批量函数体");
        assert!(batch.contains("find_supplier_payments_by_ids"));
        assert!(batch.contains("find_allocations_by_payments"));
        assert!(batch.contains("enrich_payment_allocation_views_batched"));
    }

    /// FIN-R02：批量缺失付款保持 NotFound 语义，缺失银行回单不静默为空。
    #[test]
    fn supplier_payment_batch_missing_facts_fail_closed() {
        let production = include_str!("payment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let batch = production
            .split("async fn assemble_supplier_payment_views")
            .nth(1)
            .expect("批量装配")
            .split("async fn supplier_displays_by_ids")
            .next()
            .expect("批量函数体");
        assert!(batch.contains("供应商付款单不存在"));
        let receipts = production
            .split("async fn bank_receipt_views_by_ids")
            .nth(1)
            .expect("回单批量")
            .split("async fn recipient_views_by_ids")
            .next()
            .expect("回单函数体");
        assert!(receipts.contains("银行回单不存在"));
    }

    /// FIN-R02：敏感银行信息只经掩码视图公开，不泄漏明文事实。
    #[test]
    fn supplier_payment_batch_masks_sensitive_bank_facts() {
        let production = concat!(include_str!("payment.rs"), include_str!("mapping.rs"));
        let batch = production
            .split("async fn recipient_views_by_ids")
            .nth(1)
            .expect("收款批量");
        assert!(batch.contains("payment_recipient_view"));
        assert!(!batch.contains("account_number_ciphertext"));
        assert!(!batch.contains("account_number_query_hmac"));
        assert!(production.contains("account_number_masked"));
    }

    /// 历史应付读取不得因供应商已软删除而返回 NotFound。
    #[test]
    fn historical_payable_recipient_projection_is_nonblocking() {
        let production = include_str!("mapping.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let read_projection = production
            .split("async fn resolve_optional_payment_recipient_for_read")
            .nth(1)
            .expect("只读收款账户解析")
            .split("async fn resolve_optional_party_payment_recipient")
            .next()
            .expect("只读解析函数体");
        assert!(read_projection.contains("return Ok(None)"));
        assert!(!read_projection.contains("Error::NotFound"));
    }

    /// 事务内收款账户校验必须以页面版本执行真实 CAS 写入。
    #[test]
    fn payment_recipient_lock_performs_cas_write() {
        let production = include_str!("payment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let lock = production
            .split("async fn lock_expected_payment_recipient")
            .nth(1)
            .expect("收款账户事务锁")
            .split("fn payment_recipient_lock_error")
            .next()
            .expect("收款账户事务锁函数体");
        assert!(lock.contains("expected_version"));
        assert!(lock.contains(".update(&mut recipient, executor)"));
    }

    /// 正常付款生产代码不得创建付款审批实例或暴露审批命令。
    #[test]
    fn production_has_no_supplier_payment_approval_commands() {
        let production = concat!(
            include_str!("account.rs"),
            include_str!("payment.rs"),
            include_str!("invoice.rs"),
            include_str!("mapping.rs"),
        );
        assert!(!production.contains("SupplierPaymentStatus::PendingReview"));
        assert!(!production.contains("submit_supplier_payment"));
        assert!(!production.contains("cancel_supplier_payment_approval"));
        assert!(!production.contains("start_supplier_payment_approval"));
        assert!(!production.contains("pub async fn post_supplier_payment"));
        assert!(!production.contains("SupplierPaymentStatus::InApproval"));
    }
}
