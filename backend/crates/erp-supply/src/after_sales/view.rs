use erp_core::money::Amount;
use serde::Serialize;

use super::{
    ManualRefundEvidence, RefundExecution, RefundExecutionState, SettlementEvidence, SettlementScan,
    SettlementScanState,
};
use crate::Result;
use crate::ports::connector::order::RefundReceipt;
use crate::ports::connector::settlement::SettlementMovement;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementScanView {
    pub state: SettlementScanState,
    pub blocker: Option<String>,
    pub window_complete: bool,
    pub next_run_at: i64,
}
impl From<SettlementScan> for SettlementScanView {
    fn from(scan: SettlementScan) -> Self {
        Self {
            state: scan.state,
            blocker: scan.blocker,
            window_complete: scan.complete,
            next_run_at: scan.next_run_at.unix_secs(),
        }
    }
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementEvidenceView {
    pub id: String,
    pub reference_id: String,
    pub external_entry_id: String,
    pub order_id: String,
    pub external_order_no: String,
    pub occurred_at: i64,
    pub movement: SettlementMovement,
}
impl From<SettlementEvidence> for SettlementEvidenceView {
    fn from(row: SettlementEvidence) -> Self {
        Self {
            reference_id: format!("supplier-api-settlement:{}", row.base.id),
            id: row.base.id,
            external_entry_id: row.evidence.value.external_entry_id,
            order_id: row.evidence.value.order.merchant_order_no,
            external_order_no: row.evidence.value.order.external_order_no,
            occurred_at: row.evidence.value.occurred_at.unix_secs(),
            movement: row.evidence.value.movement,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct RefundExecutionView {
    pub action_id: String,
    pub state: RefundExecutionState,
    pub amount: Amount,
    pub blocker: Option<String>,
    pub receipt: Option<RefundReceipt>,
    pub receipt_version: Option<String>,
    pub fact_id: Option<String>,
    pub manual_evidence: Option<ManualRefundEvidence>,
}
impl TryFrom<RefundExecution> for RefundExecutionView {
    type Error = crate::Error;
    fn try_from(row: RefundExecution) -> Result<Self> {
        let receipt_version = if row.receipt.is_some() { Some(row.receipt_version()?) } else { None };
        Ok(Self {
            action_id: row.base.id,
            state: row.state,
            amount: row.request.amount,
            blocker: row.blocker,
            receipt: row.receipt.map(|s| s.value),
            receipt_version,
            fact_id: row.fact_id,
            manual_evidence: row.manual_evidence,
        })
    }
}
