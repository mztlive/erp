//! 资金单据简报字段映射。

use std::collections::HashMap;

use erp_finance::entity::payable::PayableEntry;
use erp_finance::entity::payable::PaymentAllocation;
use erp_finance::entity::payable::{PayableAccount, SupplierPayment};
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::entity::receivable::PendingReceiptAllocation;
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::entity::receivable::ReceivableEntry;
use {
    erp_sales::entity::sales_order::SalesOrderRevisionLine,
    erp_sales::entity::sales_order::SalesOrderVoucherLineRevision,
};

use super::super::brief::{
    format_instant_date, join_list_summary, line_title, non_empty, push_section, BriefLine,
    ObjectBriefSource, BRIEF_LINE_LIMIT,
};
use super::super::presentation::format_yuan;
use super::super::WorkbenchObjectFact;
use super::{FundsOriginBrief, FundsOrigins};
use crate::workbench::authority::funds::origins::{
    payable_entry_counterparties, payment_counterparties, receipt_counterparties,
    receivable_entry_counterparties,
};

/// 组装付款执行任务的应付对象事实。
pub(super) fn payable_account_fact(
    account: PayableAccount,
    supplier: Option<String>,
    purchase_no: Option<String>,
    due_date: Option<String>,
) -> WorkbenchObjectFact {
    let brief_source = payable_brief_source(&account, supplier.clone(), purchase_no.clone(), due_date);
    let authority =
        crate::workbench::authority::funds::mapping::payable_account_fact(&account, supplier, purchase_no);
    let mut fact = WorkbenchObjectFact::from_authority(authority);
    fact.display.brief_source = Some(brief_source);
    fact
}

/// 组装付款执行任务的结构化应付简报。
pub(super) fn payable_brief_source(
    account: &PayableAccount,
    supplier: Option<String>,
    purchase_no: Option<String>,
    due_date: Option<String>,
) -> ObjectBriefSource {
    let mut sections = Vec::new();
    push_section(&mut sections, "供应商", supplier.as_deref(), false);
    push_section(&mut sections, "采购单", purchase_no.as_deref(), false);
    push_section(
        &mut sections,
        "未付金额",
        Some(format_yuan(&account.open_total)).as_deref(),
        true,
    );
    push_section(
        &mut sections,
        "已付金额",
        Some(format_yuan(&account.settled_total)).as_deref(),
        true,
    );
    push_section(&mut sections, "计划付款日", due_date.as_deref(), false);
    ObjectBriefSource {
        customer: supplier.clone(),
        amount_label: None,
        extra_sections: sections,
        list_summary: join_list_summary([
            supplier,
            purchase_no.map(|no| format!("采购单 {no}")),
            due_date.map(|date| format!("计划付款 {date}")),
            Some(format!("未付 {}", format_yuan(&account.open_total))),
        ]),
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
    }
}

/// 组装回款单简报。
///
/// # 参数
/// * `receipt` - 回款单
/// * `counterparty` - 往来主体名称
/// * `lines` - 待核销销售单行
///
/// # 返回
/// 返回可上屏的对象简报源。
///
/// # 错误
/// 无。
pub(super) fn receipt_brief_source(
    receipt: &CustomerReceipt,
    counterparty: Option<&str>,
    lines: Vec<BriefLine>,
) -> ObjectBriefSource {
    let more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
    let mut visible = lines;
    visible.truncate(BRIEF_LINE_LIMIT);
    let mut sections = Vec::new();
    push_section(&mut sections, "往来主体", counterparty, false);
    push_section(
        &mut sections,
        "回款金额",
        Some(format_yuan(&receipt.amount)).as_deref(),
        true,
    );
    push_section(
        &mut sections,
        "到账日",
        Some(format_instant_date(receipt.received_at)).as_deref(),
        false,
    );
    push_section(
        &mut sections,
        "银行流水",
        receipt.bank_reference.as_deref(),
        false,
    );
    if !visible.is_empty() {
        push_section(
            &mut sections,
            "待核销",
            Some(format!("{} 笔", visible.len() + more_count as usize)).as_deref(),
            false,
        );
    }
    let first_line = visible.first().map(|line| line.title.clone());
    ObjectBriefSource {
        customer: None,
        amount_label: None,
        extra_sections: sections,
        list_summary: join_list_summary([
            counterparty.map(str::to_string),
            Some(format_yuan(&receipt.amount)),
            receipt.bank_reference.clone().and_then(|text| non_empty(&text)),
            first_line,
        ]),
        lines: visible,
        more_count,
        submitter_name: None,
    }
}

