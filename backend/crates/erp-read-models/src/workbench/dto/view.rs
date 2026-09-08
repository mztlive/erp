use std::ops::{Deref, DerefMut};

use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use serde::Serialize;

use super::super::brief::assemble_brief;
use super::super::presentation::{
    next_action_hint, reason_label, usable_impact_summary, UNRESOLVED_OWNER_DISPLAY_NAME,
};
use super::status::{ProcessingBlockerView, ProcessingState, WorkItemAllowedAction};
use crate::errors::{Error, Result};

pub use erp_workflow::dto::work_item::{WorkItemApprovalContextView, WorkItemPartyView};

/// 事项简报中的只读键值。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemSummarySection {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric: Option<bool>,
    /// 可跳转关联单据的稳定身份；仅作路由键，不上屏。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
}

/// 事项简报中的一行业务明细。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemBriefLine {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_label: Option<String>,
}

/// 受控路由上下文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemRouteContext {
    /// 导入确认范围；不适用时为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_scope: Option<String>,
    /// 单据审批的 DocumentType 稳定代码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type: Option<String>,
}

/// 人工任务队列安全投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemView {
    pub id: String,
    pub work_item_type: WorkItemType,
    pub handler_key: String,
    pub destination_workspace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_context: Option<WorkItemRouteContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<String>,
    /// 由审批节点执行和有界实例投影补齐的判断上下文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_context: Option<WorkItemApprovalContextView>,
    pub status: WorkItemStatus,
    pub assignment_source: AssignmentSource,
    pub owner_role: String,
    pub owner_role_label: String,
    pub owner_organization_id: String,
    pub owner_organization: WorkItemPartyView,
    pub owner_user_id: Option<String>,
    pub owner_user: Option<WorkItemPartyView>,
    pub processing_state: ProcessingState,
    pub processing_blocker: Option<ProcessingBlockerView>,
    pub business_object_type: String,
    pub business_object_id: String,
    /// 业务对象所属的工作面根对象；与任务对象相同时返回同一 ID。
    pub root_business_object_id: String,
    pub business_object_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty_label: Option<String>,
    pub next_action_hint: String,
    pub summary_sections: Vec<WorkItemSummarySection>,
    pub brief_lines: Vec<WorkItemBriefLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief_more_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_summary: Option<String>,
    pub subject_version: String,
    pub task_version: String,
    pub allowed_actions: Vec<WorkItemAllowedAction>,
    pub action_blockers: Vec<ProcessingBlockerView>,
    pub priority: WorkItemPriority,
    pub due_at: Option<u64>,
    pub reason_code: Option<String>,
    pub reason_label: String,
    pub impact_summary: String,
    pub assigned_at: Option<u64>,
    pub started_at: Option<u64>,
    pub current_assignment_at: Option<u64>,
    pub last_activity_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub completed_by: Option<String>,
    pub closed_at: Option<u64>,
    pub closed_by: Option<String>,
    pub close_reason: Option<String>,
    pub created_at: u64,
    pub queue_context_id: String,
}

impl WorkItemView {
    /// 设置服务端完成的处理状态与动作判断。
    ///
    /// # 返回
    /// 返回更新后的投影。
    pub(crate) fn with_access(
        mut self,
        processing_state: ProcessingState,
        processing_blocker: Option<ProcessingBlockerView>,
        allowed_actions: Vec<WorkItemAllowedAction>,
        action_blockers: Vec<ProcessingBlockerView>,
    ) -> Self {
        self.processing_state = processing_state;
        self.processing_blocker = processing_blocker;
        self.allowed_actions = allowed_actions;
        self.action_blockers = action_blockers;
        self
    }

    /// 设置与本任务审批节点严格绑定的运行上下文。
    ///
    /// # 参数
    /// * `approval_context` - 服务层按节点执行批量解析的有界运行事实
    ///
    /// # 返回
    /// 无。
    pub(crate) fn set_approval_context(&mut self, approval_context: WorkItemApprovalContextView) {
        self.approval_context = Some(approval_context);
    }

