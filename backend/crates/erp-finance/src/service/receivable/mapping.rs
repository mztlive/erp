//! Finance fact projections and stable money, version, and ledger error contracts.

use erp_core::money::Amount;

use crate::dto::receivable::{ReceivableInvoiceFactView, ReceivableReceiptFactView};
/// 固定零金额与账本共用同一实体层实现（`0.00` 确定性拼写）。
///
/// # 返回
/// 返回 `0.00`。
///
/// 公开包装：`erp-read-models` 的资金作用域投影（`funds_scope` 等）跨 crate 复用本入口，保持与基线一致的
/// 公开路径 `service::receivable::mapping::zero_amount`；实体层实现保持 crate 内可见。
pub fn zero_amount() -> Amount {
    crate::entity::receivable::allocation_amount::zero_amount()
}
use crate::entity::receivable::{CustomerReceiptStatus, InvoiceKind, InvoiceStatus};
use crate::ports::receivable::ReceivableSnapshot;
use crate::{Error, Result};

/// 装配当前账户关联的正式回款事实投影。
pub fn receipt_fact_views(snapshot: &ReceivableSnapshot) -> Vec<ReceivableReceiptFactView> {
    let allocations_by_receipt = allocations_by_receipt_id(snapshot);
    let mut receipts = snapshot.receipts.iter().collect::<Vec<_>>();
    receipts.sort_by(|left, right| {
        left.received_at.cmp(&right.received_at).then_with(|| left.base.id.cmp(&right.base.id))
    });
    receipts
        .into_iter()
        .map(|receipt| {
            let allocated_to_account = allocations_by_receipt
                .get(receipt.base.id.as_str())
                .map(|lines| {
                    lines.iter().fold(zero_amount(), |total, allocation| {
                        allocation.allocation_action.apply_to_net(total, allocation.allocated_amount)
                    })
                })
                .unwrap_or_else(zero_amount);
            ReceivableReceiptFactView {
                receipt_id: receipt.base.id.clone(),
                receipt_no: receipt.receipt_no.clone(),
                received_at: receipt.received_at.as_utc().to_rfc3339(),
                gross_amount: receipt.amount,
                allocated_to_account,
                other_allocation_summary: None,
                reversed: receipt.status == CustomerReceiptStatus::Reversed,
            }
        })
        .collect()
}

/// 装配当前账户关联的正式销项发票事实投影。
pub fn invoice_fact_views(snapshot: &ReceivableSnapshot) -> Vec<ReceivableInvoiceFactView> {
    let allocations_by_invoice = allocations_by_invoice_id(snapshot);
    let mut invoices = snapshot.invoices.iter().collect::<Vec<_>>();
    invoices.sort_by(|left, right| {
        left.invoice_date.cmp(&right.invoice_date).then_with(|| left.base.id.cmp(&right.base.id))
    });
    invoices
        .into_iter()
        .map(|invoice| {
            let allocated_to_account = allocations_by_invoice
                .get(invoice.base.id.as_str())
                .map(|lines| {
                    lines.iter().fold(zero_amount(), |total, allocation| {
                        allocation.allocation_action.apply_to_net(total, allocation.allocated_gross_amount)
                    })
                })
                .unwrap_or_else(zero_amount);
            ReceivableInvoiceFactView {
                invoice_id: invoice.base.id.clone(),
                invoice_no: invoice.invoice_no.clone(),
                direction: match invoice.invoice_kind {
                    InvoiceKind::Blue => "BLUE",
                    InvoiceKind::Red => "RED",
                }
                .to_string(),
                issued_at: invoice.invoice_date.to_string(),
                gross_amount: invoice.gross_amount,
                net_amount: invoice.net_amount,
                tax_amount: invoice.tax_amount,
                allocated_to_account,
                reversed: invoice.stable.status() == InvoiceStatus::RedInvoiced,
            }
        })
        .collect()
}

/// 按回款单分组分配集合（一次分组，两视图共用；排序与输出字段不变）。
///
/// # 参数
/// * `snapshot` - 事实快照
///
/// # 返回
/// 返回 `回款单ID → 分配行` 映射（组内保持原集合顺序）。
fn allocations_by_receipt_id(
    snapshot: &ReceivableSnapshot,
) -> std::collections::HashMap<&str, Vec<&crate::entity::receivable::ReceiptAllocation>> {
    let mut grouped =
        std::collections::HashMap::<&str, Vec<&crate::entity::receivable::ReceiptAllocation>>::new();
    for allocation in &snapshot.receipt_allocations {
        grouped.entry(allocation.customer_receipt_id.as_ref()).or_default().push(allocation);
    }
    grouped
}