/// 把冻结卡券版本行转成卡券票款专属简报行。
///
/// # 参数
/// * `line` - 销售版本公共行
/// * `voucher` - 与公共行一一对应的卡券版本行
///
/// # 返回
/// 返回品名、面值、张数、成交金额和配赠金额。
///
/// # 错误
/// 无。
pub(super) fn voucher_account_line(
    line: &SalesOrderRevisionLine,
    voucher: &SalesOrderVoucherLineRevision,
) -> BriefLine {
    BriefLine {
        title: line_title(&line.item_name_snapshot, line.spec_snapshot.as_deref()),
        quantity: Some(format!(
            "面值 {} × {} 张",
            format_yuan(&voucher.face_value),
            voucher.card_count
        )),
        due_label: Some(format!(
            "成交 {} · 配赠 {}",
            format_yuan(&voucher.transaction_amount),
            format_yuan(&voucher.gift_amount)
        )),
    }
}

/// 格式化当前开票抬头税务资料状态。
///
/// # 参数
/// * `tax_no` - 当前业务日生效的税号
///
/// # 返回
/// 有有效税号时返回可核对文案；缺失时返回明确阻断提示。
///
/// # 错误
/// 无。
pub(super) fn invoice_tax_profile_label(tax_no: Option<&str>) -> String {
    match tax_no.and_then(non_empty) {
        Some(tax_no) => format!("税务资料有效 · 税号 {tax_no}"),
        None => "未找到当前有效税务资料；登记开票前必须补齐".to_string(),
    }
}

/// 把待过账核销转成简报行。
///
/// # 参数
/// * `allocations` - 待过账核销
/// * `entries` - 分录 ID 到分录
/// * `accounts` - 子账 ID 到子账
/// * `sales_nos` - 销售单 ID 到单号
///
/// # 返回
/// 返回销售单号和核销金额。
///
/// # 错误
/// 无。
pub(super) fn receipt_brief_lines(
    allocations: &[PendingReceiptAllocation],
    entries: &HashMap<String, ReceivableEntry>,
    accounts: &HashMap<String, ReceivableAccount>,
    sales_nos: &HashMap<String, String>,
) -> Vec<BriefLine> {
    allocations
        .iter()
        .map(|allocation| {
            let sales_no = entries
                .get(&allocation.receivable_entry_id.to_string())
                .and_then(|entry| accounts.get(&entry.receivable_account_id.to_string()))
                .and_then(|account| sales_nos.get(&account.sales_order_id.to_string()))
                .cloned();
            BriefLine {
                title: sales_no
                    .map(|no| format!("销售单 {no}"))
                    .unwrap_or_else(|| "应收分录".to_string()),
                quantity: Some(format_yuan(&allocation.allocated_amount)),
                due_label: None,
            }
        })
        .collect()
}

/// 把已过账付款核销事实转成采购单号、动作与金额。
///
/// # 参数
/// * `allocations` - 已过账付款核销事实
/// * `entries` - 应付分录 ID 到分录
/// * `accounts` - 应付子账 ID 到子账
/// * `purchase_nos` - 采购单 ID 到单号
///
/// # 返回
/// 返回采购单号和核销金额；业务单号缺失时使用明确占位。
///
/// # 错误
/// 无。
pub(super) fn payment_brief_lines(
    allocations: &[PaymentAllocation],
    entries: &HashMap<String, PayableEntry>,
    accounts: &HashMap<String, PayableAccount>,
    purchase_nos: &HashMap<String, String>,
) -> Vec<BriefLine> {
    allocations
        .iter()
        .map(|allocation| {
            let purchase_no = entries
                .get(&allocation.payable_entry_id.to_string())
                .and_then(|entry| accounts.get(&entry.payable_account_id.to_string()))
                .and_then(|account| purchase_nos.get(&account.source_document_id))
                .cloned();
            BriefLine {
                title: purchase_no
                    .map(|no| format!("{}采购单 {no}", allocation.allocation_action.label()))
                    .unwrap_or_else(|| format!("{}采购单号待补全", allocation.allocation_action.label())),
                quantity: Some(format_yuan(&allocation.allocated_amount)),
                due_label: None,
            }
        })
        .collect()
}

