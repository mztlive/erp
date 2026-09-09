//! 付款任务合并候选的只读契约。

use crate::dto::payable::PaymentRecipientView;
use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{PayableAccountId, SupplierAccountId, WorkItemId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 查询当前付款任务可合并的同供应商开放任务。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PaymentMergeCandidatesParams {
    /// 当前工作台打开的付款执行任务。
    #[validate(custom(function = "non_blank", message = "付款任务不能为空"))]
    #[validate(length(max = 64, message = "付款任务标识不能超过 64 个字符"))]
    pub work_item_id: String,
}

/// 一条可纳入合并打款的付款执行任务。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentMergeCandidateItemView {
    /// 开放付款执行任务。
    pub work_item_id: WorkItemId,
    /// 任务乐观锁版本。
    pub task_version: String,
    /// 绑定应付子账。
    pub payable_account_id: PayableAccountId,
    /// 应付子账乐观锁版本。
    pub subject_version: String,
    /// 采购来源单据内部身份。
    pub source_document_id: String,
    /// 采购单号；来源缺失时为空。
    pub source_document_no: Option<String>,
    /// 剩余未付含税金额。
    pub open_total: Amount,
    /// 最早应付到期日。
    pub due_date: Option<BusinessDate>,
    /// 是否为当前工作台打开的任务。
    pub is_anchor: bool,
}

/// 同一供应商、同一当前收款账户下可合并的付款任务。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentMergeCandidatesView {
    /// 当前工作台打开的付款执行任务。
    pub anchor_work_item_id: WorkItemId,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商名称；主数据缺失时为空。
    pub supplier_name: Option<String>,
    /// 当前默认收款账户摘要；未配置时为空。
    pub payment_recipient: Option<PaymentRecipientView>,
    /// 候选任务未付合计。
    pub open_total: Amount,
    /// 可勾选任务，当前任务排在最前。
    pub items: Vec<PaymentMergeCandidateItemView>,
}