/// 按发票分组分配集合（一次分组；排序与输出字段不变）。
///
/// # 参数
/// * `snapshot` - 事实快照
///
/// # 返回
/// 返回 `发票ID → 分配行` 映射（组内保持原集合顺序）。
fn allocations_by_invoice_id(
    snapshot: &ReceivableSnapshot,
) -> std::collections::HashMap<&str, Vec<&crate::entity::receivable::SalesInvoiceAllocation>> {
    let mut grouped =
        std::collections::HashMap::<&str, Vec<&crate::entity::receivable::SalesInvoiceAllocation>>::new();
    for allocation in &snapshot.invoice_allocations {
        grouped.entry(allocation.invoice_id.as_ref()).or_default().push(allocation);
    }
    grouped
}

/// 将 HTTP 字符串任务版本严格解析为运行时乐观锁版本。
pub fn parse_task_version(value: &str) -> Result<u64> {
    let normalized = value.trim();
    let parsed = normalized
        .parse::<u64>()
        .map_err(|_| Error::ValidationError("任务版本必须是无符号整数字符串".to_string()))?;
    if parsed == 0 || parsed.to_string() != normalized {
        return Err(Error::ValidationError("任务版本必须是规范的正整数字符串".to_string()));
    }
    Ok(parsed)
}

/// 将回款账本错误映射为既有业务错误。
///
/// # 参数
/// * `error` - 领域错误
///
/// # 返回
/// 返回 BusinessLogicError，文案与原过账路径一致。
///
/// # 错误
/// 本函数即错误转换。
///
/// # 约束
/// 不得落到透明 Logic。
pub fn map_ledger_error(error: erp_core::Error) -> Error {
    Error::BusinessLogicError(error.to_string())
}

/// 校验乐观锁版本。
///
/// # 错误
/// 不一致时返回冲突。
pub fn ensure_expected_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))
}

#[cfg(test)]
mod mapping_error_tests {
    use application_core::ErrorClass;

    use super::{ensure_expected_version, map_ledger_error};
    use crate::Error;

    /// 账本错误转译保持原过账文案并归为业务规则类（不得落到透明 Logic）。
    #[test]
    fn ledger_error_mapping_keeps_message_and_business_rule_class() {
        let mapped = map_ledger_error(erp_core::Error::from("核销合计超过回款金额"));
        assert_eq!(mapped.class(), ErrorClass::BusinessRule);
        assert!(matches!(mapped, Error::BusinessLogicError(_)));
        assert_eq!(mapped.to_string(), "业务逻辑错误: 核销合计超过回款金额");
    }

    /// 乐观锁版本一致通过、不一致冲突。
    #[test]
    fn expected_version_check_passes_only_on_match() {
        assert!(ensure_expected_version(7, 7).is_ok());
        assert!(matches!(ensure_expected_version(7, 8), Err(Error::ConflictError(_))));
    }
}