/// 把原资金事实与核销影响追加到退款或冲正简报。
///
/// # 参数
/// * `brief` - 待补充的金额原因简报
/// * `origin` - 原回款、付款、应收或应付上下文
/// * `has_evidence` - 当前退款或冲正是否已上传凭证
/// * `impact` - 通过后的正式核销影响
///
/// # 返回
/// 无。原事实读取失败时显示业务单号待补全，不回退内部 ID。
///
/// # 错误
/// 无。
pub(super) fn append_funds_origin(
    brief: &mut ObjectBriefSource,
    origin: Option<&FundsOriginBrief>,
    has_evidence: bool,
    impact: &str,
) {
    let original_document = origin
        .and_then(|item| item.original_document.as_deref())
        .unwrap_or("原始资金单据业务号待补全");
    push_section(
        &mut brief.extra_sections,
        "原始资金单据",
        Some(original_document),
        false,
    );
    push_section(
        &mut brief.extra_sections,
        "原单金额",
        origin.and_then(|item| item.original_amount.as_deref()),
        true,
    );
    push_section(
        &mut brief.extra_sections,
        "原银行流水/凭证",
        origin.and_then(|item| item.bank_reference.as_deref()),
        false,
    );
    push_section(
        &mut brief.extra_sections,
        "原核销事实",
        origin.and_then(|item| item.allocation_summary.as_deref()),
        false,
    );
    push_section(&mut brief.extra_sections, "核销影响", Some(impact), false);
    push_section(
        &mut brief.extra_sections,
        "本单凭证附件",
        Some(if has_evidence { "已上传" } else { "未上传" }),
        false,
    );
    let lines = origin.map(|item| item.lines.clone()).unwrap_or_default();
    brief.more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
    brief.lines = lines.into_iter().take(BRIEF_LINE_LIMIT).collect();
    brief.list_summary = join_list_summary([
        Some(brief.list_summary.clone()),
        Some(original_document.to_string()),
    ]);
}

/// 金额加原因类资金单据的共用简报。
///
/// # 参数
/// * `amount_title` - 退款或冲正金额名称
/// * `amount_label` - 金额展示
/// * `fields` - 额外键值
/// * `list_summary` - 列表一行摘要
///
/// # 返回
/// 返回可上屏的对象简报源。
///
/// # 错误
/// 无。
pub(super) fn amount_reason_brief(
    amount_title: &str,
    amount_label: String,
    fields: Vec<(&str, Option<String>)>,
    list_summary: String,
) -> ObjectBriefSource {
    let mut sections = Vec::new();
    push_section(&mut sections, amount_title, Some(amount_label.as_str()), true);
    for (label, value) in fields {
        push_section(&mut sections, label, value.as_deref(), false);
    }
    ObjectBriefSource {
        customer: None,
        amount_label: None,
        extra_sections: sections,
        list_summary,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
    }
}

/// 同一来源行分别投影命令名称与富简报；富简报保留名称缺失的原始单据。
pub(super) fn receipt_origins_from_rows(
    receipts: Vec<CustomerReceipt>,
    party_names: HashMap<String, String>,
    lines: HashMap<String, Vec<BriefLine>>,
) -> FundsOrigins {
    let counterparties = receipt_counterparties(&receipts, &party_names);
    let briefs = receipts
        .into_iter()
        .map(|receipt| {
            let id = receipt.base.id.clone();
            let allocation_summary = if receipt.pending_allocations.is_empty() {
                "未记录核销分配".to_string()
            } else {
                format!("已关联 {} 笔核销", receipt.pending_allocations.len())
            };
            (
                id.clone(),
                FundsOriginBrief {
                    counterparty: party_names
                        .get(&receipt.counterparty_party_id.to_string())
                        .cloned(),
                    original_document: Some(format!("回款单 {}", receipt.receipt_no)),
                    original_amount: Some(format_yuan(&receipt.amount)),
                    bank_reference: receipt.bank_reference,
                    allocation_summary: Some(allocation_summary),
                    lines: lines.get(&id).cloned().unwrap_or_default(),
                },
            )
        })
        .collect();
    FundsOrigins {
        counterparties,
        briefs,
    }
}

