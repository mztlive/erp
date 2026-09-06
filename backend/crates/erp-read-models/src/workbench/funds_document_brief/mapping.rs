//! 资金单据简报字段映射。

use std::collections::HashMap;

use entities::payable::{PayableAccount, PayableEntry, PaymentAllocation};
use entities::receivable::{CustomerReceipt, PendingReceiptAllocation, ReceivableAccount, ReceivableEntry};
use entities::sales_order::{SalesOrderRevisionLine, SalesOrderVoucherLineRevision};

use super::super::brief::{
    format_instant_date, join_list_summary, line_title, non_empty, push_section, BriefLine,
    ObjectBriefSource, BRIEF_LINE_LIMIT,
};
use super::super::presentation::format_yuan;
use super::super::ObjectFact;
use super::FundsOriginBrief;

/// 组装付款执行任务的应付对象事实。
pub(super) fn payable_account_fact(
    account: PayableAccount,
    supplier: Option<String>,
    purchase_no: Option<String>,
    due_date: Option<String>,
) -> ObjectFact {
    let brief_source = payable_brief_source(&account, supplier.clone(), purchase_no.clone(), due_date);
    let label = purchase_no
        .as_ref()
        .map(|no| format!("采购应付 {no}"))
        .unwrap_or_else(|| "采购应付".to_string());
    let mut fact = ObjectFact::new(account.source_document_id, label, account.stable.created_by);
    fact.counterparty_label = supplier;
    fact.impact_summary = Some(format!("未付金额 {}", format_yuan(&account.open_total)));
    fact.brief_source = Some(brief_source);
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
        amount_label: Some(format_yuan(&account.open_total)),
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
        "含税金额",
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
        amount_label: Some(format_yuan(&receipt.amount)),
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
/// * `amount_label` - 含税金额展示
/// * `fields` - 额外键值
/// * `list_summary` - 列表一行摘要
///
/// # 返回
/// 返回可上屏的对象简报源。
///
/// # 错误
/// 无。
pub(super) fn amount_reason_brief(
    amount_label: String,
    fields: Vec<(&str, Option<String>)>,
    list_summary: String,
) -> ObjectBriefSource {
    let mut sections = Vec::new();
    push_section(&mut sections, "含税金额", Some(amount_label.as_str()), true);
    for (label, value) in fields {
        push_section(&mut sections, label, value.as_deref(), false);
    }
    ObjectBriefSource {
        customer: None,
        amount_label: Some(amount_label),
        extra_sections: sections,
        list_summary,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
    }
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
            "¥500".to_string(),
            vec![("原因", Some("重复到账".into()))],
            "¥500 · 重复到账".to_string(),
        );
        assert_eq!(brief.amount_label.as_deref(), Some("¥500"));
        assert!(brief.extra_sections.iter().any(|section| section.label == "原因"));
        assert_eq!(brief.list_summary, "¥500 · 重复到账");
    }

    #[test]
    fn funds_origin_adds_original_document_allocation_impact_and_evidence() {
        let mut brief = amount_reason_brief(
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
}