    /// 由已授权字段生成队列安全投影。
    ///
    /// # 参数
    /// * `fields` - 已通过对象授权的任务字段
    /// * `queue_context_id` - 当前队列上下文
    ///
    /// # 返回
    /// 返回原因、影响和下一步均已翻译成业务语言的投影；处理人姓名仍可能是占位，由服务层补齐。
    ///
    /// # 错误
    /// DocumentApproval 缺少已签署页面映射时返回错误。
    pub(crate) fn from_fields(fields: WorkItemFields, queue_context_id: String) -> Result<Self> {
        let WorkItemFields { inner, brief_source } = fields;
        let route = handler_route(
            inner.work_item_type,
            &inner.business_object_type,
            &inner.owner_role,
        )?;
        let owner_user = inner.owner_user_id.as_ref().map(|id| WorkItemPartyView {
            id: id.clone(),
            display_name: UNRESOLVED_OWNER_DISPLAY_NAME.to_string(),
        });
        let brief = brief_source
            .as_ref()
            .map(|source| assemble_brief(source, inner.reason_code.as_deref()));
        Ok(Self {
            id: inner.id,
            work_item_type: inner.work_item_type,
            handler_key: route.handler_key.to_string(),
            destination_workspace_id: route.destination_workspace_id.to_string(),
            route_context: route.route_context,
            approval_node_execution_id: inner.approval_node_execution_id,
            approval_context: None,
            status: inner.status,
            assignment_source: inner.assignment_source,
            owner_role_label: role_label(&inner.owner_role),
            owner_role: inner.owner_role,
            owner_organization: WorkItemPartyView {
                id: inner.owner_organization_id.clone(),
                display_name: "责任组织".to_string(),
            },
            owner_organization_id: inner.owner_organization_id,
            owner_user_id: inner.owner_user_id,
            owner_user,
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            business_object_label: inner.business_object_label,
            counterparty_label: inner.counterparty_label,
            next_action_hint: next_action_hint(inner.work_item_type),
            summary_sections: brief
                .as_ref()
                .map(|assembled| {
                    assembled
                        .sections
                        .iter()
                        .map(|section| WorkItemSummarySection {
                            label: section.label.clone(),
                            value: section.value.clone(),
                            numeric: section.numeric.then_some(true),
                            object_id: section.object_id.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            brief_lines: brief
                .as_ref()
                .map(|assembled| {
                    assembled
                        .lines
                        .iter()
                        .map(|line| WorkItemBriefLine {
                            title: line.title.clone(),
                            quantity: line.quantity.clone(),
                            due_label: line.due_label.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            brief_more_count: brief
                .as_ref()
                .map(|assembled| assembled.more_count)
                .filter(|count| *count > 0),
            list_summary: brief
                .as_ref()
                .map(|assembled| assembled.list_summary.clone())
                .filter(|text| !text.trim().is_empty()),
            business_object_type: inner.business_object_type,
            business_object_id: inner.business_object_id,
            root_business_object_id: inner.root_business_object_id,
            subject_version: inner.subject_version,
            task_version: inner.task_version.to_string(),
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            priority: inner.priority,
            due_at: seconds(inner.due_at),
            reason_label: reason_label(inner.reason_code.as_deref(), inner.work_item_type),
            reason_code: inner.reason_code,
            impact_summary: usable_impact_summary(inner.impact_summary.as_deref(), inner.work_item_type),
            assigned_at: seconds(inner.assigned_at),
            started_at: seconds(inner.started_at),
            current_assignment_at: seconds(inner.current_assignment_at),
            last_activity_at: seconds(inner.last_activity_at),
            completed_at: seconds(inner.completed_at),
            completed_by: inner.completed_by,
            closed_at: seconds(inner.closed_at),
            closed_by: inner.closed_by,
            close_reason: inner.close_reason,
            created_at: inner.created_at,
            queue_context_id,
        })
    }
}

/// Authorized work-item projection plus workbench brief source.
///
/// Identity and status fields are [`erp_workflow::dto::work_item::WorkItemFields`].
#[derive(Debug, Clone)]
pub(crate) struct WorkItemFields {
    inner: erp_workflow::dto::work_item::WorkItemFields,
    pub brief_source: Option<super::super::brief::ObjectBriefSource>,
}

impl Deref for WorkItemFields {
    type Target = erp_workflow::dto::work_item::WorkItemFields;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for WorkItemFields {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<WorkItem> for WorkItemFields {
    fn from(item: WorkItem) -> Self {
        Self {
            inner: item.into(),
            brief_source: None,
        }
    }
}

impl From<erp_workflow::WorkItemRow> for WorkItemFields {
    fn from(item: erp_workflow::WorkItemRow) -> Self {
        Self {
            inner: item.into(),
            brief_source: None,
        }
    }
}

/// 从工作台唯一任务路由规则投影处理器和目标工作面。
///
/// 保留该规则对任务类型、业务对象和责任角色的验证及失败分类。
pub fn work_item_destination(
    work_item_type: WorkItemType,
    business_object_type: &str,
    owner_role: &str,
) -> Result<(&'static str, &'static str)> {
    let route = handler_route(work_item_type, business_object_type, owner_role)?;
    Ok((route.handler_key, route.destination_workspace_id))
}

pub(super) struct HandlerRoute {
    pub(super) handler_key: &'static str,
    pub(super) destination_workspace_id: &'static str,
    pub(super) route_context: Option<WorkItemRouteContext>,
}

pub(super) fn handler_route(
    work_item_type: WorkItemType,
    business_object_type: &str,
    owner_role: &str,
) -> Result<HandlerRoute> {
    let (handler_key, destination_workspace_id) = match (work_item_type, business_object_type) {
        (
            WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException,
            "SUPPLIER_FULFILLMENT_ORDER",
        ) => ("supplier_fulfillment_investigation", "W26"),
        (WorkItemType::BusinessException, "SUPPLIER_OFFERING") => ("supplier_supply_exception", "W21"),
        (WorkItemType::IntegrationResultUnknown, "integration_error_task") => ("integration_unknown", "W29"),
        (WorkItemType::BusinessException, "integration_error_task" | "reconciliation_difference") => {
            ("business_exception", "W29")
        }
        (WorkItemType::IntegrationResultUnknown, "reconciliation_difference") => {
            ("integration_unknown", "W29")
        }
        (WorkItemType::ProcurementOrderCreation, "sales_order") => ("procurement_order_creation", "W08"),
        (WorkItemType::ProcurementOrderCreation, _) => {
            return Err(Error::ValidationError("供给分配任务业务对象未注册".to_string()));
        }
        (
            WorkItemType::FulfillmentOperation,
            "purchase_receipt" | "delivery" | "electronic_delivery" | "service_fulfillment",
        ) => ("fulfillment_operation", "W01"),
        (WorkItemType::FulfillmentOperation, _) => {
            return Err(Error::ValidationError("履约任务业务对象未注册".to_string()));
        }
        (WorkItemType::CustomerAcceptanceRegistration, "sales_order") => {
            ("customer_acceptance_registration", "W06")
        }
        (WorkItemType::CustomerAcceptanceRegistration, _) => {
            return Err(Error::ValidationError("客户验收任务业务对象未注册".to_string()));
        }
        (WorkItemType::SupplierPaymentExecution, "payable_account") => ("supplier_payment_execution", "W12"),
        (WorkItemType::SupplierPaymentExecution, _) => {
            return Err(Error::ValidationError("付款执行任务业务对象未注册".to_string()));
        }
        (WorkItemType::SalesInvoiceExecution, "receivable_account") => ("sales_invoice_execution", "W11"),
        (WorkItemType::SalesInvoiceExecution, _) => {
            return Err(Error::ValidationError(
                "销项开票执行任务业务对象未注册".to_string(),
            ));
        }
        (WorkItemType::SupplierSettlementReview, "supplier_settlement_statement") => {
            ("supplier_settlement", "W27")
        }
        (WorkItemType::ImportBusinessConfirmation, "LEGACY_IMPORT_BATCH") => {
            ("import_business_confirmation", "W18")
        }
        (
            WorkItemType::PurchaseOrderReview
            | WorkItemType::SalesChangeImpactReview
            | WorkItemType::SalesChangeFinanceReview
            | WorkItemType::OwnershipMigrationSalesConfirmation
            | WorkItemType::OwnershipMigrationFinanceConfirmation
            | WorkItemType::InventoryAdjustmentReview
            | WorkItemType::FinanceCorrectionReview,
            _,
        ) => {
            return Err(Error::ValidationError("WORK_ITEM_TYPE_RETIRED".to_string()));
        }
        (
            WorkItemType::CardFundsReview
            | WorkItemType::CardFundsDeltaReview
            | WorkItemType::SupplierSettlementReview
            | WorkItemType::ImportBusinessConfirmation,
            _,
        ) => {
            return Err(Error::ValidationError("WORK_ITEM_HANDLER_UNMAPPED".to_string()));
        }
        (WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException, _) => {
            return Err(Error::ValidationError("WORK_ITEM_HANDLER_UNMAPPED".to_string()));
        }
        (WorkItemType::DocumentApproval, object_type) => document_approval_route(object_type)?,
    };
    let mut route_context = if work_item_type == WorkItemType::ImportBusinessConfirmation {
        let scope = w18_confirmation_scope(owner_role)
            .ok_or_else(|| Error::ValidationError("IMPORT_CONFIRMATION_SCOPE_UNMAPPED".to_string()))?;
        Some(WorkItemRouteContext {
            confirmation_scope: Some(scope.to_string()),
            document_type: None,
        })
    } else {
        None
    };
    if work_item_type == WorkItemType::DocumentApproval {
        route_context = Some(WorkItemRouteContext {
            confirmation_scope: None,
            document_type: Some(business_object_type.to_string()),
        });
    }
    Ok(HandlerRoute {
        handler_key,
        destination_workspace_id,
        route_context,
    })
}

fn w18_confirmation_scope(owner_role: &str) -> Option<&'static str> {
    match owner_role {
        "role-sales" => Some("SALES"),
        "role-procurement" => Some("PROCUREMENT"),
        "role-operations" => Some("OPERATIONS"),
        "role-warehouse" => Some("WAREHOUSE"),
        "role-finance" => Some("FINANCE"),
        _ => None,
    }
}

/// 按已签署页面映射单据审批目标工作面。缺少映射失败关闭，不得回落 W05。
///
/// # 参数
/// * `business_object_type` - WorkItem 中的 DocumentType 稳定代码
///
/// # 返回
/// 返回 handler 与目标 workspace。
///
/// # 错误
/// 未签署映射时返回稳定错误，不得回落默认工作面。
fn document_approval_route(business_object_type: &str) -> Result<(&'static str, &'static str)> {
    match business_object_type {
        "sales_order" | "voucher_sales_order" | "sales_change_order" => Ok(("document_approval", "W05")),
        "purchase_order" | "purchase_change_order" => Ok(("document_approval", "W08")),
        "stock_adjustment" => Ok(("document_approval", "W10")),
        "customer_receipt" | "customer_refund" | "receipt_reversal" => Ok(("document_approval", "W11")),
        "supplier_refund" | "payment_reversal" => Ok(("document_approval", "W12")),
        _ => Err(Error::ValidationError(
            "APPROVAL_DOCUMENT_ROUTE_UNMAPPED".to_string(),
        )),
    }
}

fn seconds(value: Option<erp_core::common::time::Instant>) -> Option<u64> {
    value.and_then(|instant| u64::try_from(instant.unix_secs()).ok())
}

/// 把责任角色码翻译成界面文案。
///
/// 组织角色与审批责任角色（`approval/policy.rs` 的 `owner_role`）都会落到 `owner_role`
/// 字段。未覆盖的码不得原样上屏——界面禁止展示实现标识符，回退到通用「责任人」。
pub(super) fn role_label(role: &str) -> String {
    match role {
        "role-sales" | "sales" => "销售",
        "sales_order_owner" => "负责销售",
        "role-sales-leader" | "sales_leader" => "销售领导",
        "role-procurement" | "procurement" => "采购",
        "role-operations" | "operations" => "运营",
        "role-finance" | "finance" => "财务",
        "role-management" | "management" => "管理层",
        "sales_order_approver" => "销售单审批人",
        "voucher_sales_order_approver" => "卡券销售单审批人",
        "sales_change_order_approver" => "销售变更单审批人",
        "purchase_order_approver" => "采购单审批人",
        "purchase_change_order_approver" => "采购变更单审批人",
        "stock_adjustment_approver" => "库存调整单审批人",
        "customer_receipt_approver" => "回款复核人",
        "customer_refund_approver" => "客户退款审批人",
        "supplier_refund_approver" => "供应商退款审批人",
        "receipt_reversal_approver" => "回款冲正审批人",
        "payment_reversal_approver" => "付款冲正审批人",
        "approver" => "审批人",
        _ => "责任人",
    }
    .to_string()
}