/// 同一来源行分别投影命令名称与富简报；富简报保留名称缺失的原始单据。
pub(super) fn payment_origins_from_rows(
    payments: Vec<SupplierPayment>,
    supplier_names: HashMap<String, String>,
    lines: HashMap<String, Vec<BriefLine>>,
) -> FundsOrigins {
    let counterparties = payment_counterparties(&payments, &supplier_names);
    let briefs = payments
        .into_iter()
        .map(|payment| {
            let id = payment.base.id.clone();
            let allocation_count = lines.get(&id).map_or(0, Vec::len);
            let allocation_summary = if allocation_count == 0 {
                "未记录核销分配".to_string()
            } else {
                format!("已关联 {allocation_count} 笔核销事实")
            };
            (
                id.clone(),
                FundsOriginBrief {
                    counterparty: supplier_names.get(&payment.supplier_id.to_string()).cloned(),
                    original_document: Some(format!("付款单 {}", payment.payment_no)),
                    original_amount: Some(format_yuan(&payment.amount)),
                    bank_reference: payment.bank_reference,
                    allocation_summary: Some(allocation_summary),
                    lines: lines.get(&id).cloned().unwrap_or_default(),
                },
            )
        })
        .collect();
    FundsOrigins {
        counterparties,
        briefs,
    }
}

/// 同一来源行分别投影命令名称与富简报；富简报保留名称缺失的原始单据。
pub(super) fn receivable_origins_from_rows(
    entries: Vec<ReceivableEntry>,
    accounts: HashMap<String, ReceivableAccount>,
    party_names: HashMap<String, String>,
    sales_nos: HashMap<String, String>,
) -> FundsOrigins {
    let counterparties = receivable_entry_counterparties(&entries, &accounts, &party_names);
    let briefs = entries
        .into_iter()
        .map(|entry| {
            let account = accounts.get(&entry.receivable_account_id.to_string());
            let sales_no = account.and_then(|account| sales_nos.get(&account.sales_order_id.to_string()));
            let counterparty = account.and_then(|account| {
                party_names
                    .get(&account.counterparty_party_id.to_string())
                    .cloned()
            });
            let title = sales_no
                .map(|no| format!("销售单 {no}"))
                .unwrap_or_else(|| "销售单号待补全".to_string());
            let id = entry.base.id.clone();
            (
                id,
                FundsOriginBrief {
                    counterparty,
                    original_document: Some(format!("应收分录 · {title}")),
                    original_amount: Some(format_yuan(&entry.amount)),
                    bank_reference: None,
                    allocation_summary: Some(format!("原应收到期 {}", entry.due_date)),
                    lines: vec![BriefLine {
                        title,
                        quantity: Some(format_yuan(&entry.amount)),
                        due_label: Some(format!("{} 到期", entry.due_date)),
                    }],
                },
            )
        })
        .collect();
    FundsOrigins {
        counterparties,
        briefs,
    }
}

