//! 资金与票款单据审批任务的事项简报装载。
//!
//! 覆盖回款、退款、冲正、付款和应收子账。创建人从审计事实回填，往来名称从
//! 主体当前修订读取。正式通过/驳回仍走单据审批命令。

mod account;
mod mapping;
mod payment;
mod query;
mod receipt;

use std::collections::HashMap;

use super::brief::BriefLine;

#[derive(Debug, Clone, Default)]
struct FundsOriginBrief {
    counterparty: Option<String>,
    original_document: Option<String>,
    original_amount: Option<String>,
    bank_reference: Option<String>,
    allocation_summary: Option<String>,
    lines: Vec<BriefLine>,
}

#[derive(Debug, Clone, Default)]
struct VoucherAccountBrief {
    expiry_label: Option<String>,
    face_summary: Option<String>,
    total_count: u64,
    lines: Vec<BriefLine>,
    more_count: u32,
}

#[derive(Debug, Clone, Default)]
struct InvoiceRequirementBrief {
    invoice_type: String,
    tax_point: String,
}

#[derive(Debug, Clone, Default)]
struct ReceivableRevisionBriefs {
    invoice_requirements: HashMap<String, InvoiceRequirementBrief>,
    vouchers: HashMap<String, VoucherAccountBrief>,
}
