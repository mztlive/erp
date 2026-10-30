//! 适配器只读证据进入财务拥有领域；不复制任意来源JSON。

use erp_finance::entity::supplier_deposit::{
    DepositCurrency, FundsBalanceObservationData, FundsEvidenceArchive, FundsEvidenceInput,
    FundsEvidenceResult, FundsTransactionDirection,
};
use erp_supply::ports::connector::common::{Snapshot, SourceRevision};
use erp_supply::ports::connector::funds::{
    SupplierFundsBalance, SupplierFundsCurrency, SupplierFundsDirection, SupplierFundsEvidence,
    SupplierFundsObservation, SupplierFundsObservationResult,
};

pub(super) fn observation(snapshot: Snapshot<SupplierFundsObservation>) -> FundsEvidenceInput {
    FundsEvidenceInput {
        source_observed_at: snapshot.stamp.observed_at,
        source_revision: revision(&snapshot.stamp.revision),
        archive: archive(snapshot.value.evidence),
        result: match snapshot.value.result {
            SupplierFundsObservationResult::Transaction(transaction) => FundsEvidenceResult::Transaction {
                stable_transaction_id: transaction.stable_transaction_id,
                external_order_id: transaction.external_order_id,
                direction: direction(transaction.direction),
                amount: transaction.amount,
                currency: currency(transaction.currency),
                occurred_at: transaction.occurred_at,
                evidence_reference: transaction.evidence_reference,
                original_transaction_id: transaction.original_transaction_id,
                bank_reference: transaction.bank_reference,
                business_code: Some(transaction.business_code),
            },
            SupplierFundsObservationResult::Unresolved(value) => FundsEvidenceResult::Unresolved {
                observation_id: value.observation_id,
                evidence_reference: value.evidence_reference,
                external_transaction_id: value.external_transaction_id,
                external_order_id: value.external_order_id,
                amount: value.amount,
                occurred_at: value.occurred_at,
                business_code: value.business_code,
                reason_code: value.reason_code,
            },
        },
    }
}

pub(super) fn archive(value: SupplierFundsEvidence) -> FundsEvidenceArchive {
    FundsEvidenceArchive {
        reference: value.reference,
        body_encrypted: value.body_encrypted,
        content_fingerprint: value.content_fingerprint,
    }
}

pub(super) fn balance(value: Snapshot<SupplierFundsBalance>) -> FundsBalanceObservationData {
    FundsBalanceObservationData {
        observation_id: value.value.evidence.content_fingerprint.clone(),
        balance: value.value.amount,
        currency: currency(value.value.currency),
        observed_at: value.stamp.observed_at,
        source_revision: revision(&value.stamp.revision),
        archive: archive(value.value.evidence),
    }
}

pub(super) fn direction(value: SupplierFundsDirection) -> FundsTransactionDirection {
    match value {
        SupplierFundsDirection::Recharge => FundsTransactionDirection::Recharge,
        SupplierFundsDirection::Debit => FundsTransactionDirection::Debit,
        SupplierFundsDirection::Return => FundsTransactionDirection::Return,
    }
}

fn currency(value: SupplierFundsCurrency) -> DepositCurrency {
    match value {
        SupplierFundsCurrency::CNY => DepositCurrency::CNY,
    }
}

fn revision(value: &SourceRevision) -> Option<String> {
    match value {
        SourceRevision::Unversioned => None,
        SourceRevision::EqualityToken(value) => Some(value.clone()),
        SourceRevision::Sequence { scope, value } => Some(format!("{scope}:{value}")),
    }
}