#[cfg(test)]
mod mapping_grouping_equivalence_tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{
        CustomerAccountId, CustomerReceiptId, InvoiceId, PartyId, ReceiptAllocationId, ReceivableAccountId,
        ReceivableEntryId, SalesInvoiceAllocationId,
    };
    use erp_core::money::Amount;

    use super::{invoice_fact_views, receipt_fact_views, zero_amount};
    use crate::entity::receivable::{
        AllocationAction, CustomerReceipt, CustomerReceiptData, Invoice, InvoiceData, InvoiceDirection,
        InvoiceKind, ReceiptAllocation, ReceiptAllocationData, SalesInvoiceAllocation,
        SalesInvoiceAllocationData,
    };
    use crate::ports::receivable::ReceivableSnapshot;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn receipt(id: &str) -> CustomerReceipt {
        CustomerReceipt::new(
            CustomerReceiptId::new(id),
            CustomerReceiptData {
                receipt_no: format!("RC-{id}"),
                counterparty_party_id: PartyId::new("party-1"),
                customer_id: Some(CustomerAccountId::new("customer-1")),
                received_at: Instant::from_unix_secs(20),
                amount: amount("1000.00"),
                bank_reference: None,
            },
            "actor-1",
        )
        .unwrap()
    }

    fn receipt_allocation(
        id: &str,
        receipt_id: &str,
        seq: u32,
        action: AllocationAction,
        value: &str,
        reverses: Option<&str>,
    ) -> ReceiptAllocation {
        ReceiptAllocation::new(
            ReceiptAllocationId::new(id),
            ReceiptAllocationData {
                customer_receipt_id: CustomerReceiptId::new(receipt_id),
                receivable_entry_id: ReceivableEntryId::new("entry-1"),
                allocation_seq: seq,
                allocation_action: action,
                allocated_amount: amount(value),
                allocated_at: Instant::from_unix_secs(20),
                reverses_allocation_id: reverses.map(ReceiptAllocationId::new),
            },
        )
        .unwrap()
    }

    fn invoice(id: &str) -> Invoice {
        Invoice::new(
            InvoiceId::new(id),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: format!("INV-{id}"),
                invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                gross_amount: amount("1000.00"),
                net_amount: amount("800.00"),
                tax_amount: amount("200.00"),
                rounding_adjustment_amount: amount("0.00"),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "tester",
        )
        .unwrap()
    }

    fn invoice_allocation(
        id: &str,
        invoice_id: &str,
        seq: u32,
        action: AllocationAction,
        triple: (&str, &str, &str),
        reverses: Option<&str>,
    ) -> SalesInvoiceAllocation {
        SalesInvoiceAllocation::new(
            SalesInvoiceAllocationId::new(id),
            SalesInvoiceAllocationData {
                invoice_id: InvoiceId::new(invoice_id),
                receivable_account_id: ReceivableAccountId::new("account-1"),
                allocation_seq: seq,
                allocation_action: action,
                allocated_gross_amount: amount(triple.0),
                allocated_net_amount: amount(triple.1),
                allocated_tax_amount: amount(triple.2),
                reverses_allocation_id: reverses.map(SalesInvoiceAllocationId::new),
            },
        )
        .unwrap()
    }

    fn snapshot() -> ReceivableSnapshot {
        ReceivableSnapshot {
            current_sales_order_revision_id: "sor-1".to_string(),
            sales_order_no: "SO-1".to_string(),
            sales_order_revision_no: 1,
            sales_order_snapshot_at: 1_700_000_000,
            customer_name: "客户一".to_string(),
            counterparty_party_name: None,
            entries: Vec::new(),
            receipt_allocations: vec![
                receipt_allocation("a-1", "r-1", 1, AllocationAction::Apply, "100.00", None),
                receipt_allocation("a-2", "r-1", 2, AllocationAction::Reverse, "30.00", Some("a-1")),
                receipt_allocation("a-3", "r-2", 1, AllocationAction::Apply, "50.00", None),
            ],
            invoice_allocations: vec![
                invoice_allocation(
                    "b-1",
                    "i-1",
                    1,
                    AllocationAction::Apply,
                    ("100.00", "80.00", "20.00"),
                    None,
                ),
                invoice_allocation(
                    "b-2",
                    "i-1",
                    2,
                    AllocationAction::Reverse,
                    ("30.00", "24.00", "6.00"),
                    Some("b-1"),
                ),
                invoice_allocation(
                    "b-3",
                    "i-2",
                    1,
                    AllocationAction::Apply,
                    ("50.00", "40.00", "10.00"),
                    None,
                ),
            ],
            receipts: vec![receipt("r-1"), receipt("r-2")],
            invoices: vec![invoice("i-1"), invoice("i-2")],
        }
    }

    /// 分组装配与逐单据全量扫描的旧算法输出一致，且净额正确。
    #[test]
    fn grouped_views_match_naive_per_document_scan() {
        let snapshot = snapshot();
        let receipt_views = receipt_fact_views(&snapshot);
        let invoice_views = invoice_fact_views(&snapshot);

        for (receipt_id, expected) in [("r-1", "70.00"), ("r-2", "50.00")] {
            let naive = snapshot
                .receipt_allocations
                .iter()
                .filter(|allocation| allocation.customer_receipt_id.as_ref() == receipt_id)
                .fold(zero_amount(), |total, allocation| {
                    allocation.allocation_action.apply_to_net(total, allocation.allocated_amount)
                });
            assert_eq!(naive, amount(expected));
            let view =
                receipt_views.iter().find(|view| view.receipt_id.as_str() == receipt_id).expect("回款视图");
            assert_eq!(view.allocated_to_account, naive);
        }
        for (invoice_id, expected) in [("i-1", "70.00"), ("i-2", "50.00")] {
            let naive = snapshot
                .invoice_allocations
                .iter()
                .filter(|allocation| allocation.invoice_id.as_ref() == invoice_id)
                .fold(zero_amount(), |total, allocation| {
                    allocation.allocation_action.apply_to_net(total, allocation.allocated_gross_amount)
                });
            assert_eq!(naive, amount(expected));
            let view =
                invoice_views.iter().find(|view| view.invoice_id.as_str() == invoice_id).expect("发票视图");
            assert_eq!(view.allocated_to_account, naive);
        }
    }
}
