//! 预存款管理命令，主体、供应商及实际操作人由服务端复验。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::entity::payable::PendingPaymentAllocation;
use erp_finance::entity::supplier_deposit::{
    DepositAccountIdentity, DepositReleaseReason, DepositTransactionEvidence,
};
use erp_finance::repository::supplier_deposit_query::DepositAccountStatusFilter;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenDepositAccount {
    pub operation_key: String,
    pub identity: DepositAccountIdentity,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositAccountQuery {
    pub supplier_id: Option<String>,
    pub payer_party_id: Option<String>,
    pub status: Option<DepositAccountStatusFilter>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestDepositRecharge {
    pub operation_key: String,
    pub amount: Amount,
    pub reason: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApproveDepositRecharge {
    pub expected_version: u64,
    pub approval_reference: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayDepositRecharge {
    pub expected_version: u64,
    pub payee_bank_account_id: String,
    pub paid_at: Instant,
    pub bank_reference: String,
    pub bank_receipt_asset_id: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiveDepositRecharge {
    pub operation_key: String,
    pub recharge_request_id: String,
    pub external_transaction_id: String,
    pub evidence_reference: String,
    pub received_at: Instant,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizeDepositExecution {
    pub operation_key: String,
    pub approval_reference: String,
    pub valid_from: Instant,
    pub valid_until: Instant,
    pub per_execution_limit: Amount,
    pub cumulative_limit: Amount,
    pub purchase_order_ids: Vec<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositExpectedVersion {
    pub expected_version: u64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileDepositAccount {
    pub operation_key: String,
    pub covered_from: Instant,
    pub covered_until: Instant,
    pub observed_at: Instant,
    pub remote_balance: Amount,
    pub local_balance_at_cutoff: Amount,
    pub unresolved_transaction_ids: Vec<String>,
    pub evidence_reference: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveDepositExecution {
    pub operation_key: String,
    pub execution_group_id: String,
    pub payment_request_id: String,
    pub authorization_id: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDepositDebit {
    pub operation_key: String,
    pub reservation_id: String,
    pub amount: Amount,
    pub evidence: DepositTransactionEvidence,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettleDepositUsage {
    pub operation_key: String,
    pub allocations: Vec<PendingPaymentAllocation>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindDepositConnection {
    pub connection_id: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnDepositUsage {
    pub operation_key: String,
    pub usage_id: String,
    pub amount: Amount,
    pub evidence: DepositTransactionEvidence,
}
#[derive(Debug, Clone, Serialize)]
pub struct DepositAccountList<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveDepositReconciliation {
    pub operation_key: String,
    pub expected_version: u64,
    pub balanced_reconciliation_id: String,
    pub evidence_asset_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDepositReservation {
    pub operation_key: String,
    pub expected_version: u64,
    pub reconciliation_id: String,
    pub evidence_asset_id: String,
    pub reason: DepositReleaseReason,
}
