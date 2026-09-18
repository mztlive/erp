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

    /// 银行回单可用性规则必须由 `BankReceiptEvidencePolicy` 单一入口承担：
    /// pending 待登记与 stored 已落库两条路径都调用该 VO，旧 Service 校验
    /// helper 已删除。
    #[test]
    fn bank_receipt_evidence_uses_single_policy_entry() {
        let production = concat!(
            include_str!("payment.rs"),
            include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
        )
        .split("#[cfg(test)]")
        .next()
        .expect("生产代码");
        let process = include_str!("../../attachments/payable.rs");
        assert!(process.contains("request.registration.content_type"));
        assert!(process.contains("request.registration.sensitivity_class"));
        assert!(process.contains("request.registration.retention_class"));
        assert!(process.contains("BankReceiptEvidencePolicy::validate("));

        let stored_path =
            production.split("BankReceiptEvidencePolicy::validate(").nth(1).expect("已落库校验入口");
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
        let source = concat!(
            include_str!("payment.rs"),
            include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
        );
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::SupplierPayment"));
        assert!(source.contains("persist_unbound_supplier_payment_document"));
        assert!(source.contains("供应商付款为 NO_APPROVAL"));
    }

    /// 付款必须绑定执行任务并在同一事务直接过账。
    #[test]
    fn commit_records_task_and_posts_without_approval() {
        let production = concat!(
            include_str!("payment.rs"),
            include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
        )
        .split("#[cfg(test)]")
        .next()
        .expect("生产代码");
        assert!(production.contains("record_payment_execution"));
        assert!(production.contains("post_supplier_payment"));
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
        let production = concat!(
            include_str!("payment.rs"),
            include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
        )
        .split("#[cfg(test)]")
        .next()
        .expect("生产代码");
        assert!(production.contains("PaymentAllocationLedger::new"));
        assert!(production.contains("ledger.apply("));
        assert!(production.contains("req.pending_allocations()?"));
        assert!(!production.contains("fn pending_allocations_from_request"));
        assert!(!production.contains("fn pending_allocated_total"));
        assert!(!production.contains("fn net_payment_allocated"));
        let dto_production: String = [
            include_str!("../../../../erp-finance/src/dto/payable.rs"),
            include_str!("../../../../erp-finance/src/dto/payable/account.rs"),
            include_str!("../../../../erp-finance/src/dto/payable/payment.rs"),
            include_str!("../../../../erp-finance/src/dto/payable/invoice.rs"),
        ]
        .iter()
        .map(|content| content.split("#[cfg(test)]").next().expect("生产代码"))
        .collect();
        let dto = dto_production.as_str();
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
            concat!(
                include_str!("account.rs"),
                include_str!("../../../../erp-read-models/src/finance/payable/account.rs")
            ),
            concat!(
                include_str!("payment.rs"),
                include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
                include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
            ),
            include_str!("../../../../erp-read-models/src/finance/payable/mapping.rs"),
        );
        assert!(production.contains("PartyBankAccount::resolve_current_default(&accounts)"));
        assert!(production.contains("recipient.matches_expected("));
        assert!(!production.contains("fn ensure_expected_payment_recipient"));
    }

    /// 列表投影不得逐行解析收款账户；收款账户只允许详情/任务路径加载。
    #[test]
    fn list_views_omit_payment_recipient_lookups() {
        let production = concat!(
            concat!(
                include_str!("account.rs"),
                include_str!("../../../../erp-read-models/src/finance/payable/account.rs")
            ),
            concat!(
                include_str!("payment.rs"),
                include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
                include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
            )
        );
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
        let production = concat!(
            include_str!("payment.rs"),
            include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
        )
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
        let production = concat!(
            include_str!("payment.rs"),
            include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
        )
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
        let production = concat!(
            concat!(
                include_str!("payment.rs"),
                include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
                include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
            ),
            include_str!("../../../../erp-read-models/src/finance/payable/mapping.rs")
        );
        let batch = production.split("async fn recipient_views_by_ids").nth(1).expect("收款批量");
        assert!(batch.contains("payment_recipient_view"));
        assert!(!batch.contains("account_number_ciphertext"));
        assert!(!batch.contains("account_number_query_hmac"));
        assert!(production.contains("account_number_masked"));
    }

    /// 历史应付读取不得因供应商已软删除而返回 NotFound。
    #[test]
    fn historical_payable_recipient_projection_is_nonblocking() {
        let production = include_str!("../../../../erp-read-models/src/finance/payable/mapping.rs")
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
        let production = concat!(
            include_str!("payment.rs"),
            include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
        )
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
            concat!(
                include_str!("account.rs"),
                include_str!("../../../../erp-read-models/src/finance/payable/account.rs")
            ),
            concat!(
                include_str!("payment.rs"),
                include_str!("../../../../erp-finance/src/service/payable/payment.rs"),
                include_str!("../../../../erp-read-models/src/finance/payable/payment.rs")
            ),
            include_str!("../../../../erp-finance/src/service/payable/invoice.rs"),
            include_str!("../../../../erp-read-models/src/finance/payable/mapping.rs"),
        );
        assert!(!production.contains("SupplierPaymentStatus::PendingReview"));
        assert!(!production.contains("submit_supplier_payment"));
        assert!(!production.contains("cancel_supplier_payment_approval"));
        assert!(!production.contains("start_supplier_payment_approval"));
        assert!(!production.contains("pub async fn post_supplier_payment"));
        assert!(!production.contains("SupplierPaymentStatus::InApproval"));
    }
}
