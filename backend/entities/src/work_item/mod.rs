//! 域 D03 `work_item`：审批及独立人工任务的当前责任事实（页面：W01、W02）。
//!
//! `status` 只表达 `OPEN / COMPLETED / CLOSED` 生命周期；开放任务必须有个人责任人。
//! 审批推进与正式业务结果不由本域决定。

mod card_funds_command;
mod due;
mod entity;
mod finance_responsibility;
mod finance_task;
mod fulfillment_responsibility;
mod queue_context;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::WorkItemId;
pub use card_funds_command::{
    CardFundsCommandIdentityError, CardFundsCommandLock, CardFundsCommandSubject, CardFundsReviewKind,
};
pub use due::{WorkItemDueFilter, WorkItemDueWindow};
pub use entity::{
    ApprovalDecisionTaskError, ApprovalRuntimeTaskEnding, AssignmentSource, AvailableWorkItemAccount,
    DocumentApprovalWorkItemData, WorkItem, WorkItemAssignmentSeparationPolicy, WorkItemBriefObjectKind,
    WorkItemBriefRelation, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemStatus,
    WorkItemSubjectVersions, WorkItemType,
};
pub use finance_responsibility::{
    FinanceResponsibilityOperation, FinanceResponsibilityRule, FinanceResponsibilityRuleData,
    FinanceResponsibilityRuleSet, FinanceResponsibilityScope,
};
pub use finance_task::{
    card_funds_impact_summary, card_funds_task_kind, is_purchase_payable, is_zero_amount,
    matches_card_funds_identity, matches_sales_invoice_identity, matches_supplier_payment_identity,
    new_card_funds_task, new_sales_invoice_task, new_supplier_payment_task, payment_due_at,
    sales_invoice_impact_summary, supplier_payment_impact_summary, CardFundsTaskKind, CardFundsTaskSpec,
    SalesInvoiceTaskReason, SalesInvoiceTaskSpec, SupplierPaymentTaskReason, SupplierPaymentTaskSpec,
    FINANCE_OWNER_ROLE, PAYABLE_OBJECT_TYPE, RECEIVABLE_OBJECT_TYPE,
};
pub use fulfillment_responsibility::FulfillmentResponsibilityKey;
pub use queue_context::{QueueContextField, QueueContextIdentity};