/// 同一来源行分别投影命令名称与富简报；富简报保留名称缺失的原始单据。
pub(super) fn payable_origins_from_rows(
    entries: Vec<PayableEntry>,
    accounts: HashMap<String, PayableAccount>,
    supplier_names: HashMap<String, String>,
    purchase_nos: HashMap<String, String>,
) -> FundsOrigins {
    let counterparties = payable_entry_counterparties(&entries, &accounts, &supplier_names);
    let briefs = entries
        .into_iter()
        .map(|entry| {
            let account = accounts.get(&entry.payable_account_id.to_string());
            let purchase_no = account.and_then(|account| purchase_nos.get(&account.source_document_id));
            let counterparty =
                account.and_then(|account| supplier_names.get(&account.supplier_id.to_string()).cloned());
            let title = purchase_no
                .map(|no| format!("采购单 {no}"))
                .unwrap_or_else(|| "采购单号待补全".to_string());
            let id = entry.base.id.clone();
            (
                id,
                FundsOriginBrief {
                    counterparty,
                    original_document: Some(format!("应付分录 · {title}")),
                    original_amount: Some(format_yuan(&entry.amount)),
                    bank_reference: None,
                    allocation_summary: Some(format!("原应付到期 {}", entry.due_date)),
                    lines: vec![BriefLine {
                        title,
                        quantity: Some(format_yuan(&entry.amount)),
                        due_label: Some(format!("{} 到期", entry.due_date)),
                    }],
                },
            )
        })
        .collect();
    FundsOrigins {
        counterparties,
        briefs,
    }
}
/// 富显示明确覆盖往来字段；None 是原结果，不回退 authority 名称。
pub(super) fn funds_fact_display(
    authority: erp_workflow::ports::ObjectFact,
    counterparty: Option<String>,
) -> WorkbenchObjectFact {
    let mut fact = WorkbenchObjectFact::from_authority(authority);
    fact.display.counterparty_label = counterparty;
    fact
}
/// 卡券富显示可由券行扩充，其 predicate 不得覆盖命令 authority。
pub(super) fn receivable_fact_display(
    authority: erp_workflow::ports::ObjectFact,
    display_voucher: bool,
) -> WorkbenchObjectFact {
    let mut fact = WorkbenchObjectFact::from_authority(authority);
    fact.display.impact_summary =
        Some(crate::workbench::authority::funds::mapping::receivable_account_impact(display_voucher));
    fact
}
/// 富来源优先存在的原单条目，即使该条目名称为空也不落到分录。
pub(super) fn select_funds_origin<'a, P: ToString, E: ToString>(
    primary_id: Option<&P>,
    primary: &'a FundsOrigins,
    entry_id: Option<&E>,
    entries: &'a FundsOrigins,
) -> Option<&'a FundsOriginBrief> {
    primary_id
        .and_then(|id| primary.briefs.get(&id.to_string()))
        .or_else(|| entry_id.and_then(|id| entries.briefs.get(&id.to_string())))
}

#[cfg(test)]
mod tests {
    use erp_core::money::Amount;

    use super::*;
    use crate::workbench::brief::BriefLine;

    fn amount(value: &str) -> Amount {
        value.parse().expect("测试金额必须合法")
    }

    #[test]
    fn receipt_list_summary_joins_counterparty_amount_and_first_line() {
        let lines = [BriefLine {
            title: "销售单 SO-1".to_string(),
            quantity: Some("¥8,000".to_string()),
            due_label: None,
        }];
        let summary = join_list_summary([
            Some("华东纸业".into()),
            Some(format_yuan(&amount("8000"))),
            Some("流水-9".into()),
            lines.first().map(|line| line.title.clone()),
        ]);
        assert!(summary.contains("华东纸业"));
        assert!(summary.contains("¥8,000"));
        assert!(summary.contains("销售单 SO-1"));
    }

    #[test]
    fn amount_reason_brief_keeps_amount_and_reason() {
        let brief = amount_reason_brief(
            "退款金额",
            "¥500".to_string(),
            vec![("原因", Some("重复到账".into()))],
            "¥500 · 重复到账".to_string(),
        );
        assert!(brief.amount_label.is_none());
        let assembled = crate::workbench::brief::assemble_brief(&brief, None);
        assert!(assembled
            .sections
            .iter()
            .any(|s| s.label == "退款金额" && s.value == "¥500"));
        assert!(!assembled.sections.iter().any(|s| s.label == "含税金额"));
        assert!(brief.extra_sections.iter().any(|section| section.label == "原因"));
        assert_eq!(brief.list_summary, "¥500 · 重复到账");
    }

