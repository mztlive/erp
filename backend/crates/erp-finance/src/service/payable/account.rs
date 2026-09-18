//! 应付子账与原始分录构建；调用者负责验证采购来源并提交同一事务。
use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, PayableEntryId};
use erp_core::money::Amount;
use id_generator::next_id;

use crate::Result;
use crate::dto::payable::CreatePayableAccountRequest;
use crate::entity::payable::{
    EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData, PayableEntryType,
};
/// 在来源事实校验后构建完整应付事实；不开始事务，也不写入外域。
pub fn prepare_payable_account(
    req: CreatePayableAccountRequest,
    actor_id: &str,
) -> Result<(PayableAccount, PayableEntry)> {
    let account_id = PayableAccountId::new(next_id());
    let entry_id = PayableEntryId::new(next_id());
    let account = PayableAccount::new(
        account_id.clone(),
        PayableAccountData {
            source_document_id: req.source_document_id.clone(),
            supplier_id: req.supplier_id.clone(),
            source_type: req.source_type,
            gross_total: req.gross_total,
            settled_total: Amount::zero(),
            invoiceable_total: req.invoiceable_total.unwrap_or(req.gross_total),
            invoiced_total: Amount::zero(),
        },
        actor_id,
    )?;
    let entry = PayableEntry::new(
        entry_id,
        PayableEntryData {
            payable_account_id: account_id.clone(),
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: account.gross_total,
            due_date: req.due_date,
            source_fact_type: "purchase_order".to_string(),
            source_document_id: req.source_document_id,
            source_revision_id: req.source_revision_id,
            source_sequence: req.source_sequence,
            posted_at: Instant::now(),
        },
    )?;
    Ok((account, entry))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::SupplierAccountId;

    use super::*;
    use crate::entity::payable::{PayableAccountStatus, PayableSourceType};

    /// 创建入口迁移后，默认收票额度、原始分录来源及金额尺度保持一致。
    #[test]
    fn account_preparation_preserves_amounts_and_source_identity() {
        let req = CreatePayableAccountRequest {
            source_document_id: "purchase-1".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: Amount::from_str("123.45").unwrap(),
            invoiceable_total: None,
            due_date: BusinessDate::from_str("2026-09-07").unwrap(),
            source_revision_id: "revision-1".to_string(),
            source_sequence: 7,
        };
        let (account, entry) = prepare_payable_account(req, "actor-1").unwrap();
        assert_eq!(account.gross_total.to_string(), "123.45");
        assert_eq!(account.invoiceable_total, account.gross_total);
        assert_eq!(account.settled_total, Amount::zero());
        assert_eq!(account.invoiced_total, Amount::zero());
        assert_eq!(account.stable.status(), PayableAccountStatus::Open);
        assert_eq!(entry.payable_account_id.as_ref(), account.base.id);
        assert_eq!(entry.amount, account.gross_total);
        assert_eq!(entry.source_document_id, "purchase-1");
        assert_eq!(entry.source_revision_id, "revision-1");
        assert_eq!(entry.source_sequence, 7);
        assert_eq!(entry.entry_type, PayableEntryType::Original);
        assert_eq!(entry.direction, EntryDirection::Increase);
    }
}