    #[test]
    fn funds_origin_adds_original_document_allocation_impact_and_evidence() {
        let mut brief = amount_reason_brief(
            "冲正金额",
            "¥500".to_string(),
            vec![("原因", Some("重复到账".into()))],
            "¥500 · 重复到账".to_string(),
        );
        let origin = FundsOriginBrief {
            original_document: Some("回款单 CR-1".to_string()),
            original_amount: Some("¥800".to_string()),
            bank_reference: Some("BANK-9".to_string()),
            allocation_summary: Some("已关联 1 笔核销".to_string()),
            lines: vec![BriefLine {
                title: "销售单 SO-1".to_string(),
                quantity: Some("¥800".to_string()),
                due_label: None,
            }],
            ..FundsOriginBrief::default()
        };

        append_funds_origin(&mut brief, Some(&origin), true, "通过后追加反向核销，原事实保留");

        assert!(brief
            .extra_sections
            .iter()
            .any(|section| { section.label == "原始资金单据" && section.value == "回款单 CR-1" }));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "核销影响"));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "本单凭证附件" && section.value == "已上传"));
        assert_eq!(brief.lines[0].title, "销售单 SO-1");
    }

    #[test]
    fn invoice_tax_profile_label_fails_closed_when_current_profile_is_missing() {
        assert_eq!(
            invoice_tax_profile_label(Some("91310000ABC")),
            "税务资料有效 · 税号 91310000ABC"
        );
        assert_eq!(
            invoice_tax_profile_label(None),
            "未找到当前有效税务资料；登记开票前必须补齐"
        );
    }

    fn original_receipt() -> CustomerReceipt {
        use erp_core::ids::{CustomerReceiptId, PartyId};
        use erp_finance::entity::receivable::CustomerReceiptData;
        CustomerReceipt::new(
            CustomerReceiptId::new("receipt-1"),
            CustomerReceiptData {
                receipt_no: "CR-1".to_string(),
                counterparty_party_id: PartyId::new("party-1"),
                customer_id: None,
                received_at: erp_core::common::time::Instant::from_unix_secs(1),
                amount: amount("800"),
                bank_reference: None,
            },
            "entity-creator",
        )
        .unwrap()
    }

    #[test]
    fn origin_maps_keep_distinct_membership_for_a_nameless_receipt() {
        let origins = receipt_origins_from_rows(vec![original_receipt()], HashMap::new(), HashMap::new());
        assert!(origins.counterparties.is_empty());
        assert_eq!(origins.briefs.len(), 1);
        let brief = origins.briefs.get("receipt-1").unwrap();
        assert_eq!(brief.counterparty, None);
        assert_eq!(brief.original_document.as_deref(), Some("回款单 CR-1"));
        assert_eq!(brief.original_amount.as_deref(), Some("¥800"));
    }

    #[test]
    fn customer_refund_authority_can_fall_back_past_nameless_rich_origin() {
        use erp_core::ids::{CustomerAccountId, CustomerReceiptId, CustomerRefundId, ReceivableEntryId};
        use erp_returns::entity::returns::{CustomerRefund, CustomerRefundData};
        let mut refund = CustomerRefund::new(
            CustomerRefundId::new("refund-1"),
            CustomerRefundData {
                refund_no: "RF-1".into(),
                sales_return_case_id: None,
                customer_id: CustomerAccountId::new("customer-missing"),
                original_receipt_id: Some(CustomerReceiptId::new("receipt-1")),
                original_receivable_entry_id: None,
                reason_code: None,
                reason_text: "原款重复".into(),
                amount: amount("10"),
                handled_by: "handler".into(),
                reviewed_by: "reviewer".into(),
                occurred_at: erp_core::common::time::Instant::from_unix_secs(1),
                evidence_attachment_id: None,
            },
            "entity-creator",
        )
        .unwrap();
        // 历史同时带两类来源的已读行必须沿用原 fallback；不得在读取时补新构造校验。
        refund.original_receivable_entry_id = Some(ReceivableEntryId::new("entry-1"));
        let origins = receipt_origins_from_rows(vec![original_receipt()], HashMap::new(), HashMap::new());
        let entries = FundsOrigins {
            counterparties: HashMap::from([("entry-1".into(), "分录主体".into())]),
            briefs: HashMap::from([(
                "entry-1".into(),
                FundsOriginBrief {
                    counterparty: Some("分录主体".into()),
                    ..Default::default()
                },
            )]),
        };
        let actor = "audit-actor".to_string();
        let authority = crate::workbench::authority::funds::mapping::customer_refund_fact(
            &refund,
            Some(&actor),
            None,
            &origins.counterparties,
            &entries.counterparties,
        );
        let origin = select_funds_origin(
            refund.original_receipt_id.as_ref(),
            &origins,
            refund.original_receivable_entry_id.as_ref(),
            &entries,
        );
        let fact = funds_fact_display(authority, origin.and_then(|row| row.counterparty.clone()));
        assert_eq!(fact.authority.root_document_id, "refund-1");
        assert_eq!(fact.authority.created_by, "audit-actor");
        assert_eq!(fact.authority.counterparty_label.as_deref(), Some("分录主体"));
        assert_eq!(fact.display.counterparty_label, None);
        assert_eq!(origin.unwrap().original_document.as_deref(), Some("回款单 CR-1"));
    }

    #[test]
    fn supplier_refund_display_keeps_missing_supplier_while_authority_uses_origin() {
        use erp_core::ids::{SupplierAccountId, SupplierPaymentId, SupplierRefundId};
        use erp_returns::entity::returns::{SupplierRefund, SupplierRefundData};
        let refund = SupplierRefund::new(
            SupplierRefundId::new("refund-2"),
            SupplierRefundData {
                refund_no: "SR-1".into(),
                purchase_return_order_id: None,
                supplier_id: SupplierAccountId::new("supplier-missing"),
                original_payment_id: Some(SupplierPaymentId::new("payment-1")),
                original_payable_entry_id: None,
                reason_code: None,
                reason_text: "重复付款".into(),
                amount: amount("10"),
                handled_by: "handler".into(),
                reviewed_by: "reviewer".into(),
                occurred_at: erp_core::common::time::Instant::from_unix_secs(1),
                evidence_attachment_id: None,
            },
            "entity-creator",
        )
        .unwrap();
        let origins = HashMap::from([("payment-1".into(), "来源供应商".into())]);
        let authority = crate::workbench::authority::funds::mapping::supplier_refund_fact(
            &refund,
            None,
            None,
            &origins,
            &HashMap::new(),
        );
        let fact = funds_fact_display(authority, None);
        assert_eq!(fact.authority.created_by, "");
        assert_eq!(fact.authority.counterparty_label.as_deref(), Some("来源供应商"));
        assert_eq!(fact.display.counterparty_label, None);
        assert_eq!(fact.authority.root_document_id, "refund-2");
    }

    #[test]
    fn voucher_display_impact_does_not_replace_command_revision_predicate() {
        use crate::workbench::authority::funds::mapping::voucher_revision_ids;
        use erp_core::common::time::Instant;
        use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SkuId};
        use erp_sales::entity::sales_order::{
            HeaderSnapshotData, RevisionSource, SalesOrderRevision, SalesOrderRevisionData,
        };
        let mut revision = SalesOrderRevision::new(
            SalesOrderRevisionId::new("revision-1"),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new("sales-1"),
                revision_no: 1,
                revision_source: RevisionSource::ErpApproval,
                previous_revision_id: None,
                content_hash: "hash".into(),
                customer_revision_id: None,
                contract_revision_id: None,
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".into(),
                    contract_no: None,
                    settlement_party_name: None,
                    payment_term_code: "NET30".into(),
                    payment_term_name: "月结 30 天".into(),
                    invoice_type: "增值税专用发票".into(),
                    tax_point: "6".into(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                gross_amount: amount("10"),
                net_amount: amount("10"),
                tax_amount: amount("0"),
                effective_at: Instant::from_unix_secs(1),
                recorded_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap();
        let command_voucher =
            voucher_revision_ids(std::slice::from_ref(&revision)).contains(&revision.base.id);
        assert!(!command_voucher);
        let mut authority = erp_workflow::ports::ObjectFact::new("sales-1", "应收子账 1", "creator");
        authority.impact_summary =
            Some(crate::workbench::authority::funds::mapping::receivable_account_impact(command_voucher));
        let fact = receivable_fact_display(authority, true);
        assert_eq!(
            fact.authority.impact_summary.as_deref(),
            Some("不复核则票款与开票事实不能确认")
        );
        assert_eq!(
            fact.display.impact_summary.as_deref(),
            Some("不复核则卡券票款、开票与兑付前置事实不能确认")
        );
        assert_eq!(fact.authority.root_document_id, "sales-1");
        assert_eq!(fact.authority.created_by, "creator");
        // 读取既有修订时仍按任一标志识别，不追加新构造时的成对校验。
        revision.voucher_category_sku_id = Some(SkuId::new("voucher-sku"));
        assert!(voucher_revision_ids(std::slice::from_ref(&revision)).contains(&revision.base.id));
        revision.voucher_category_sku_id = None;
        revision.voucher_expiry_at = Some(Instant::from_unix_secs(2));
        assert!(voucher_revision_ids(std::slice::from_ref(&revision)).contains(&revision.base.id));
    }
}
